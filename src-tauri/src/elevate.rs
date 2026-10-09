//! 提权：GUI 自己**不能**安装服务（`CreateServiceW` / `sc.exe` → access denied），
//! 而 Tauri 的窗口配置里没有 elevation 字段（那是 exe manifest 的事）。
//!
//! 做法：spawn 一个**系统会弹提权提示**的辅助进程，把中继命令交给它跑。
//!
//! | 平台 | 手段 | 用户看到 |
//! | --- | --- | --- |
//! | Windows | `powershell Start-Process -Verb RunAs`（跑一个做重定向的 .cmd） | 一次 UAC |
//! | macOS | `osascript do shell script … with administrator privileges` | 系统密码框 |
//! | Linux | `pkexec` | polkit 认证框（无 agent 时失败） |
//!
//! ⚠️ **Windows 必须绕一层 .cmd**：`Start-Process -Wait -PassThru` 只给退出码，
//! 子进程的 stdout/stderr 会留在它自己那个一闪而过的控制台里。于是「安装失败」在界面上的
//! 表现是「中继返回退出码 1（没有更多信息）」——真正的原因看不见（真机踩过）。
//! 批处理里做 `> out 2> err`，父进程再读回来。
//!
//! ⚠️ **用户取消不是错误**：返回 [`ElevationOutcome::Cancelled`]，界面显示「已取消」，
//! 状态不变、不记错误（`01-ui-and-states.md § 3.1`）。

use std::path::{Path, PathBuf};
use std::process::Command;

/// Windows 上「不要窗口」的创建标志（`CREATE_NO_WINDOW`）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use crate::sidecar::SidecarOutput;

/// 提权结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevationOutcome {
    /// 提权并跑完了（退出码与输出在 `output` 里）。
    Ran(SidecarOutput),
    /// 用户取消了提权 —— 界面显示「已取消」，**不算失败**。
    Cancelled,
}

/// 要执行的命令（program + args）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevationCommand {
    /// 执行哪个程序。
    pub program: String,
    /// 传什么参数。
    pub args: Vec<String>,
}

/// Windows 提权时用来接输出的三个临时文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElevationFiles {
    /// 子进程的 stdout 落在这里。
    pub stdout: PathBuf,
    /// 子进程的 stderr 落在这里。
    pub stderr: PathBuf,
}

impl ElevationFiles {
    /// 在临时目录里挑三个不冲突的名字（同一次进程里连续调用也不会撞）。
    pub fn create() -> std::io::Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let base =
            std::env::temp_dir().join(format!("peon-hall-elevate-{}-{stamp}", std::process::id()));
        Ok(Self {
            stdout: base.with_extension("out"),
            stderr: base.with_extension("err"),
        })
    }

    /// 清理临时文件（失败就算了：临时目录里的垃圾比一个 panic 无害）。
    pub fn cleanup(&self) {
        for path in [&self.stdout, &self.stderr] {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// 给 `cmd /c` 的一条命令行：`"<exe>" <args> > "<out>" 2> "<err>"`。
///
/// ⚠️ **不写中间批处理**：曾经是「写一个 .cmd 再提权跑它」，而用户机器上弹出了 Windows 的
/// 「你要如何打开这个文件?」（ShellExecute 遇到没有关联的文件时的行为）。现在提权的目标
/// 只有 `cmd.exe` 本身，没有任何需要「打开」的中间文件。
///
/// cmd 的行规则：整条命令以引号开头时，**最外层还要再包一对引号**，所以返回的字符串形如
/// `""C:\a b\burrow.exe" run > "out" 2> "err""`。
///
/// 只接受我们自己构造的参数；出现 `"` / `%` / 换行这类无法安全传给 cmd 的字符时**报错**，
/// 而不是想办法转义 —— 这台机器上的参数永远不该有它们。
pub fn cmd_line(exe: &Path, args: &[String], files: &ElevationFiles) -> Result<String, String> {
    let exe = exe.display().to_string();
    if exe.contains('"') {
        return Err(format!("安装路径里有引号，无法安全提权：{exe}"));
    }
    for arg in args {
        if arg.contains('"') || arg.contains('%') || arg.contains(['\r', '\n']) {
            return Err(format!("参数里有无法安全传给 cmd 的字符：{arg}"));
        }
    }

    let mut line = format!("\"\"{exe}\"");
    for arg in args {
        line.push(' ');
        line.push_str(arg);
    }
    line.push_str(&format!(
        " > \"{}\" 2> \"{}\"\"",
        files.stdout.display(),
        files.stderr.display()
    ));
    Ok(line)
}

/// 构造提权命令（**纯函数**：平台分支可单测，不需要真的弹 UAC）。
pub fn elevation_command(line: &str) -> ElevationCommand {
    #[cfg(windows)]
    {
        // 1223 = ERROR_CANCELLED：用固定退出码区分「用户点了否」与「命令真的失败了」
        // PowerShell 的单引号字符串里，`'` 要写成 `''`
        let line = line.replace('\'', "''");
        let command = format!(
            // `-WindowStyle Hidden`：提权出来的 cmd 窗口不该在用户眼前闪
            "try {{ $p = Start-Process -FilePath $env:ComSpec -ArgumentList '/c','{line}' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode }} catch {{ Write-Error $_.Exception.Message; exit 1223 }}"
        );
        ElevationCommand {
            program: "powershell".to_owned(),
            args: vec![
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-Command".to_owned(),
                command,
            ],
        }
    }

    #[cfg(target_os = "macos")]
    {
        // 这条分支不用临时文件：osascript 自己就是被 Rust 直接 spawn 的，输出能接住。
        // 正文里放什么由 `run` 决定，这里留一个占位实现。
        let _ = line;
        ElevationCommand {
            program: "osascript".to_owned(),
            args: Vec::new(),
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // 同上：pkexec 的输出由 Rust 直接接住。
        let _ = line;
        ElevationCommand {
            program: "pkexec".to_owned(),
            args: Vec::new(),
        }
    }
}

/// 用户取消提权了吗。
///
/// 判据是**关键词 + 固定退出码**双保险：Windows 的 1223（ERROR_CANCELLED）、
/// Linux 的 126/127（polkit 拒绝），加上中英文的取消文案（系统语言可能是任一种）。
pub fn is_cancelled(stderr: &str, code: i32) -> bool {
    if matches!(code, 1223 | 126 | 127) {
        return true;
    }
    let text = stderr.to_lowercase();
    const MARKERS: [&str; 8] = [
        "canceled by the user",
        "cancelled by the user",
        "the operation was canceled",
        "user canceled",
        "user cancelled",
        "操作已被用户取消",
        "用户取消",
        "not authorized",
    ];
    MARKERS.iter().any(|marker| text.contains(marker))
}

/// 在 Windows 上：写批处理 → 提权跑 → 把两个文件读回来。
#[cfg(windows)]
pub fn run(exe: &Path, args: &[String]) -> Result<ElevationOutcome, String> {
    let files = ElevationFiles::create().map_err(|error| format!("建不了临时文件：{error}"))?;
    let line = cmd_line(exe, args, &files)?;

    let outcome = run_windows(&line, &files);
    files.cleanup();

    let (code, stdout, stderr) = outcome?;
    if is_cancelled(&stderr, code) {
        return Ok(ElevationOutcome::Cancelled);
    }
    Ok(ElevationOutcome::Ran(SidecarOutput {
        code,
        stdout,
        stderr,
    }))
}

/// Windows 上的实际 spawn：退出码来自 PowerShell，输出来自两个临时文件。
#[cfg(windows)]
fn run_windows(line: &str, files: &ElevationFiles) -> Result<(i32, String, String), String> {
    let command = elevation_command(line);
    let mut child = Command::new(&command.program);
    child.args(&command.args);

    // ⚠️ 父进程 PowerShell 也不许弹窗口：它只是个提权跳板，用户该看到的只有 UAC
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        child.creation_flags(CREATE_NO_WINDOW);
    }

    let output = child
        .output()
        .map_err(|error| format!("起不了提权进程 {}：{error}", command.program))?;

    let code = output.status.code().unwrap_or(-1);
    let powershell_stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // 提权子进程的输出：拿不到就退化成 PowerShell 自己的话（取消时就是它）
    let stdout = std::fs::read_to_string(&files.stdout).unwrap_or_default();
    let child_stderr = std::fs::read_to_string(&files.stderr).unwrap_or_default();

    let stderr = if child_stderr.trim().is_empty() {
        powershell_stderr
    } else {
        child_stderr
    };

    Ok((code, stdout, stderr))
}

/// macOS / Linux：直接 spawn，输出由 Rust 接住。
#[cfg(not(windows))]
pub fn run(exe: &Path, args: &[String]) -> Result<ElevationOutcome, String> {
    let program: String;
    let full_args: Vec<String>;

    #[cfg(target_os = "macos")]
    {
        // osascript 只接受一条 shell 命令字符串，所以这里要自己拼一层引号
        let mut command = shell_quote(&exe.display().to_string());
        for arg in args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        program = "osascript".to_owned();
        full_args = vec!["-e".to_owned(), script];
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        program = "pkexec".to_owned();
        full_args = std::iter::once(exe.display().to_string())
            .chain(args.iter().cloned())
            .collect();
    }

    let output = Command::new(&program)
        .args(&full_args)
        .output()
        .map_err(|error| format!("起不了提权进程 {program}：{error}"))?;

    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if is_cancelled(&stderr, code) {
        return Ok(ElevationOutcome::Cancelled);
    }

    Ok(ElevationOutcome::Ran(SidecarOutput {
        code,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr,
    }))
}

/// POSIX shell 单引号转义（macOS 分支用）。
#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> ElevationFiles {
        ElevationFiles {
            stdout: PathBuf::from(r"C:\Temp\elevate.out"),
            stderr: PathBuf::from(r"C:\Temp\elevate.err"),
        }
    }

    #[test]
    fn cancellation_is_recognised_in_both_languages() {
        // Windows：ERROR_CANCELLED
        assert!(is_cancelled("", 1223));
        assert!(is_cancelled("任意文案", 1223));
        // Linux：polkit 拒绝
        assert!(is_cancelled("", 126));
        assert!(is_cancelled("", 127));
        // 中英文文案
        assert!(is_cancelled("The operation was canceled by the user.", 1));
        assert!(is_cancelled("操作已被用户取消。", 1));
        assert!(is_cancelled("User canceled.", 1));
        // 真正的失败不能被当成取消
        assert!(!is_cancelled("错误：服务已经安装", 1));
        assert!(!is_cancelled("", 0));
    }

    #[cfg(windows)]
    #[test]
    fn the_cmd_line_redirects_output_so_the_error_survives() {
        // 这条守的是「安装失败但界面只说退出码 1」那个 bug：重定向必须在命令行里
        let line = cmd_line(
            Path::new(r"C:\Program Files\peon-hall\burrow.exe"),
            &[
                "service".to_owned(),
                "install".to_owned(),
                "--mode".to_owned(),
                "system".to_owned(),
                "--json".to_owned(),
            ],
            &files(),
        )
        .expect("应当能生成");

        assert!(
            line.starts_with("\"\"C:\\Program Files\\peon-hall\\burrow.exe\""),
            "以引号开头时要再包一层引号（cmd 的行规则）：{line}"
        );
        assert!(
            line.contains("service install --mode system --json"),
            "{line}"
        );
        assert!(
            line.contains("> \"C:\\Temp\\elevate.out\""),
            "stdout 必须重定向：{line}"
        );
        assert!(
            line.contains("2> \"C:\\Temp\\elevate.err\""),
            "stderr 必须重定向：{line}"
        );
        assert!(line.ends_with('"'), "最外层引号要闭合：{line}");
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_command_uses_runas_and_a_fixed_cancel_code() {
        let line = cmd_line(Path::new(r"C:\x\burrow.exe"), &[], &files()).expect("应当能生成");
        let command = elevation_command(&line);
        assert_eq!(command.program, "powershell");
        let script = command.args.last().expect("脚本");
        assert!(script.contains("-Verb RunAs"), "必须走 UAC：{script}");
        assert!(
            script.contains("-WindowStyle Hidden"),
            "窗口要隐藏：{script}"
        );
        assert!(script.contains("-Wait"), "要等它跑完：{script}");
        assert!(script.contains("exit 1223"), "取消要有固定退出码：{script}");
        assert!(
            script.contains("ComSpec"),
            "要经过 cmd 才能做重定向：{script}"
        );
        assert!(
            script.contains("elevate.out"),
            "重定向的落点要进命令行：{script}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn single_quotes_in_arguments_are_escaped() {
        // PowerShell 单引号字符串里的 `'` 必须翻倍，否则命令行会被截断
        let files = files();
        let line = cmd_line(
            Path::new(r"C:\x\burrow.exe"),
            &["--note".to_owned(), "it's".to_owned()],
            &files,
        )
        .expect("应当能生成");
        let command = elevation_command(&line);
        let script = command.args.last().expect("脚本");
        assert!(script.contains("it''s"), "单引号要翻倍：{script}");
    }

    #[cfg(windows)]
    #[test]
    fn characters_that_cannot_be_passed_to_cmd_are_refused() {
        // 与其想办法转义，不如直接拒绝：我们自己的参数永远不该带这些
        let quoted = cmd_line(Path::new(r#"C:\we"ird\burrow.exe"#), &[], &files());
        assert!(quoted.is_err(), "路径里有引号必须拒绝");

        let percent = cmd_line(
            Path::new(r"C:\ok\burrow.exe"),
            &["--mode".to_owned(), "%PATH%".to_owned()],
            &files(),
        );
        assert!(percent.is_err(), "参数里有 % 必须拒绝");
    }

    #[test]
    fn temp_file_names_do_not_collide() {
        let first = ElevationFiles::create().expect("第一次");
        let second = ElevationFiles::create().expect("第二次");
        assert_ne!(first.stdout, second.stdout, "两次调用不能撞名");
        assert!(first.stdout.extension().is_some_and(|ext| ext == "out"));
    }

    #[test]
    fn cleanup_removes_everything_it_created() {
        let files = ElevationFiles::create().expect("建名字");
        for path in [&files.stdout, &files.stderr] {
            std::fs::write(path, b"x").expect("写文件");
        }
        files.cleanup();
        for path in [&files.stdout, &files.stderr] {
            assert!(!path.exists(), "{path:?} 应当被删掉");
        }
    }
}
