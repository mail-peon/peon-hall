use super::{CommandError, ErrorCode};
use crate::control;
use crate::discovery::{self, Discovery};
use peon_burrow_ipc_types::Request;
use serde_json::Value;

/// 透传一条控制面命令（`doctor` / `version` / `restart` / `updateCheck` …）。
///
/// ⚠️ 命令集是**枚举白名单**（`peon_burrow_ipc_types::Request`）：前端传不了任意命令。
#[tauri::command]
pub async fn control_command(
    command: String,
    payload: Option<Value>,
) -> Result<Value, CommandError> {
    let request = build_request(&command, payload.as_ref()).ok_or_else(|| {
        CommandError::new(ErrorCode::Failed, format!("不支持的控制面命令：{command}"))
    })?;

    let Discovery::Fresh { endpoint, .. } = discovery::load() else {
        return Err(CommandError::new(
            ErrorCode::ControlUnreachable,
            "中继没有在运行。点【启动】。",
        ));
    };

    control::request(&endpoint, request)
        .await
        .map_err(|error| CommandError::new(ErrorCode::ControlUnreachable, error.message()))
}

/// 扩展连通性诊断：模拟扩展侧的行为，从桌面端进程对中继地址做一次 TCP 探测。
///
/// 与 burrow 侧 `doctor` 的区别：burrow 查的是**中继自己**的健康（配置 / 端口 / 日志）；
/// 这一查的是「扩展 webview 跑这个 `relayUrl` 能不能连上」—— 二者都失败时用户的诊断路径不一样。
///
/// ⚠️ 不要在这里读 token / 不要做 WebSocket 握手：那是 burrow 的活，重复做只会让
/// 诊断变慢 + 把「端口可达 / 端口不通」的信息密度冲淡。
#[tauri::command]
pub async fn check_extension_reachable(relay_url: String) -> peon_burrow_ipc_types::CheckResult {
    crate::doctor::check(&relay_url).await
}

/// 把命令名映射成白名单里的 `Request`。
pub fn build_request(command: &str, payload: Option<&Value>) -> Option<Request> {
    let verbose = payload
        .and_then(|value| value.get("verbose"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let force = payload
        .and_then(|value| value.get("force"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let seconds = payload
        .and_then(|value| value.get("seconds"))
        .and_then(Value::as_u64)
        .map(|value| u32::try_from(value).unwrap_or(60))
        .unwrap_or(60);

    match command {
        "ping" => Some(Request::Ping),
        "status" => Some(Request::Status),
        "version" => Some(Request::Version),
        "doctor" => Some(Request::Doctor { verbose }),
        "restart" => Some(Request::Restart),
        "stop" => Some(Request::Stop {
            reason: Some("桌面端".to_owned()),
        }),
        "updateCheck" => Some(Request::UpdateCheck { force }),
        "updateApply" => Some(Request::UpdateApply),
        "traceOn" => Some(Request::TraceOn { seconds }),
        "traceOff" => Some(Request::TraceOff),
        _ => None,
    }
}
