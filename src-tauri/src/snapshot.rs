//! 界面要显示的一切：**四种状态的判定、地址、版本**。
//!
//! 判定放在 Rust 侧而不是前端，是因为「已安装未运行」与「未安装但前台在跑」
//! 这两种状态**看起来一模一样**（都是连不上 / 都连着），只有把
//! 服务管理器 + 控制面两个来源放在一起才能分开。这段逻辑必须能单测。

use peon_burrow_ipc_types::{ProcessStatus, ServiceStatus};
use serde::Serialize;

/// 「现在是什么状态」—— 界面主文案就由它决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// 已注册为服务 + 控制面可连。
    Running,
    /// 已注册为服务，但控制面连不上。
    InstalledStopped,
    /// 没注册为服务，但控制面可连（有人在终端里跑着）。
    Foreground,
    /// 没注册 + 控制面连不上。
    NotInstalled,
    /// sidecar 不在（安装包损坏 / 被删）。
    Incomplete,
}

/// 地址是从哪来的（界面上要如实说明，别让人以为一定读到了配置）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UrlSource {
    /// 控制面报的（最准）。
    ControlPlane,
    /// 从 `relay.toml` 里读的。
    ConfigFile,
    /// 内置默认值（连配置文件都没有）。
    Default,
}

/// 发现文件的状态（三态，见 `discovery`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiscoveryState {
    /// 文件不存在。
    Missing,
    /// 文件在，但进程没了。
    Stale,
    /// 文件可用。
    Fresh,
}

/// 一次状态快照（前端一次 `invoke('snapshot')` 就拿到全部）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 状态组合（界面主文案）。
    pub phase: Phase,
    /// 服务管理器看到的状态（来自 sidecar 的 `service status --json`）。
    pub service: ServiceStatus,
    /// 控制面看到的进程状态（`None` = 连不上）。
    pub process: Option<ProcessStatus>,
    /// 控制面连不上的原因（给诊断用，不是给主界面弹窗用的）。
    pub control_error: Option<String>,
    /// 发现文件的三态。
    pub discovery: DiscoveryState,
    /// 发现文件三态的可读说明。
    pub discovery_detail: String,
    /// 扩展里该填的地址。
    pub relay_url: String,
    /// 地址的来源。
    pub url_source: UrlSource,
    /// sidecar（中继二进制）在不在。
    pub sidecar_present: bool,
    /// 桌面端版本。
    pub app_version: String,
    /// 本安装包绑定的 core 版本（`core-version.txt`）。
    pub core_bound: String,
}

impl Snapshot {
    /// 判定四种组合。
    ///
    /// ⚠️ 判据只能是**服务管理器**说的「注册了吗」+**控制面**说的「活着吗」，
    /// 不能靠「端口连不上」推断：前台运行（未注册）时端口是通的。
    pub fn phase_of(
        service: &ServiceStatus,
        process: Option<&ProcessStatus>,
        sidecar_present: bool,
    ) -> Phase {
        if !sidecar_present {
            return Phase::Incomplete;
        }
        match (service.installed, process.is_some()) {
            (true, true) => Phase::Running,
            (true, false) => Phase::InstalledStopped,
            (false, true) => Phase::Foreground,
            (false, false) => Phase::NotInstalled,
        }
    }

    /// 卡片副标题的**数据来源**（文案在前端 zh-CN 里，这里只给结构化信息）。
    pub fn status_code(&self) -> i32 {
        match self.phase {
            Phase::Running => 0,
            Phase::InstalledStopped => 1,
            Phase::Foreground => 2,
            Phase::NotInstalled => 3,
            Phase::Incomplete => 4,
        }
    }
}

/// 拼扩展地址。
///
/// IPv6 要加方括号（`ws://[::1]:41316/`），否则是无效 URL。
pub fn relay_url(host: &str, port: u16) -> String {
    let host = host.trim();
    let shown = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    format!("ws://{shown}:{port}/")
}

/// 默认监听地址（与 core 的 `RelayOptions::default()` 一致；界面上标成「默认值」）。
pub const DEFAULT_HOST: &str = "127.0.0.1";
/// 默认端口（core 的 `adr-0004`：41316）。
pub const DEFAULT_PORT: u16 = 41316;

#[cfg(test)]
mod tests {
    use super::*;
    use peon_burrow_ipc_types::ServiceLevel;

    fn installed_service() -> ServiceStatus {
        ServiceStatus {
            installed: true,
            running: true,
            level: Some(ServiceLevel::User),
            autostart: Some(peon_burrow_ipc_types::Autostart::Logon),
            name: "peon-burrow".to_owned(),
            binary_path: None,
            requires_elevation: false,
            restart_policy_configured: true,
            last_exit_code: None,
        }
    }

    fn process() -> ProcessStatus {
        ProcessStatus {
            running: true,
            host: "127.0.0.1".to_owned(),
            port: 41316,
            version: "0.1.0".to_owned(),
            protocol: 1,
            started_at: "2026-10-09T00:00:00Z".to_owned(),
            connections: 1,
            watch_connections: 1,
            last_error: None,
        }
    }

    #[test]
    fn the_four_combinations_are_distinguished() {
        let installed = installed_service();
        let not_installed = ServiceStatus::not_installed("peon-burrow");
        let process = process();

        // ① 已安装 + 可连
        assert_eq!(
            Snapshot::phase_of(&installed, Some(&process), true),
            Phase::Running
        );
        // ② 已安装 + 连不上（用户最容易误判成「坏了」的那种）
        assert_eq!(
            Snapshot::phase_of(&installed, None, true),
            Phase::InstalledStopped
        );
        // ③ 未安装 + 可连（终端里跑着）—— 与②看起来一样，但判定必须不同
        assert_eq!(
            Snapshot::phase_of(&not_installed, Some(&process), true),
            Phase::Foreground
        );
        // ④ 未安装 + 连不上
        assert_eq!(
            Snapshot::phase_of(&not_installed, None, true),
            Phase::NotInstalled
        );
    }

    #[test]
    fn a_missing_sidecar_beats_everything_else() {
        // 安装包损坏时，界面要说「安装不完整」，而不是「未安装」
        assert_eq!(
            Snapshot::phase_of(&installed_service(), Some(&process()), false),
            Phase::Incomplete
        );
        assert_eq!(
            Snapshot::phase_of(&ServiceStatus::not_installed("x"), None, false),
            Phase::Incomplete
        );
    }

    #[test]
    fn the_four_states_have_distinct_codes() {
        // 前端按 code 取文案；重复的 code 会让两种状态显示同一句话
        let phases = [
            Phase::Running,
            Phase::InstalledStopped,
            Phase::Foreground,
            Phase::NotInstalled,
            Phase::Incomplete,
        ];
        let mut codes: Vec<i32> = Vec::new();
        for phase in phases {
            let snapshot = Snapshot {
                phase,
                service: ServiceStatus::not_installed("peon-burrow"),
                process: None,
                control_error: None,
                discovery: DiscoveryState::Missing,
                discovery_detail: String::new(),
                relay_url: relay_url(DEFAULT_HOST, DEFAULT_PORT),
                url_source: UrlSource::Default,
                sidecar_present: true,
                app_version: "0.1.0".to_owned(),
                core_bound: "v0.0.0".to_owned(),
            };
            assert!(
                !codes.contains(&snapshot.status_code()),
                "code 重复：{phase:?}"
            );
            codes.push(snapshot.status_code());
        }
    }

    #[test]
    fn relay_urls_are_valid_for_ipv4_ipv6_and_defaults() {
        assert_eq!(relay_url("127.0.0.1", 41316), "ws://127.0.0.1:41316/");
        assert_eq!(relay_url("::1", 41316), "ws://[::1]:41316/");
        assert_eq!(relay_url("[::1]", 41316), "ws://[::1]:41316/");
        assert_eq!(relay_url("localhost", 8080), "ws://localhost:8080/");
        // 与 core 的默认值一致（端口 41316）
        assert_eq!(DEFAULT_PORT, 41316);
    }
}
