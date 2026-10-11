//! 暴露给前端的 Tauri 命令 —— **唯一的系统动作入口**。
//!
//! 前端只渲染与交互：所有读状态、装服务、提权都在这里。这样 capabilities
//! 可以收到最小（只有剪贴板写入），前端也没有能力执行任意命令。

use peon_burrow_ipc_types::{Request, ServiceLevel, ServiceStatus};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::AppHandle;
use tauri::Emitter;

use crate::control;
use crate::discovery::{self, Discovery};
use crate::elevate::{self, ElevationOutcome};
use crate::sidecar::{self, ServiceAction, SidecarError};
use crate::snapshot::{self, DiscoveryState, Snapshot, UrlSource};

/// 本安装包绑定的 core 版本（来自仓库根的 `bundle.toml`，由 `build.rs` 在编译期注入）。
pub const CORE_BOUND: &str = env!("CORE_BOUND");

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
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
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
    fn from_sidecar(error: SidecarError) -> Self {
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
    fn done(message: impl Into<String>, output: Option<Value>) -> Self {
        Self {
            ok: true,
            cancelled: false,
            message: message.into(),
            output,
        }
    }

    fn cancelled(message: impl Into<String>) -> Self {
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
    pub process: Option<peon_burrow_ipc_types::ProcessStatus>,
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

/// 命令抽屉的事件名（前端 `listen` 这个）。
pub const COMMAND_EVENT: &str = "peon-hall://command";

/// 发给前端命令抽屉的一行。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandEvent {
    /// 本次调用的序号（前端按它分组，一次动作一个号）。
    pub id: u64,
    /// `command`（要跑的命令行）/ `stdout` / `stderr` / `exit`。
    pub kind: &'static str,
    /// `user`（用户点的安装/卸载/启停/自启）或 `auto`（界面自己的状态轮询）。
    ///
    /// 抽屉只在 `user` 时自动弹出：轮询每 10 秒一次，全弹出来界面就没法用了。
    pub origin: &'static str,
    /// 内容；`exit` 时是退出码。
    pub line: String,
}

/// 下一条命令的序号。
fn next_command_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// 往抽屉里写一行（发不出去就算了：抽屉是给人看的，不该影响命令本身）。
fn emit_command(
    app: &AppHandle,
    id: u64,
    kind: &'static str,
    origin: &'static str,
    line: impl Into<String>,
) {
    let _ = app.emit(
        COMMAND_EVENT,
        CommandEvent {
            id,
            kind,
            origin,
            line: line.into(),
        },
    );
}

/// 跑一次中继命令，并把「命令行 + 每行输出 + 退出码」**实时**发给抽屉。
///
/// 输入（要跑什么）也是展示的一部分：界面上只看到输出、看不到命令，排查时很被动。
fn run_with_events(
    app: &AppHandle,
    exe: &Path,
    args: &[String],
    origin: &'static str,
) -> Result<sidecar::SidecarOutput, sidecar::SidecarError> {
    let id = next_command_id();
    emit_command(app, id, "command", origin, sidecar::display_line(exe, args));

    let result = sidecar::run_streaming(exe, args, |kind, line| {
        let kind = match kind {
            sidecar::StreamKind::Stdout => "stdout",
            sidecar::StreamKind::Stderr => "stderr",
        };
        emit_command(app, id, kind, origin, line);
    });

    match &result {
        Ok(output) => emit_command(app, id, "exit", origin, output.code.to_string()),
        Err(error) => emit_command(app, id, "stderr", origin, error.to_string()),
    }
    result
}

/// 跑一次**提权**命令，同样把命令行与输出写进抽屉。
///
/// ⚠️ 提权子进程的流在它自己的窗口里（Windows 上是临时文件），父进程拿不到**实时**输出 ——
/// 所以这里只能「先记命令行，跑完再补输出」。取消也会记一行，免得界面看着像卡住了。
fn run_elevated_with_events(
    app: &AppHandle,
    exe: &Path,
    args: &[String],
    origin: &'static str,
) -> Result<ElevationOutcome, String> {
    let id = next_command_id();
    emit_command(app, id, "command", origin, sidecar::display_line(exe, args));

    let outcome = elevate::run(exe, args);

    match &outcome {
        Ok(ElevationOutcome::Ran(output)) => {
            for line in output.stdout.lines() {
                emit_command(app, id, "stdout", origin, line.to_owned());
            }
            for line in output.stderr.lines() {
                emit_command(app, id, "stderr", origin, line.to_owned());
            }
            emit_command(app, id, "exit", origin, output.code.to_string());
        }
        Ok(ElevationOutcome::Cancelled) => {
            emit_command(app, id, "stderr", origin, "已取消提权（用户点了「否」）");
        }
        Err(error) => emit_command(app, id, "stderr", origin, error.clone()),
    }

    outcome
}

/// 问一次服务管理器（**重**：一次进程调用；界面每 5 次轮询查一次）。
#[tauri::command]
pub async fn service_status(app: AppHandle) -> Result<ServiceStatus, CommandError> {
    if !sidecar::present() {
        return Ok(ServiceStatus::not_installed("peon-burrow"));
    }

    let exe = sidecar::sidecar_path()
        .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
    let output = tauri::async_runtime::spawn_blocking(move || {
        run_with_events(&app, &exe, &sidecar::status_args(), "auto")
    })
    .await
    .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
    .map_err(CommandError::from_sidecar)?;

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

    let output = if elevated {
        let exe = sidecar::sidecar_path()
            .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            run_elevated_with_events(&app, &exe, &args, "user")
        })
        .await
        .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
        .map_err(|error| CommandError::new(ErrorCode::Failed, error))?;

        match outcome {
            ElevationOutcome::Cancelled => {
                // ⚠️ 取消不是错误：界面显示「已取消」，状态不变
                return Ok(ActionResult::cancelled(match level {
                    ServiceLevel::System => "已取消（系统服务需要管理员权限）",
                    ServiceLevel::User => "已取消",
                }));
            }
            ElevationOutcome::Ran(output) => output,
        }
    } else {
        let exe = sidecar::sidecar_path()
            .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
        tauri::async_runtime::spawn_blocking(move || {
            run_with_events(&app, &exe, &args.clone(), "user")
        })
        .await
        .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
        .map_err(CommandError::from_sidecar)?
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

    let output = if sidecar::needs_elevation(ServiceAction::Start, level) {
        let exe = sidecar::sidecar_path()
            .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
        match tauri::async_runtime::spawn_blocking(move || {
            run_elevated_with_events(&app, &exe, &args, "user")
        })
        .await
        .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
        .map_err(|error| CommandError::new(ErrorCode::Failed, error))?
        {
            ElevationOutcome::Cancelled => {
                return Ok(ActionResult::cancelled("已取消（需要管理员权限）"));
            }
            ElevationOutcome::Ran(output) => output,
        }
    } else {
        let exe = sidecar::sidecar_path()
            .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
        tauri::async_runtime::spawn_blocking(move || run_with_events(&app, &exe, &args, "user"))
            .await
            .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
            .map_err(CommandError::from_sidecar)?
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

/// 应用信息。
#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        app_version: app.package_info().version.to_string(),
        core_bound: CORE_BOUND.trim().to_owned(),
        platform: std::env::consts::OS.to_owned(),
    }
}

/// 用系统文件管理器打开配置目录（放一个 README 说明能改什么）。
#[tauri::command]
pub async fn open_config() -> Result<String, CommandError> {
    let path = discovery::config_file()
        .ok_or_else(|| CommandError::new(ErrorCode::Failed, "定位不到配置目录"))?;
    let target = path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or(path);
    open_in_file_manager(&target).map_err(|error| CommandError::new(ErrorCode::Failed, error))?;
    Ok(target.display().to_string())
}

/// 打开日志目录。
#[tauri::command]
pub async fn open_logs() -> Result<String, CommandError> {
    let path = discovery::log_dir()
        .ok_or_else(|| CommandError::new(ErrorCode::Failed, "定位不到日志目录"))?;
    // 目录还不存在时先建出来，免得用户点了没反应
    let _ = std::fs::create_dir_all(&path);
    open_in_file_manager(&path).map_err(|error| CommandError::new(ErrorCode::Failed, error))?;
    Ok(path.display().to_string())
}

/// 当前安装级别（没装则按配置里的 `[service] mode`，默认用户级）。
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

/// 用系统文件管理器打开一个目录。
fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    let (program, args) = ("explorer", vec![path.display().to_string()]);

    #[cfg(target_os = "macos")]
    let (program, args) = ("open", vec![path.display().to_string()]);

    #[cfg(all(unix, not(target_os = "macos")))]
    let (program, args) = ("xdg-open", vec![path.display().to_string()]);

    #[cfg(not(any(windows, unix)))]
    let (program, args): (&str, Vec<String>) = ("", Vec::new());

    if program.is_empty() {
        return Err("当前平台不支持打开文件管理器".to_owned());
    }

    std::process::Command::new(program)
        .args(&args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("打不开 {program}：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
