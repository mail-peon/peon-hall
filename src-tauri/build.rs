//! 构建期把 `bundle.toml` 里的 core 版本注入成编译期常量（界面「关于」显示用）。
//!
//! ⚠️ 从前是 `include_str!("../../core-version.txt")`：那样文件只能是纯文本，而且路径
//! 写死在源码里。换成 `bundle.toml` 之后必须在这里解析 —— 出口是 `env!("CORE_BOUND")`。
//!
//! ⚠️ 失败一律 `panic!`：这些值是**发布产物**的一部分，宁可在构建期炸，也不要装出一个
//! 界面写着 `v0.0.0`、实际捆着别的二进制的安装包。

use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config_path = manifest_dir.join("../bundle.toml");
    // 改了配置要重新编译，否则界面上显示的还是旧版本
    println!("cargo:rerun-if-changed={}", config_path.display());

    let version = read_config(&config_path);
    check_against_the_ipc_dependency(manifest_dir, &version);

    println!("cargo:rustc-env=CORE_BOUND={version}");
    tauri_build::build()
}

/// 读 `bundle.toml` 的 `[core] version`。
fn read_config(path: &Path) -> String {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    let parsed: toml::Value = text
        .parse()
        .unwrap_or_else(|error| panic!("{} 不是合法 TOML：{error}", path.display()));
    let version = parsed
        .get("core")
        .and_then(|core| core.get("version"))
        .and_then(toml::Value::as_str)
        .unwrap_or_else(|| panic!("{} 里缺 [core] version", path.display()))
        .to_owned();

    assert!(
        version.trim_start_matches('v').split('.').count() >= 2,
        "bundle.toml 的 core.version 要写成 tag（例 `v0.1.0`），实际是 {version}"
    );
    version
}

/// 断言 bundle 的 core 版本与 `peon-burrow-ipc` 的依赖要求是同一代。
///
/// 两者漂了，说明「界面显示的 core 版本」和「真正编进去的控制面协议」不是一回事 ——
/// 这正是这个文件要防的事（`ai-docs/03-build-and-sidecar.md § 1`）。
fn check_against_the_ipc_dependency(manifest_dir: &Path, version: &str) {
    let path = manifest_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    let manifest: toml::Value = text
        .parse()
        .unwrap_or_else(|error| panic!("{} 不是合法 TOML：{error}", path.display()));
    let entry = manifest
        .get("dependencies")
        .and_then(|deps| deps.get("peon-burrow-ipc"))
        .unwrap_or_else(|| panic!("{} 里缺 peon-burrow-ipc 依赖", path.display()));

    // 本地联调时这个依赖会写成内联表（`{ path = "../../peon-burrow/crates/..." }`，见
    // Cargo.toml 的注释）—— 那种模式下版本对不上是**预期**的，所以只对字符串形式
    // （`peon-burrow-ipc = "0.1"`，即提交到仓库的正式状态）做同代检查。
    let Some(requirement) = entry.as_str() else {
        println!(
            "cargo:warning=peon-burrow-ipc 是 path/table 依赖（本地联调模式），\
             跳过与 bundle.toml 的版本对齐检查"
        );
        return;
    };

    let bare = version.trim_start_matches('v');
    assert!(
        bare.starts_with(requirement),
        "bundle.toml 的 core.version（{version}）与 Cargo.toml 的 peon-burrow-ipc = \"{requirement}\" \
         不是同一代：捆绑的二进制版本和编进去的控制面协议必须对齐"
    );
}
