use crate::sidecar::SidecarError;
use crate::snapshot::{DiscoveryState, UrlSource};
use peon_burrow_ipc_types::ProcessStatus;
use serde::Serialize;
use serde_json::Value;

/// 错误码：前端按它分支（**不要匹配 message**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// 用户取消了提权 —— 显示「已取消」，**不是错误**。
    ElevationCancelled,
    /// 中继程序不在（安装包损坏）。
    SidecarMissing,
    /// 控制面连不上（服务没跑）。
    ControlUnreachable,
    /// 中继的返回看不懂 → 版本不匹配，提示更新桌面端。
    VersionMismatch,
    /// 其它失败。
    Failed,
}

/// 命令错误。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// 错误码。
    pub code: ErrorCode,
    /// 人话说明（含下一步）。
    pub message: String,
    /// 是否建议用户点【诊断】。
    pub suggest_doctor: bool,
}

impl CommandError {
    pub(super) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            suggest_doctor: !matches!(
                code,
                ErrorCode::ElevationCancelled | ErrorCode::VersionMismatch
            ),
        }
    }

    /// 从 sidecar 错误映射（区分「缺二进制」与「看不懂返回」）。
    pub(super) fn from_sidecar(error: SidecarError) -> Self {
        match error {
            SidecarError::Missing(_) => Self::new(ErrorCode::SidecarMissing, error.to_string()),
            SidecarError::Unparsable(_) => Self::new(ErrorCode::VersionMismatch, error.to_string()),
            other => Self::new(ErrorCode::Failed, other.to_string()),
        }
    }
}

/// 一次服务动作的结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    /// 是否成功。
    pub ok: bool,
    /// 用户取消了提权（此时 `ok` 为 false，但**不是错误**）。
    pub cancelled: bool,
    /// 给界面的一行结果文案。
    pub message: String,
    /// 中继返回的结构化结果（有就给）。
    pub output: Option<Value>,
}

impl ActionResult {
    pub(super) fn done(message: impl Into<String>, output: Option<Value>) -> Self {
        Self {
            ok: true,
            cancelled: false,
            message: message.into(),
            output,
        }
    }

    pub(super) fn cancelled(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            cancelled: true,
            message: message.into(),
            output: None,
        }
    }
}

/// 应用自身的信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// 桌面端版本（来自 `tauri.conf.json`）。
    pub app_version: String,
    /// 本安装包绑定的 core 版本。
    pub core_bound: String,
    /// 当前平台（界面做平台相关的文案时用）。
    pub platform: String,
}

/// 只要控制面那一半的状态（**轻**，2 秒轮询用这个）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    /// 进程状态（`None` = 连不上）。
    pub process: Option<ProcessStatus>,
    /// 连不上的原因。
    pub control_error: Option<String>,
    /// 发现文件的三态。
    pub discovery: DiscoveryState,
    /// 发现文件说明。
    pub discovery_detail: String,
    /// 扩展地址。
    pub relay_url: String,
    /// 地址来源。
    pub url_source: UrlSource,
}
