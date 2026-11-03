use super::{AppInfo, CommandError, ErrorCode, CORE_BOUND};
use crate::discovery;
use tauri::AppHandle;

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
