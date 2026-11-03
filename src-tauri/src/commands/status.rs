use super::service::service_status;
use super::{CommandError, Heartbeat, CORE_BOUND};
use crate::control;
use crate::discovery::{self, Discovery};
use crate::sidecar;
use crate::snapshot::{self, DiscoveryState, Snapshot, UrlSource};
use tauri::AppHandle;

/// 完整快照（打开窗口 / 窗口获得焦点 / 每次操作结束后调）。
#[tauri::command]
pub async fn snapshot(app: AppHandle) -> Result<Snapshot, CommandError> {
    let heart = heartbeat().await?;

    // ⚠️ 读不到服务状态**不能**当「没安装」（`ai-docs/01-ui-and-states.md § 5`：
    // 「读取状态本身失败 → 显示原始原因」）。从前这里是
    // `unwrap_or_else(|_| ServiceStatus::not_installed(...))`，于是中继的返回看不懂
    // （版本不匹配 = `Unparsable`）时卡片写「未安装」，用户会去点【安装服务】，
    // 而真正该做的是升级桌面端。现在原样带着原因走，界面显示「读不到服务状态」。
    let (service, service_error) = match service_status(app.clone()).await {
        Ok(status) => (Some(status), None),
        Err(error) => (None, Some(error.message)),
    };

    let phase = Snapshot::phase_of(service.as_ref(), heart.process.as_ref(), sidecar::present());

    Ok(Snapshot {
        phase,
        service,
        service_error,
        process: heart.process,
        control_error: heart.control_error,
        discovery: heart.discovery,
        discovery_detail: heart.discovery_detail,
        relay_url: heart.relay_url,
        url_source: heart.url_source,
        sidecar_present: sidecar::present(),
        app_version: app.package_info().version.to_string(),
        core_bound: CORE_BOUND.trim().to_owned(),
    })
}

/// 只要控制面状态。
///
/// ⚠️ 这是**每 2 秒**都要跑的那条路，所以它只能做「读文件 + 连一次控制面」：
/// 「文件里那个进程还在吗」是一次进程调用（Windows 上是 `tasklist`），
/// 只在控制面**连不上之后**才做（见 `discovery` 的文件头与 H-1 的取舍）。
#[tauri::command]
pub async fn heartbeat() -> Result<Heartbeat, CommandError> {
    let discovery = discovery::load();

    let (process, control_error, state) = match &discovery {
        Discovery::Fresh { endpoint, pid } => match control::status(endpoint).await {
            Ok(report) => (Some(report.process), None, DiscoveryState::Fresh),
            // 控制面是权威：连不上之后才轮到问「文件里记的那个进程还在吗」。
            // 它不在了 → 陈旧（说清是哪个 pid 没了）；还在 → 多半正在重启/更新。
            Err(error) => {
                let gone = match *pid {
                    Some(pid) => pid_gone_off_thread(pid).await,
                    None => false,
                };
                match pid {
                    Some(pid) if gone => (
                        None,
                        Some(discovery::stale_detail(*pid)),
                        DiscoveryState::Stale,
                    ),
                    _ => (None, Some(error.message()), DiscoveryState::Fresh),
                }
            }
        },
        Discovery::Missing => (None, None, DiscoveryState::Missing),
    };

    // 说明文案跟着**最终判定**走：判成陈旧时要说出是哪个进程没了
    let detail = match (state, &discovery) {
        (DiscoveryState::Stale, Discovery::Fresh { pid: Some(pid), .. }) => {
            discovery::stale_detail(*pid)
        }
        _ => discovery.describe(),
    };

    // 地址优先用控制面报的（最准），其次配置文件，最后默认值
    let (relay_url, url_source) = match &process {
        Some(process) => (
            snapshot::relay_url(&process.host, process.port),
            UrlSource::ControlPlane,
        ),
        None => {
            let (host, port, from_file) = control::configured_address();
            (
                snapshot::relay_url(&host, port),
                if from_file {
                    UrlSource::ConfigFile
                } else {
                    UrlSource::Default
                },
            )
        }
    };

    Ok(Heartbeat {
        process,
        control_error,
        discovery: state,
        discovery_detail: detail,
        relay_url,
        url_source,
    })
}
/// 「那个进程还在吗」——**挪到线程池上问**。
///
/// `discovery::pid_gone` 是一次进程调用（Windows 上是 `tasklist`，实测 126–159ms），
/// 直接在 async 命令里同步调会占着运行时的工作线程。中继那些进程调用
/// （`service status` / 五个动作）都走 `spawn_blocking`，这里保持一致。
async fn pid_gone_off_thread(pid: u32) -> bool {
    tauri::async_runtime::spawn_blocking(move || discovery::pid_gone(pid))
        .await
        // 线程池挂了（不该发生）：当作「还在」，宁可多试一次连接
        .unwrap_or(false)
}
