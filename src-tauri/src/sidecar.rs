//! 调中继二进制的 `service` 子命令：读服务状态、安装/卸载/启停、开关自启。
//!
//! 为什么不让 Rust 直接调平台 API：那要写三套（SCM / launchd / systemd），
//! 而且逻辑与 core 的 `peon-burrow-service` **必然漂移**。这里只做两件事：
//! 拼参数、解析 JSON —— 两个都是纯函数，可单测。
//!
//! ⚠️ 参数必须与 core 的 CLI **逐字一致**（`peon-burrow` 仓库
//! `crates/peon-burrow/src/cli.rs` 的 `ServiceCommand`）。

use std::path::{Path, PathBuf};
use std::process::Command;

use peon_burrow_ipc_types::{ServiceLevel, ServiceStatus};
use serde::Serialize;

/// 服务动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    /// 安装（注册 + 自启 + 启动）。
    Install,
    /// 卸载（先停）。
    Uninstall,
    /// 启动。
    Start,
    /// 停止。
    Stop,
    /// 重启。
    Restart,
}

impl ServiceAction {
    /// 从命令参数解析（前端传来的是字符串）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "install" => Some(Self::Install),
            "uninstall" => Some(Self::Uninstall),
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "restart" => Some(Self::Restart),
            _ => None,
        }
    }

    /// 规范写法。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Uninstall => "uninstall",
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

/// 中继二进制的位置：**与桌面端同一个目录**。
///
/// Tauri 的 `externalBin` 会把 `burrow[.exe]` 放在可执行文件旁边（安装目录里），
/// 所以这样找既能覆盖安装态，也能覆盖 `tauri dev`（开发时把二进制放进 `src-tauri/binaries/`
/// 之后它会被放到 target 目录旁边）。
///
/// 可用 `PEON_HALL_SIDECAR` 覆盖：集成测试（测试二进制在 `target/debug/deps/`，
/// 邻居不是安装目录）与「不想拷贝二进制」的本地调试都靠它。
pub fn sidecar_path() -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("PEON_HALL_SIDECAR") {
        let path = PathBuf::from(override_path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    Some(dir.join(binary_name()))
}

/// 平台上的中继可执行文件名。
pub fn binary_name() -> &'static str {
    if cfg!(windows) {
        "burrow.exe"
    } else {
        "burrow"
    }
}

/// 中继二进制在不在（不在 = 安装包损坏 / 用户手动删了）。
pub fn present() -> bool {
    sidecar_path().map(|path| path.is_file()).unwrap_or(false)
}

/// `service status --json`
pub fn status_args() -> Vec<String> {
    strings(["service", "status", "--json"])
}

/// 一次服务动作的参数。
///
/// 安装时**显式**给 `--mode`：默认级别来自桌面端的意图，不能让 core 自己猜。
pub fn action_args(action: ServiceAction, level: ServiceLevel, autostart: bool) -> Vec<String> {
    let level = level_arg(level);
    match action {
        ServiceAction::Install => {
            let mut args = strings(["service", "install", "--mode", level]);
            if !autostart {
                args.push("--no-autostart".to_owned());
            }
            args.push("--json".to_owned());
            args
        }
        ServiceAction::Uninstall => strings(["service", "uninstall", "--json"]),
        ServiceAction::Start => strings(["service", "start", "--json"]),
        ServiceAction::Stop => strings(["service", "stop", "--json"]),
        ServiceAction::Restart => strings(["service", "restart", "--json"]),
    }
}

/// `service autostart on|off --json`
pub fn autostart_args(on: bool) -> Vec<String> {
    strings([
        "service",
        "autostart",
        if on { "on" } else { "off" },
        "--json",
    ])
}

/// 这个动作需不需要提权。
///
/// 用户级**一切都不需要**（零 UAC 是主路径）；系统级只有「停止」可以走控制面免提权
/// （让服务自己停），其余都要经过提权辅助。
pub fn needs_elevation(action: ServiceAction, level: ServiceLevel) -> bool {
    matches!(level, ServiceLevel::System) && !matches!(action, ServiceAction::Stop)
}

/// `--mode` 的取值。
pub fn level_arg(level: ServiceLevel) -> &'static str {
    match level {
        ServiceLevel::User => "user",
        ServiceLevel::System => "system",
    }
}

/// 一次 sidecar 调用的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarOutput {
    /// 退出码。
    pub code: i32,
    /// 标准输出。
    pub stdout: String,
    /// 标准错误。
    pub stderr: String,
}

/// sidecar 相关的错误。
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    /// 找不到中继程序。
    #[error("安装不完整：找不到中继程序（{0}）。请重新安装 peon-hall")]
    Missing(String),
    /// 起不来。
    #[error("起不了中继程序：{0}")]
    Spawn(String),
    /// 命令失败。
    #[error("{0}")]
    Failed(String),
    /// 返回看不懂（多半是版本不匹配）。
    #[error("中继的返回看不懂（可能是版本不匹配，请更新 peon-hall）：{0}")]
    Unparsable(String),
}

/// 跑一次中继命令（**非提权**；提权走 [`crate::elevate`]）。
pub fn run(args: &[String]) -> Result<SidecarOutput, SidecarError> {
    let exe =
        sidecar_path().ok_or_else(|| SidecarError::Missing("定位不到可执行文件".to_owned()))?;
    run_at(&exe, args)
}

/// 在指定路径上跑（测试直接喂一个假 exe）。
pub fn run_at(exe: &Path, args: &[String]) -> Result<SidecarOutput, SidecarError> {
    if !exe.is_file() {
        return Err(SidecarError::Missing(exe.display().to_string()));
    }

    let output = Command::new(exe)
        .args(args)
        .output()
        .map_err(|error| SidecarError::Spawn(error.to_string()))?;

    Ok(SidecarOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// 解析 `service status --json`。
///
/// ⚠️ 解析失败要说「版本不匹配」，不能把空白界面丢给用户（`02-core-integration.md § 7` 第 4 条）。
pub fn parse_status(stdout: &str) -> Result<ServiceStatus, SidecarError> {
    serde_json::from_str(stdout.trim()).map_err(|error| SidecarError::Unparsable(error.to_string()))
}

/// 解析动作的 JSON 输出（`--json` 时 core 会打一份结果）。
pub fn parse_action(output: &SidecarOutput) -> Result<Option<serde_json::Value>, SidecarError> {
    if output.code != 0 {
        let message = if output.stderr.trim().is_empty() {
            // 提权路径拿不到子进程输出时会是这个形态；告诉用户怎么看到真正的原因
            format!(
                "中继返回退出码 {}，但它没有输出。在终端里手动跑一次同样的命令能看到完整原因",
                output.code
            )
        } else {
            output.stderr.trim().to_owned()
        };
        return Err(SidecarError::Failed(message));
    }
    let text = output.stdout.trim();
    if text.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => Ok(Some(value)),
        // 动作成功但输出不是 JSON：不算失败（有的命令只打人话），但也别丢信息
        Err(_) => Ok(None),
    }
}

/// 把 `&str` 数组变成 `Vec<String>`（参数拼接到处都是，收口成一个函数）。
fn strings<const N: usize>(items: [&str; N]) -> Vec<String> {
    items.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_args_match_the_core_cli() {
        // 必须与 core 的 `service install --mode <user|system> [--no-autostart] --json` 一致
        assert_eq!(
            action_args(ServiceAction::Install, ServiceLevel::User, true),
            vec!["service", "install", "--mode", "user", "--json"]
        );
        assert_eq!(
            action_args(ServiceAction::Install, ServiceLevel::System, false),
            vec![
                "service",
                "install",
                "--mode",
                "system",
                "--no-autostart",
                "--json"
            ]
        );
    }

    #[test]
    fn the_other_actions_are_exact() {
        assert_eq!(
            action_args(ServiceAction::Uninstall, ServiceLevel::User, true),
            vec!["service", "uninstall", "--json"]
        );
        assert_eq!(
            action_args(ServiceAction::Start, ServiceLevel::User, true),
            vec!["service", "start", "--json"]
        );
        assert_eq!(
            action_args(ServiceAction::Stop, ServiceLevel::User, true),
            vec!["service", "stop", "--json"]
        );
        assert_eq!(
            action_args(ServiceAction::Restart, ServiceLevel::User, true),
            vec!["service", "restart", "--json"]
        );
        assert_eq!(status_args(), vec!["service", "status", "--json"]);
        assert_eq!(
            autostart_args(false),
            vec!["service", "autostart", "off", "--json"]
        );
        assert_eq!(
            autostart_args(true),
            vec!["service", "autostart", "on", "--json"]
        );
    }

    #[test]
    fn only_system_level_actions_need_elevation() {
        use ServiceAction::*;
        for action in [Install, Uninstall, Start, Stop, Restart] {
            assert!(
                !needs_elevation(action, ServiceLevel::User),
                "用户级不该提权：{action:?}"
            );
        }
        assert!(needs_elevation(Install, ServiceLevel::System));
        assert!(needs_elevation(Uninstall, ServiceLevel::System));
        assert!(needs_elevation(Start, ServiceLevel::System));
        assert!(needs_elevation(Restart, ServiceLevel::System));
        // 停系统服务走控制面（让服务自己停），所以不需要提权
        assert!(!needs_elevation(Stop, ServiceLevel::System));
    }

    #[test]
    fn actions_round_trip_through_strings() {
        for action in [
            ServiceAction::Install,
            ServiceAction::Uninstall,
            ServiceAction::Start,
            ServiceAction::Stop,
            ServiceAction::Restart,
        ] {
            assert_eq!(ServiceAction::parse(action.as_str()), Some(action));
        }
        assert_eq!(ServiceAction::parse("format"), None);
    }

    #[test]
    fn status_json_parses_and_mismatches_are_named() {
        let json = r#"{
            "installed": true,
            "running": false,
            "level": "user",
            "autostart": "logon",
            "name": "peon-burrow",
            "binaryPath": "C:\\x\\burrow.exe",
            "requiresElevation": false,
            "restartPolicyConfigured": true,
            "lastExitCode": 4
        }"#;
        let status = parse_status(json).expect("应当能解析");
        assert!(status.installed);
        assert_eq!(status.last_exit_code, Some(4));

        // 字段被 core 改名 → 必须说「版本不匹配」，而不是空白
        let error = parse_status(r#"{"isInstalled":true}"#).expect_err("应当失败");
        let text = error.to_string();
        assert!(text.contains("版本不匹配"), "{text}");
    }

    #[test]
    fn a_failed_action_surfaces_stderr() {
        let output = SidecarOutput {
            code: 2,
            stdout: String::new(),
            stderr: "错误：配置里 max_connections 不合法".to_owned(),
        };
        let error = parse_action(&output).expect_err("应当失败");
        assert!(error.to_string().contains("max_connections"));
    }

    #[test]
    fn a_successful_action_may_print_nothing() {
        let output = SidecarOutput {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert_eq!(parse_action(&output).expect("成功"), None);

        let output = SidecarOutput {
            code: 0,
            stdout: "{\"installed\":true}".to_owned(),
            stderr: String::new(),
        };
        assert!(parse_action(&output).expect("成功").is_some());
    }

    #[test]
    fn a_missing_binary_is_reported_not_panicked() {
        let error = run_at(Path::new("/definitely/not/there/burrow"), &status_args())
            .expect_err("应当失败");
        assert!(matches!(error, SidecarError::Missing(_)));
        assert!(error.to_string().contains("安装不完整"));
    }
}
