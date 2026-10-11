//! 找控制面：定位发现文件、读它、判断它是不是陈旧的。
//!
//! ⚠️ **发现文件不是权威**，只是缓存。权威顺序是：
//! 控制面（能连上吗）→ 服务管理器（注册了吗）→ 发现文件（只剩地址与 token）。
//! 所以这里的产物只用来「试着连一下」，任何结论都要由连接结果或服务管理器来定。
//!
//! # 「陈旧」什么时候判（H-1）
//!
//! 读文件是**每 2 秒**都跑的事（`commands::heartbeat`），而「那个 pid 还在吗」是一次
//! 进程调用（Windows 上是 `tasklist`，实测 126–159ms）。所以两者必须分开：
//!
//! | 做什么 | 什么时候 | 代价 |
//! | --- | --- | --- |
//! | [`load`]：读文件、解析出 endpoint | 每次心跳（2 秒） | 一次本地文件读 |
//! | [`pid_gone`]：文件里记的进程还在吗 | **控制面连不上之后**（失败路径）+ TTL 缓存 | 一次进程调用 / 10 秒 |
//!
//! 取舍：`DiscoveryState::Stale` 的判定**变晚**了（先按文件报 Fresh，连不上之后才转 Stale），
//! 换来的是 2 秒轮询里不再有进程调用；顺带修掉一个更隐蔽的问题 —— 从前 pid 判定在
//! `load()` 里，判成 Stale 就**不再尝试连接**，于是一次误判（`tasklist` 抽风）
//! 会让界面在真正活着的中继上写「发现文件是旧的」。
//!
//! 路径必须与 core 的 `Paths::discover()` **逐字节一致**（`peon-burrow` 仓库
//! `crates/peon-burrow/src/paths.rs`）：两边都从 `directories::ProjectDirs` 推，
//! 且 Windows 上 `data_local_dir()` / `config_dir()` 会带 `data` / `config` 子目录。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use peon_burrow_ipc::{read_endpoint, ControlEndpoint};

/// 发现文件（`control.json`）的位置。
pub fn control_file() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_local_dir().join("control.json"))
}

/// 数据目录（日志在它下面）。
pub fn data_dir() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.data_local_dir().to_path_buf())
}

/// 日志目录。
pub fn log_dir() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("logs"))
}

/// 中继的配置文件（`relay.toml`）—— `open_config` 会打开它。
pub fn config_file() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.config_dir().join("relay.toml"))
}

/// 唯一的 `ProjectDirs` 调用点（布局铁律 L4：注入而非全局）。
fn project_dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "peon-burrow")
}

/// 发现文件的两态（**不含新鲜度判定**：那个要等控制面连不上之后才做，见文件头）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// 文件不存在 → 服务从没跑过，或者刚被停掉。
    Missing,
    /// 文件在、能解析：拿 `endpoint` 去连控制面。
    ///
    /// `pid` 只用于**事后**判断陈旧（[`pid_gone`]），不在这里判 —— 见文件头的取舍。
    Fresh {
        /// 控制面地址（连接用）。
        endpoint: Box<ControlEndpoint>,
        /// 文件里记的 pid（可选：老版本写的文件里没有它）。
        pid: Option<u32>,
    },
}

impl Discovery {
    /// 给界面/诊断用的说明。
    ///
    /// ⚠️ 这里只说「文件本身」的情况；「陈旧」那句话由 [`stale_detail`] 给
    /// （它要说出是哪个 pid 没了，而那是判定之后才知道的事）。
    pub fn describe(&self) -> String {
        match self {
            Self::Missing => "找不到发现文件：中继没有在运行（或从没装过）".to_owned(),
            Self::Fresh { endpoint, .. } => format!("发现文件可用：{}", endpoint.address),
        }
    }
}

/// 「文件是旧的」那句话（判定陈旧之后用它填 `control_error` 与 `discovery_detail`）。
pub fn stale_detail(pid: u32) -> String {
    format!("发现文件是旧的（它记的进程 {pid} 已经不在了）—— 中继可能在重启或已经被停掉")
}

/// 读发现文件（**不做进程判定**）。
///
/// 每次心跳都会调到它，所以这里**只做本地读取**（`read_endpoint` + `read_pid` 两次文件读），
/// 不做任何进程调用 —— 那是一次上百毫秒的开销，见文件头。
pub fn load() -> Discovery {
    match read_endpoint_at(control_file().as_deref()) {
        Err(()) => Discovery::Missing,
        Ok((endpoint, pid)) => Discovery::Fresh {
            endpoint: Box::new(endpoint),
            pid,
        },
    }
}

/// 读文件（含 pid）。`Err(())` = 没有文件或读不动。
fn read_endpoint_at(path: Option<&Path>) -> Result<(ControlEndpoint, Option<u32>), ()> {
    let path = path.ok_or(())?;
    let endpoint = read_endpoint(path).map_err(|_| ())?;
    // pid 是可选字段：老版本写的文件里没有它（`peon-burrow-ipc-types` 里
    // `#[serde(default)]`），那就只当「不知道」，不做陈旧判断
    let pid = read_pid(path);
    Ok((endpoint, pid))
}

/// 从文件里再读一次 pid（发现文件的 JSON 里带 `pid` 字段）。
fn read_pid(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    value
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .map(|pid| pid as u32)
}

/// 文件里记的那个进程**已经不在了**吗。
///
/// ⚠️ 只在「控制面连不上」这条**失败路径**上调用（`commands::heartbeat`）：
/// 控制面才是权威，发现文件只是缓存（见文件头）。判定结果带 TTL 缓存，
/// 因为这条路在「服务停着 + 留下一个崩溃残留的文件」时会每 2 秒都被走到。
pub fn pid_gone(pid: u32) -> bool {
    !pid_alive_cached(pid)
}

/// 同一份判定复用多久。
///
/// 「哪个 pid 还在不在」在 10 秒里不会变 —— 真变了（新实例起来）也会**重写发现文件**，
/// 于是 pid 跟着变，缓存自然失效（缓存按 pid 匹配）。
const PID_CACHE_TTL: Duration = Duration::from_secs(10);

/// 一次进程判定的结果（缓存条目）。
#[derive(Debug, Clone, Copy)]
struct PidVerdict {
    /// 这是哪个 pid 的判定。
    pid: u32,
    /// 它当时还活着吗。
    alive: bool,
    /// 什么时候查的。
    checked: Instant,
}

/// 进程判定的缓存。
///
/// C5-EXCEPTION：这是**同步**代码（本模块没有 async 函数），持锁期间只读/写一个 `Copy`
/// 的小结构，不做 IO、不跨 `await`；真正的子进程调用一律在**锁外**做（见
/// [`pid_alive_cached`]），所以不会把别的线程卡在一次 `tasklist` 上。
static PID_CACHE: Mutex<Option<PidVerdict>> = Mutex::new(None);

/// 带缓存的存活判定。
fn pid_alive_cached(pid: u32) -> bool {
    let now = Instant::now();
    let cached = PID_CACHE
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .and_then(|verdict| cached_verdict(verdict, pid, now));
    if let Some(alive) = cached {
        return alive;
    }

    // ⚠️ 子进程调用在**锁外**：它可能上百毫秒，不该卡住别的线程
    let alive = pid_alive(pid);
    if let Ok(mut guard) = PID_CACHE.lock() {
        *guard = Some(PidVerdict {
            pid,
            alive,
            checked: Instant::now(),
        });
    }
    alive
}

/// 能不能复用上次的判定（**纯函数**，便于单测）。
///
/// 判据是「同一个 pid」且「离上次查不到 [`PID_CACHE_TTL`]」。
fn cached_verdict(verdict: PidVerdict, pid: u32, now: Instant) -> Option<bool> {
    (verdict.pid == pid && now.duration_since(verdict.checked) < PID_CACHE_TTL)
        .then_some(verdict.alive)
}

/// 这个进程还活着吗（**每次都是一次进程调用**，别在热路径上直接用它）。
fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(target_os = "macos")]
    {
        // `kill -0` 不发送信号，只做权限/存在性检查
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    #[cfg(windows)]
    {
        // tasklist 过滤到具体 pid：输出里有这个 pid 才算活着
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output();
        match output {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                text.contains(&format!("\"{pid}\""))
            }
            // 查不出来就当作「活着」：宁可多试一次连接，也不要因为查不到而谎报「服务没跑」
            Err(_) => true,
        }
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paths_match_the_core_crate_layout() {
        // 与 core 的 paths.rs 保持一致：Windows 上 data_local_dir() 带 `data` 子目录
        let control = control_file().expect("本机应当能定位用户目录");
        assert!(control.ends_with("control.json"), "{control:?}");
        assert!(
            control.parent().unwrap().ends_with("peon-burrow")
                || control.parent().unwrap().file_name().unwrap() == "data",
            "数据目录应当是 …/peon-burrow[/data]：{control:?}"
        );

        let config = config_file().expect("配置文件路径");
        assert!(config.ends_with("relay.toml"), "{config:?}");
    }

    #[test]
    fn a_missing_file_is_missing_not_an_error() {
        assert_eq!(read_endpoint_at(None), Err(()));
        assert!(matches!(Discovery::Missing.describe(), text if text.contains("找不到")));
    }

    #[test]
    fn a_stale_file_says_which_process_is_gone() {
        let described = stale_detail(4321);
        assert!(described.contains("4321"), "{described}");
        assert!(described.contains("旧的"));
    }

    #[test]
    fn pid_zero_is_never_alive() {
        assert!(!pid_alive(0));
        // 0 也走「不在了」这条路（缓存不该把它的结论搅混）
        assert!(pid_gone(0));
    }

    #[test]
    #[cfg(windows)]
    fn the_current_process_is_alive() {
        // 自己一定活着（这条同时验证 Windows 分支的解析逻辑没写歪，以及缓存不吞结论）
        assert!(!pid_gone(std::process::id()));
    }

    #[test]
    fn a_fresh_endpoint_is_described_by_its_address() {
        let discovery = Discovery::Fresh {
            endpoint: Box::new(ControlEndpoint::local_socket("peon-burrow", "t")),
            pid: None,
        };
        assert!(discovery.describe().contains("peon-burrow"));
    }

    #[test]
    fn reading_the_file_never_runs_a_process_check() {
        // H-1 的核心：`load()` 是每 2 秒都要跑的，它只能读文件。
        // 这里不断言「有没有 spawn」（那要看系统调用），而是断言**判定结果**：
        // 文件里记的 pid 明明早就没了，`load()` 仍然报 Fresh ——
        // 「陈旧」是控制面连不上之后才由 `pid_gone` 判的。
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("control.json");
        std::fs::write(
            &path,
            // 424242 这个 pid 不可能存在（远大于常见 pid 上限）
            r#"{"kind":"local-socket","address":"\\\\.\\pipe\\peon-burrow","token":"t","pid":424242}"#,
        )
        .expect("write");

        let discovery = read_endpoint_at(Some(&path)).expect("文件能读");
        assert_eq!(discovery.1, Some(424242), "pid 要读出来（后面判定要用）");
        // 判定确实说「不在了」——只是这一步不该在 `load()` 里做
        assert!(pid_gone(424242));

        drop(directory);
    }

    #[test]
    fn the_pid_cache_reuses_a_verdict_only_for_the_same_live_pid() {
        let now = Instant::now();
        let verdict = PidVerdict {
            pid: 100,
            alive: true,
            checked: now,
        };

        // 同一个 pid、还在 TTL 里 → 复用
        assert_eq!(
            cached_verdict(verdict, 100, now + Duration::from_secs(1)),
            Some(true)
        );
        // 换了 pid（新实例重写了文件）→ 不复用
        assert_eq!(
            cached_verdict(verdict, 200, now + Duration::from_secs(1)),
            None
        );
        // 过了 TTL → 不复用（pid 可能被复用）
        assert_eq!(
            cached_verdict(verdict, 100, now + PID_CACHE_TTL + Duration::from_secs(1)),
            None
        );
        // 「不在了」这个结论同样会被复用（失败路径上也是 10 秒一次进程调用）
        let dead = PidVerdict {
            pid: 100,
            alive: false,
            checked: now,
        };
        assert_eq!(
            cached_verdict(dead, 100, now + Duration::from_secs(1)),
            Some(false)
        );
    }
}
