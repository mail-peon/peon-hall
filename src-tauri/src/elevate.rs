//! 提权：GUI 自己**不能**安装服务（`CreateServiceW` / `sc.exe` → access denied），
//! 而 Tauri 的窗口配置里没有 elevation 字段（那是 exe manifest 的事）。
//!
//! 做法：spawn 一个**系统会弹提权提示**的辅助进程，把中继命令交给它跑。
//!
//! | 平台 | 手段 | 用户看到 |
//! | --- | --- | --- |
//! | Windows | `powershell Start-Process -Verb RunAs` | 一次 UAC |
//! | macOS | `osascript do shell script … with administrator privileges` | 系统密码框 |
//! | Linux | `pkexec` | polkit 认证框（无 agent 时失败） |
//!
//! ⚠️ **用户取消不是错误**：返回 [`ElevationOutcome::Cancelled`]，界面显示「已取消」，
//! 状态不变、不记错误（`01-ui-and-states.md § 3.1`）。

use std::path::Path;
use std::process::Command;

use crate::sidecar::SidecarOutput;

/// 提权结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElevationOutcome {
    /// 提权并跑完了（退出码在 `output` 里）。
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

/// 构造提权命令（**纯函数**：平台分支可单测，不需要真的弹 UAC）。
pub fn elevation_command(exe: &Path, args: &[String]) -> ElevationCommand {
    let exe = exe.display().to_string();

    #[cfg(windows)]
    {
        // 退出码 1223 = ERROR_CANCELLED：用固定值区分「用户点了否」与「命令真的失败了」
        //
        // ⚠️ 路径与参数都要做 PowerShell 单引号转义（把 `'` 翻倍）。漏掉路径那一处是**注入**：
        // 一个形如 `C:\it'; Remove-Item …; '\burrow.exe` 的安装路径会变成脚本的一部分。
        let exe = exe.replace('\'', "''");
        let list = args
            .iter()
            .map(|arg| format!("'{}'", arg.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            "try {{ $p = Start-Process -FilePath '{exe}' -ArgumentList {list} -Verb RunAs -Wait -PassThru; exit $p.ExitCode }} catch {{ Write-Error $_.Exception.Message; exit 1223 }}"
        );
        ElevationCommand {
            program: "powershell".to_owned(),
            args: vec![
                "-NoProfile".to_owned(),
                "-NonInteractive".to_owned(),
                "-Command".to_owned(),
                script,
            ],
        }
    }

    #[cfg(target_os = "macos")]
    {
        // osascript 只接受一条 shell 命令字符串，所以这里要自己拼一层引号
        let mut command = shell_quote(&exe);
        for arg in args {
            command.push(' ');
            command.push_str(&shell_quote(arg));
        }
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        ElevationCommand {
            program: "osascript".to_owned(),
            args: vec!["-e".to_owned(), script],
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut full = vec![exe];
        full.extend(args.iter().cloned());
        ElevationCommand {
            program: "pkexec".to_owned(),
            args: full,
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

/// 跑提权命令。
pub fn run(exe: &Path, args: &[String]) -> Result<ElevationOutcome, String> {
    let command = elevation_command(exe, args);
    let output = Command::new(&command.program)
        .args(&command.args)
        .output()
        .map_err(|error| format!("起不了提权进程 {}：{error}", command.program))?;

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
    fn the_windows_command_uses_runas_and_a_fixed_cancel_code() {
        let command = elevation_command(
            Path::new(r"C:\Program Files\peon-hall\burrow.exe"),
            &[
                "service".to_owned(),
                "install".to_owned(),
                "--mode".to_owned(),
                "system".to_owned(),
            ],
        );
        assert_eq!(command.program, "powershell");
        let script = command.args.last().expect("脚本");
        assert!(script.contains("-Verb RunAs"), "必须走 UAC：{script}");
        assert!(script.contains("-Wait"), "要等它跑完：{script}");
        assert!(script.contains("exit 1223"), "取消要有固定退出码：{script}");
        // 参数被逐个单引号包起来（带空格的路径不会被拆开）
        assert!(
            script.contains("'service','install','--mode','system'"),
            "{script}"
        );
        assert!(script.contains(r"C:\Program Files\peon-hall\burrow.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn single_quotes_in_paths_are_escaped() {
        let command = elevation_command(Path::new(r"C:\it's here\burrow.exe"), &[]);
        let script = command.args.last().expect("脚本");
        assert!(script.contains("it''s here"), "单引号要翻倍：{script}");
    }
}
