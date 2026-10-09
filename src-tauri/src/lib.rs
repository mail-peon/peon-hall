//! peon-hall：peon-burrow 中继的桌面安装器与状态面板。
//!
//! 这一层**不含中继逻辑、不含服务注册逻辑**：
//!
//! | 想知道的 | 问谁 |
//! | --- | --- |
//! | 服务注册了吗 / 自启开了吗 / 安装级别 | 中继二进制的 `service status --json`（[`sidecar`]） |
//! | 进程活着吗 / 端口 / 版本 / 连接数 | 控制面（[`control`]，协议实现来自 `peon-burrow-ipc`） |
//! | 现在是什么状态（四种组合） | [`snapshot::Snapshot::phase_of`]（**可单测的纯函数**） |
//! | 需要提权吗 | [`sidecar::needs_elevation`] |
//!
//! 界面只渲染这些；所有系统动作都在 [`commands`] 里，前端没法执行任意命令。

pub mod commands;
pub mod control;
pub mod discovery;
pub mod elevate;
pub mod sidecar;
pub mod snapshot;

/// 启动桌面端。
pub fn run() {
    init_tracing();

    tauri::Builder::default()
        // 只有剪贴板：界面要「复制扩展地址」（capabilities 里也只为它开了权限）
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::heartbeat,
            commands::service_status,
            commands::service_action,
            commands::set_autostart,
            commands::control_command,
            commands::app_info,
            commands::open_config,
            commands::open_logs,
        ])
        .run(tauri::generate_context!())
        .expect("启动 peon-hall 失败");
}

/// 日志：开发时看 stderr，发布时也留着（`RUST_LOG` 不影响，级别固定 info）。
///
/// ⚠️ 桌面端**不做自更新**，也不在启动时联网检查任何东西（离线也要能用）。
fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
}
