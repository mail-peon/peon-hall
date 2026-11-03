//! Tauri command facade. Implementation and DTOs are grouped by responsibility.
pub mod control;
pub mod desktop;
mod events;
pub mod service;
pub mod status;
mod types;

pub use control::*;
pub use desktop::*;
pub use events::{CommandEvent, COMMAND_EVENT};
pub use service::*;
pub use status::*;
pub use types::*;

/// Version of the bundled relay.
pub const CORE_BOUND: &str = env!("CORE_BOUND");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidecar::SidecarError;
    use peon_burrow_ipc_types::Request;

    #[test]
    fn only_whitelisted_control_commands_are_accepted() {
        for command in [
            "ping",
            "status",
            "version",
            "doctor",
            "restart",
            "stop",
            "updateCheck",
            "updateApply",
            "traceOn",
            "traceOff",
        ] {
            assert!(
                build_request(command, None).is_some(),
                "{command} 应当被接受"
            );
        }
        // ⚠️ 前端不能借这个通道执行任意东西
        for command in ["exec", "readCredentials", "setPort", "", "SHUTDOWN"] {
            assert!(
                build_request(command, None).is_none(),
                "{command} 不该被接受"
            );
        }
    }

    #[test]
    fn payload_flags_are_carried_through() {
        let payload = serde_json::json!({ "verbose": true });
        assert_eq!(
            build_request("doctor", Some(&payload)),
            Some(Request::Doctor { verbose: true })
        );

        let payload = serde_json::json!({ "seconds": 120 });
        assert_eq!(
            build_request("traceOn", Some(&payload)),
            Some(Request::TraceOn { seconds: 120 })
        );

        // 缺 payload 时给安全默认值（不详细、60 秒）
        assert_eq!(
            build_request("doctor", None),
            Some(Request::Doctor { verbose: false })
        );
        assert_eq!(
            build_request("traceOn", None),
            Some(Request::TraceOn { seconds: 60 })
        );
    }

    #[test]
    fn error_codes_drive_the_ui_branch() {
        let missing = CommandError::from_sidecar(SidecarError::Missing("x".to_owned()));
        assert_eq!(missing.code, ErrorCode::SidecarMissing);

        let mismatch = CommandError::from_sidecar(SidecarError::Unparsable("y".to_owned()));
        assert_eq!(mismatch.code, ErrorCode::VersionMismatch);
        // 版本不匹配不该建议用户去点诊断（诊断也读同一份不兼容的输出）
        assert!(!mismatch.suggest_doctor);

        let failed = CommandError::from_sidecar(SidecarError::Failed("z".to_owned()));
        assert_eq!(failed.code, ErrorCode::Failed);
        assert!(failed.suggest_doctor);
    }

    #[test]
    fn cancellation_is_not_an_error() {
        let result = ActionResult::cancelled("已取消（系统服务需要管理员权限）");
        assert!(!result.ok);
        assert!(result.cancelled);
        assert!(result.message.contains("已取消"));
        assert!(!result.message.contains("失败"));
    }

    #[test]
    fn the_bound_core_version_is_present() {
        // `build.rs` 已经从 bundle.toml 里解析并断言过一次（含与 IPC 依赖的对齐）；
        // 这里守的是「注入真的发生了」—— CI 还会验它指向的 release 资产能下载
        assert!(!CORE_BOUND.trim().is_empty());
        assert!(CORE_BOUND.trim().starts_with('v'), "{}", CORE_BOUND);
    }
}
