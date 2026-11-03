use super::events::execute;
use super::{ActionResult, CommandError, ErrorCode};
use crate::control;
use crate::discovery::{self, Discovery};
use crate::sidecar::{self, ServiceAction};
use peon_burrow_ipc_types::{Request, ServiceLevel, ServiceStatus};
use tauri::AppHandle;

/// 问一次服务管理器（**重**：一次进程调用；界面每 5 次轮询查一次）。
#[tauri::command]
pub async fn service_status(app: AppHandle) -> Result<ServiceStatus, CommandError> {
    if !sidecar::present() {
        return Ok(ServiceStatus::not_installed("peon-burrow"));
    }

    let output = execute(app, sidecar::status_args(), false, "auto")
        .await?
        .ok_or_else(|| CommandError::new(ErrorCode::Failed, "状态查询被取消"))?;

    sidecar::parse_status(&output.stdout).map_err(CommandError::from_sidecar)
}

/// 五个操作：安装 / 卸载 / 启动 / 停止 / 重启。
#[tauri::command]
pub async fn service_action(
    app: AppHandle,
    action: String,
    mode: Option<String>,
    autostart: Option<bool>,
) -> Result<ActionResult, CommandError> {
    let action = ServiceAction::parse(&action)
        .ok_or_else(|| CommandError::new(ErrorCode::Failed, format!("不认识的操作：{action}")))?;

    if !sidecar::present() {
        return Err(CommandError::new(
            ErrorCode::SidecarMissing,
            "安装不完整：缺少中继程序。请重新安装 peon-hall",
        ));
    }

    // 「停止」优先走控制面：让服务自己停，**不需要提权**（系统服务也一样，见
    // `ai-docs/01-ui-and-states.md § 3`）。走到下面那段的停止都是**降级**：
    // 控制面连不上 → `service stop`，而系统服务下那一步要管理员（`sc stop`）。
    // ⚠️ 从前没有这条降级，于是系统服务在控制面不可连时点【停止】只会拿到
    // 一句 access denied（提权判定当时只写了「Stop 不需要提权」）。
    let stopped_by_control_plane = action == ServiceAction::Stop && stop_via_control_plane().await;
    if stopped_by_control_plane {
        return Ok(ActionResult::done("已停止（通过控制面）", None));
    }

    // 级别：界面可以覆盖（系统服务是高级选项），默认按当前状态
    let level = match mode.as_deref() {
        Some("system") => ServiceLevel::System,
        Some("user") => ServiceLevel::User,
        _ => current_level(&app).await,
    };
    let autostart = autostart.unwrap_or(true);
    let args = sidecar::action_args(action, level, autostart);

    // 提权判定：一般动作只看级别；「停止」能走到这里就说明是降级路径，
    // 系统服务下要一次管理员（`sidecar::fallback_stop_needs_elevation`）。
    let elevated = sidecar::needs_elevation(action, level)
        || (action == ServiceAction::Stop && sidecar::fallback_stop_needs_elevation(level));

    let output = match execute(app, args, elevated, "user").await? {
        Some(output) => output,
        None => {
            return Ok(ActionResult::cancelled(match level {
                ServiceLevel::System => "已取消（系统服务需要管理员权限）",
                ServiceLevel::User => "已取消",
            }))
        }
    };

    let parsed = sidecar::parse_action(&output).map_err(CommandError::from_sidecar)?;
    // 停止的降级路径（提权跑 `service stop`）在结果里说清走了哪条路：
    // 用户刚点过一次 UAC，值得知道它是为什么弹的。
    let message: &str = if action == ServiceAction::Stop && elevated {
        "已停止（控制面连不上，改用管理员权限停服务）"
    } else {
        human_action(action)
    };
    Ok(ActionResult::done(message, parsed))
}

/// 让中继**自己**停下来（控制面 `stop`）。返回它是否答应了。
///
/// 这条路不需要提权：服务进程收到控制码后自己退出（`ai-docs/01-ui-and-states.md § 3`）。
/// 连不上就返回 `false`，由调用方降级到 `service stop`。
async fn stop_via_control_plane() -> bool {
    let Discovery::Fresh { endpoint, .. } = discovery::load() else {
        return false;
    };
    control::request(
        &endpoint,
        Request::Stop {
            reason: Some("桌面端".to_owned()),
        },
    )
    .await
    .is_ok()
}
/// 开机自启开关。
#[tauri::command]
pub async fn set_autostart(app: AppHandle, on: bool) -> Result<ActionResult, CommandError> {
    if !sidecar::present() {
        return Err(CommandError::new(
            ErrorCode::SidecarMissing,
            "安装不完整：缺少中继程序。请重新安装 peon-hall",
        ));
    }

    let level = current_level(&app).await;
    let args = sidecar::autostart_args(on);

    let output = match execute(
        app,
        args,
        sidecar::needs_elevation(ServiceAction::Start, level),
        "user",
    )
    .await?
    {
        Some(output) => output,
        None => return Ok(ActionResult::cancelled("已取消（需要管理员权限）")),
    };

    let parsed = sidecar::parse_action(&output).map_err(CommandError::from_sidecar)?;
    Ok(ActionResult::done(
        if on {
            "开机自启已打开"
        } else {
            "开机自启已关闭"
        },
        parsed,
    ))
}
async fn current_level(app: &AppHandle) -> ServiceLevel {
    match service_status(app.clone()).await {
        Ok(status) => status.level.unwrap_or(ServiceLevel::User),
        Err(_) => ServiceLevel::User,
    }
}

/// 动作完成后的那句人话。
fn human_action(action: ServiceAction) -> &'static str {
    match action {
        ServiceAction::Install => "服务已安装并启动",
        ServiceAction::Uninstall => "服务已卸载",
        ServiceAction::Start => "服务已启动",
        ServiceAction::Stop => "服务已停止",
        ServiceAction::Restart => "服务已重启",
    }
}
