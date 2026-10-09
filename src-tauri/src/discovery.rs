//! 找控制面：定位发现文件、读它、判断它是不是陈旧的。
//!
//! ⚠️ **发现文件不是权威**，只是缓存。权威顺序是：
//! 控制面（能连上吗）→ 服务管理器（注册了吗）→ 发现文件（只剩地址与 token）。
//! 所以这里的产物只用来「试着连一下」，任何结论都要由连接结果或服务管理器来定。
//!
//! 路径必须与 core 的 `Paths::discover()` **逐字节一致**（`peon-burrow` 仓库
//! `crates/peon-burrow/src/paths.rs`）：两边都从 `directories::ProjectDirs` 推，
//! 且 Windows 上 `data_local_dir()` / `config_dir()` 会带 `data` / `config` 子目录。

use std::path::{Path, PathBuf};

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

/// 发现文件的三态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Discovery {
    /// 文件不存在 → 服务从没跑过，或者刚被停掉。
    Missing,
    /// 文件在，但它记的进程已经没了 → 陈旧（界面要说明这一点，别让用户困惑）。
    Stale {
        /// 文件里记的 pid。
        pid: u32,
    },
    /// 文件看起来有效：可以用它去连控制面。
    Fresh(Box<ControlEndpoint>),
}

impl Discovery {
    /// 给界面/诊断用的说明。
    pub fn describe(&self) -> String {
        match self {
            Self::Missing => "找不到发现文件：中继没有在运行（或从没装过）".to_owned(),
            Self::Stale { pid } => format!(
                "发现文件是旧的（它记的进程 {pid} 已经不在了）—— 中继可能在重启或已经被停掉"
            ),
            Self::Fresh(endpoint) => format!("发现文件可用：{}", endpoint.address),
        }
    }
}

/// 读发现文件并判断新鲜度。
pub fn load() -> Discovery {
    let read = read_endpoint_at(control_file().as_deref());
    match read {
        Err(()) => Discovery::Missing,
        Ok((endpoint, pid)) => match pid {
            // pid 还在 → 直接用；pid 不在 → 陈旧（但界面仍然可以试着连一下，见 commands）
            Some(pid) if !pid_alive(pid) => Discovery::Stale { pid },
            _ => Discovery::Fresh(Box::new(endpoint)),
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

/// 这个进程还活着吗。
///
/// ⚠️ 只在「文件在、但控制面连不上」这条**失败路径**上调用 —— 它是一次进程调用，
/// 不该出现在每 2 秒的轮询里。
pub fn pid_alive(pid: u32) -> bool {
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
        let described = Discovery::Stale { pid: 4321 }.describe();
        assert!(described.contains("4321"), "{described}");
        assert!(described.contains("旧的"));
    }

    #[test]
    fn pid_zero_is_never_alive() {
        assert!(!pid_alive(0));
    }

    #[test]
    #[cfg(windows)]
    fn the_current_process_is_alive() {
        // 自己一定活着（这条同时验证 Windows 分支的解析逻辑没写歪）
        assert!(pid_alive(std::process::id()));
    }

    #[test]
    fn a_fresh_endpoint_is_described_by_its_address() {
        let discovery =
            Discovery::Fresh(Box::new(ControlEndpoint::local_socket("peon-burrow", "t")));
        assert!(discovery.describe().contains("peon-burrow"));
    }
}
