//! 调中继二进制的 `service` 子命令：读服务状态、安装/卸载/启停、开关自启。
//!
//! 为什么不让 Rust 直接调平台 API：那要写三套（SCM / launchd / systemd），
//! 而且逻辑与 core 的 `peon-burrow-service` **必然漂移**。这里只做两件事：
//! 拼参数、解析 JSON —— 两个都是纯函数，可单测。
//!
//! ⚠️ 参数必须与 core 的 CLI **逐字一致**（`peon-burrow` 仓库
//! `crates/peon-burrow/src/cli.rs` 的 `ServiceCommand`）。

use std::path::PathBuf;

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
///
/// ⚠️ 「停止」的免提权**只对控制面那条路成立**。控制面连不上时会降级成 `service stop`
/// （`sc stop`），那一步在系统服务下要管理员 —— 见 [`fallback_stop_needs_elevation`]
/// 与 `commands::service_action`。
pub fn needs_elevation(action: ServiceAction, level: ServiceLevel) -> bool {
    matches!(level, ServiceLevel::System) && !matches!(action, ServiceAction::Stop)
}

/// 降级路径（控制面停不下来 → 跑 `service stop`）要不要提权。
///
/// 单独写成函数是为了让这条语义**能单测**：系统服务下 `sc stop` 需要管理员，
/// 用户级下 `schtasks /End` / `systemctl --user stop` 不需要。
/// `ai-docs/01-ui-and-states.md § 3` 承诺过这条降级。
pub fn fallback_stop_needs_elevation(level: ServiceLevel) -> bool {
    matches!(level, ServiceLevel::System)
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

mod process;
pub use process::{display_line, run, run_at, run_streaming, silent_command, StreamKind};

#[cfg(test)]
mod tests;
