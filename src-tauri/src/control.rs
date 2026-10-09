//! 控制面：读进程状态、发命令。
//!
//! **协议的唯一实现是 `peon-burrow-ipc`**（见 `ai-docs/02-core-integration.md § 1`）：
//! 本文件只做「找地址 → 发请求 → 把错误翻译成人话」，绝不手写第二份控制面类型。

use std::path::Path;

use peon_burrow_ipc::{ClientError, ControlClient, ControlEndpoint};
use peon_burrow_ipc_types::{IpcErrorCode, Request, StatusReport};
use serde_json::Value;

use crate::snapshot::{DEFAULT_HOST, DEFAULT_PORT};

/// 控制面相关的错误（**已翻译成人话**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlError {
    /// 找不到发现文件 —— 服务没在跑，或从没装过。
    NotRunning(String),
    /// 发现文件在，但连不上（可能正在重启/更新）。
    Unreachable(String),
    /// 服务明确拒绝了这条请求（token 过期、协议太新……）。
    Rejected {
        /// 错误码。
        code: IpcErrorCode,
        /// 说明。
        message: String,
    },
}

impl ControlError {
    /// 给用户看的文案（含下一步），遵循 core 的文案规范。
    pub fn message(&self) -> String {
        match self {
            Self::NotRunning(detail) => {
                format!("中继没有在运行。点【安装服务】或【启动】。{detail}")
            }
            Self::Unreachable(detail) => {
                format!("连不上中继（可能正在重启或更新中）。{detail}")
            }
            Self::Rejected { message, .. } => message.clone(),
        }
    }

    /// 是「连不上」这一类吗（界面据此决定是否退避轮询）。
    pub fn is_connection(&self) -> bool {
        matches!(self, Self::NotRunning(_) | Self::Unreachable(_))
    }
}

impl From<ClientError> for ControlError {
    fn from(error: ClientError) -> Self {
        match error {
            // 「连不上」这一类要分开：界面据此决定「服务没在跑」还是「正在重启」
            ClientError::Connect(detail) => Self::NotRunning(detail),
            ClientError::Io(ref io) => Self::Unreachable(io.to_string()),
            ClientError::Timeout(duration) => {
                Self::Unreachable(format!("控制面响应超时（{duration:?}）"))
            }
            ClientError::Protocol(detail) => Self::Unreachable(detail),
            ClientError::Rejected { code, message } => Self::Rejected { code, message },
        }
    }
}

/// 发一条请求。
pub async fn request(endpoint: &ControlEndpoint, request: Request) -> Result<Value, ControlError> {
    ControlClient::new(endpoint.clone())
        .request(request)
        .await
        .map_err(ControlError::from)
}

/// 读结构化状态（`status`）。
pub async fn status(endpoint: &ControlEndpoint) -> Result<StatusReport, ControlError> {
    let value = request(endpoint, Request::Status).await?;
    serde_json::from_value(value).map_err(|error| ControlError::Unreachable(error.to_string()))
}

/// 配置里写的监听地址（`relay.toml` 的 `host` / `port`）。
///
/// 返回 `(host, port, 是否来自配置文件)`：界面要如实标出「这是默认值」还是「配置值」。
pub fn configured_address() -> (String, u16, bool) {
    let Some(path) = crate::discovery::config_file() else {
        return (DEFAULT_HOST.to_owned(), DEFAULT_PORT, false);
    };
    read_address_from(&path).unwrap_or_else(|| (DEFAULT_HOST.to_owned(), DEFAULT_PORT, false))
}

/// 从指定的 TOML 文件里读 `host` / `port`。
pub fn read_address_from(path: &Path) -> Option<(String, u16, bool)> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let table = value.as_table()?;

    let host = table
        .get("host")
        .and_then(toml::Value::as_str)
        .unwrap_or(DEFAULT_HOST)
        .to_owned();
    let port = match table.get("port") {
        Some(value) => u16::try_from(value.as_integer()?)
            .ok()
            .unwrap_or(DEFAULT_PORT),
        None => DEFAULT_PORT,
    };
    Some((host, port, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(text: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("relay.toml");
        std::fs::write(&path, text).expect("write");
        (directory, path)
    }

    #[test]
    fn a_config_file_supplies_host_and_port() {
        let (_dir, path) = temp_config("host = \"127.0.0.1\"\nport = 41317\n");
        assert_eq!(
            read_address_from(&path),
            Some(("127.0.0.1".to_owned(), 41317, true))
        );
    }

    #[test]
    fn missing_fields_fall_back_to_the_core_defaults() {
        let (_dir, path) = temp_config("log_level = \"debug\"\n");
        assert_eq!(
            read_address_from(&path),
            Some((DEFAULT_HOST.to_owned(), DEFAULT_PORT, true))
        );
    }

    #[test]
    fn a_broken_or_absent_config_is_not_an_error() {
        assert_eq!(
            read_address_from(Path::new("/definitely/not/here.toml")),
            None
        );
        let (_dir, path) = temp_config("this is not toml = = =");
        assert_eq!(read_address_from(&path), None);
    }

    #[test]
    fn an_insane_port_falls_back_instead_of_panicking() {
        let (_dir, path) = temp_config("port = 999999\n");
        assert_eq!(
            read_address_from(&path).map(|(_, port, _)| port),
            Some(DEFAULT_PORT)
        );
    }

    #[test]
    fn connection_errors_are_translated_into_human_text() {
        let not_running = ControlError::NotRunning("找不到 control.json".to_owned());
        assert!(not_running.message().contains("点【安装服务】"));
        assert!(not_running.is_connection());

        let rejected = ControlError::Rejected {
            code: IpcErrorCode::Unauthorized,
            message: "token 不匹配".to_owned(),
        };
        assert_eq!(rejected.message(), "token 不匹配");
        assert!(!rejected.is_connection());
    }

    #[test]
    fn client_errors_map_to_the_right_bucket() {
        let connect = ControlError::from(ClientError::Connect("拒绝连接".to_owned()));
        assert!(matches!(connect, ControlError::NotRunning(_)));

        let timeout = ControlError::from(ClientError::Timeout(std::time::Duration::from_secs(2)));
        assert!(matches!(timeout, ControlError::Unreachable(_)));

        let rejected = ControlError::from(ClientError::Rejected {
            code: IpcErrorCode::Busy,
            message: "正在更新".to_owned(),
        });
        assert!(matches!(rejected, ControlError::Rejected { .. }));
    }
}
