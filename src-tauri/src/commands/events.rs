use super::{CommandError, ErrorCode};
use crate::elevate::{self, ElevationOutcome};
use crate::sidecar;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter};

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
pub(super) fn run_with_events(
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
pub(super) fn run_elevated_with_events(
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

/// Executes the same command event flow for service actions and autostart changes.
pub(super) async fn execute(
    app: AppHandle,
    args: Vec<String>,
    elevated: bool,
    origin: &'static str,
) -> Result<Option<sidecar::SidecarOutput>, CommandError> {
    let exe = sidecar::sidecar_path()
        .ok_or_else(|| CommandError::new(ErrorCode::SidecarMissing, "定位不到中继程序"))?;
    tauri::async_runtime::spawn_blocking(move || {
        if elevated {
            match run_elevated_with_events(&app, &exe, &args, origin)
                .map_err(|error| CommandError::new(ErrorCode::Failed, error))?
            {
                ElevationOutcome::Cancelled => Ok(None),
                ElevationOutcome::Ran(output) => Ok(Some(output)),
            }
        } else {
            run_with_events(&app, &exe, &args, origin)
                .map(Some)
                .map_err(CommandError::from_sidecar)
        }
    })
    .await
    .map_err(|error| CommandError::new(ErrorCode::Failed, error.to_string()))?
}
