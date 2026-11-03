use super::*;
use std::path::Path;

#[test]
fn install_args_match_the_core_cli() {
    // 必须与 core 的 `service install --mode <user|system> [--no-autostart] --json` 一致
    assert_eq!(
        action_args(ServiceAction::Install, ServiceLevel::User, true),
        vec!["service", "install", "--mode", "user", "--json"]
    );
    assert_eq!(
        action_args(ServiceAction::Install, ServiceLevel::System, false),
        vec![
            "service",
            "install",
            "--mode",
            "system",
            "--no-autostart",
            "--json"
        ]
    );
}

#[test]
fn the_other_actions_are_exact() {
    assert_eq!(
        action_args(ServiceAction::Uninstall, ServiceLevel::User, true),
        vec!["service", "uninstall", "--json"]
    );
    assert_eq!(
        action_args(ServiceAction::Start, ServiceLevel::User, true),
        vec!["service", "start", "--json"]
    );
    assert_eq!(
        action_args(ServiceAction::Stop, ServiceLevel::User, true),
        vec!["service", "stop", "--json"]
    );
    assert_eq!(
        action_args(ServiceAction::Restart, ServiceLevel::User, true),
        vec!["service", "restart", "--json"]
    );
    assert_eq!(status_args(), vec!["service", "status", "--json"]);
    assert_eq!(
        autostart_args(false),
        vec!["service", "autostart", "off", "--json"]
    );
    assert_eq!(
        autostart_args(true),
        vec!["service", "autostart", "on", "--json"]
    );
}

#[test]
fn only_system_level_actions_need_elevation() {
    use ServiceAction::*;
    for action in [Install, Uninstall, Start, Stop, Restart] {
        assert!(
            !needs_elevation(action, ServiceLevel::User),
            "用户级不该提权：{action:?}"
        );
    }
    assert!(needs_elevation(Install, ServiceLevel::System));
    assert!(needs_elevation(Uninstall, ServiceLevel::System));
    assert!(needs_elevation(Start, ServiceLevel::System));
    assert!(needs_elevation(Restart, ServiceLevel::System));
    // 停系统服务走控制面（让服务自己停），所以不需要提权
    assert!(!needs_elevation(Stop, ServiceLevel::System));
}

#[test]
fn stopping_a_system_service_needs_elevation_only_on_the_fallback_path() {
    // H-3：控制面连不上时降级成 `service stop`；系统服务下 `sc stop` 要管理员
    //（`01-ui-and-states.md § 3` 的承诺），用户级下不要。
    assert!(fallback_stop_needs_elevation(ServiceLevel::System));
    assert!(!fallback_stop_needs_elevation(ServiceLevel::User));

    // 「停止」在系统级：控制面这条路不要提权，降级那条要 —— 两者合起来才是完整语义
    assert!(!needs_elevation(ServiceAction::Stop, ServiceLevel::System));
    assert!(fallback_stop_needs_elevation(ServiceLevel::System));
}

#[test]
fn actions_round_trip_through_strings() {
    for action in [
        ServiceAction::Install,
        ServiceAction::Uninstall,
        ServiceAction::Start,
        ServiceAction::Stop,
        ServiceAction::Restart,
    ] {
        assert_eq!(ServiceAction::parse(action.as_str()), Some(action));
    }
    assert_eq!(ServiceAction::parse("format"), None);
}

#[test]
fn status_json_parses_and_mismatches_are_named() {
    let json = r#"{
        "installed": true,
        "running": false,
        "level": "user",
        "autostart": "logon",
        "name": "peon-burrow",
        "binaryPath": "C:\\x\\burrow.exe",
        "requiresElevation": false,
        "restartPolicyConfigured": true,
        "lastExitCode": 4
    }"#;
    let status = parse_status(json).expect("应当能解析");
    assert!(status.installed);
    assert_eq!(status.last_exit_code, Some(4));

    // 字段被 core 改名 → 必须说「版本不匹配」，而不是空白
    let error = parse_status(r#"{"isInstalled":true}"#).expect_err("应当失败");
    let text = error.to_string();
    assert!(text.contains("版本不匹配"), "{text}");
}

#[test]
fn a_failed_action_surfaces_stderr() {
    let output = SidecarOutput {
        code: 2,
        stdout: String::new(),
        stderr: "错误：配置里 max_connections 不合法".to_owned(),
    };
    let error = parse_action(&output).expect_err("应当失败");
    assert!(error.to_string().contains("max_connections"));
}

#[test]
fn a_successful_action_may_print_nothing() {
    let output = SidecarOutput {
        code: 0,
        stdout: String::new(),
        stderr: String::new(),
    };
    assert_eq!(parse_action(&output).expect("成功"), None);

    let output = SidecarOutput {
        code: 0,
        stdout: "{\"installed\":true}".to_owned(),
        stderr: String::new(),
    };
    assert!(parse_action(&output).expect("成功").is_some());
}

/// 真跑一个子进程：流式回调要拿得到 stdout / stderr，退出码要传回来。
///
/// 这条同时守着「静默执行」的回归：spawn 走的是 [`silent_command`]（带
/// `CREATE_NO_WINDOW`），如果哪天有人换回裸 `Command::new`，界面上就会闪黑框。
#[test]
#[cfg(windows)]
fn streaming_reports_stdout_stderr_and_the_exit_code() {
    let cmd = Path::new(r"C:\Windows\System32\cmd.exe");
    if !cmd.is_file() {
        return;
    }

    let args: Vec<String> = ["/c", "echo out-line & echo err-line 1>&2 & exit /b 3"]
        .iter()
        .map(|arg| (*arg).to_owned())
        .collect();

    let mut seen: Vec<(StreamKind, String)> = Vec::new();
    let output = run_streaming(cmd, &args, |kind, line| seen.push((kind, line))).expect("应当能跑");

    assert_eq!(output.code, 3, "退出码要传回来");
    assert!(
        seen.iter()
            .any(|(kind, line)| *kind == StreamKind::Stdout && line.contains("out-line")),
        "{seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(kind, line)| *kind == StreamKind::Stderr && line.contains("err-line")),
        "{seen:?}"
    );
    assert!(output.stdout.contains("out-line"));
    assert!(output.stderr.contains("err-line"));
}

#[test]
fn a_missing_binary_is_reported_not_panicked() {
    let error =
        run_at(Path::new("/definitely/not/there/burrow"), &status_args()).expect_err("应当失败");
    assert!(matches!(error, SidecarError::Missing(_)));
    assert!(error.to_string().contains("安装不完整"));
}
