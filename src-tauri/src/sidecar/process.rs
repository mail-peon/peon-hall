//! Hidden subprocess creation and stdout/stderr streaming.
use super::{sidecar_path, SidecarError, SidecarOutput};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

/// Windows 上「不要窗口」的创建标志（`CREATE_NO_WINDOW`）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 输出来自哪条流（前端抽屉按这个上色）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// 标准输出。
    Stdout,
    /// 标准错误。
    Stderr,
}

/// 造一条**不弹窗口**的子进程命令。
///
/// ⚠️ Windows 上从 GUI spawn 控制台程序会闪一个黑框 —— 对「点一下按钮」的界面来说
/// 这很吓人（用户以为出了什么事）。`CREATE_NO_WINDOW` 让窗口根本不出现，
/// 而 stdout/stderr 管道照常工作。
pub fn silent_command(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command
}

/// 给抽屉看的一行命令（可复制、可读）。
pub fn display_line(exe: &Path, args: &[String]) -> String {
    let mut line = quote(&exe.display().to_string());
    for arg in args {
        line.push(' ');
        line.push_str(&quote(arg));
    }
    line
}

/// 带空格就加引号（`display_line` 用）。
fn quote(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_owned()
    }
}

/// 跑一次中继命令（**非提权**；提权走 [`crate::elevate`]）。
pub fn run(args: &[String]) -> Result<SidecarOutput, SidecarError> {
    let exe =
        sidecar_path().ok_or_else(|| SidecarError::Missing("定位不到可执行文件".to_owned()))?;
    run_at(&exe, args)
}

/// 在指定路径上跑（测试直接喂一个假 exe）。
pub fn run_at(exe: &Path, args: &[String]) -> Result<SidecarOutput, SidecarError> {
    run_streaming(exe, args, |_, _| {})
}

/// 在指定路径上跑，并把每一行**实时**交给 `on_line`（前端命令抽屉用）。
///
/// 实现：两条读线程各自把行推进同一个 channel，调用线程边收边回调 —— 这样回调不必是
/// `Send`，而 stdout/stderr 都不会因为管道写满把子进程卡住（先读干、再 wait）。
pub fn run_streaming(
    exe: &Path,
    args: &[String],
    mut on_line: impl FnMut(StreamKind, String),
) -> Result<SidecarOutput, SidecarError> {
    if !exe.is_file() {
        return Err(SidecarError::Missing(exe.display().to_string()));
    }

    let mut child = silent_command(exe)
        .args(args)
        .spawn()
        .map_err(|error| SidecarError::Spawn(error.to_string()))?;

    let (sender, receiver) = std::sync::mpsc::channel::<(StreamKind, String)>();
    // 两条流的类型不同（ChildStdout / ChildStderr），显式擦成同一个 trait object
    let pipes: [(StreamKind, Option<Box<dyn std::io::Read + Send>>); 2] = [
        (
            StreamKind::Stdout,
            child
                .stdout
                .take()
                .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
        ),
        (
            StreamKind::Stderr,
            child
                .stderr
                .take()
                .map(|pipe| Box::new(pipe) as Box<dyn std::io::Read + Send>),
        ),
    ];
    for (kind, pipe) in pipes {
        let Some(pipe) = pipe else { continue };
        let sender = sender.clone();
        thread::spawn(move || {
            for line in BufReader::new(pipe).lines() {
                match line {
                    Ok(line) => {
                        if sender.send((kind, line)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    drop(sender);

    let mut stdout = String::new();
    let mut stderr = String::new();
    for (kind, line) in receiver {
        on_line(kind, line.clone());
        let buffer = match kind {
            StreamKind::Stdout => &mut stdout,
            StreamKind::Stderr => &mut stderr,
        };
        buffer.push_str(&line);
        buffer.push('\n');
    }

    let code = child
        .wait()
        .map_err(|error| SidecarError::Spawn(error.to_string()))?
        .code()
        .unwrap_or(-1);

    Ok(SidecarOutput {
        code,
        stdout,
        stderr,
    })
}
