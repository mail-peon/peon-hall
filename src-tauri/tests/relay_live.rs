//! 端到端：**真的**和一个运行中的中继对话（需要本地 core 二进制）。
//!
//! 默认 `#[ignore]`（CI 里 `src-tauri/binaries/` 放的是占位文件，且 CI 不想起进程）：
//!
//! ```text
//! cd ../peon-burrow && cargo build -p peon-burrow
//! cd ../peon-hall && pwsh scripts/sync-sidecar.ps1
//! cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture
//! ```
//!
//! ⚠️ 它会**占用** core 的发现文件（`control.json`）。所以开头先看一眼：如果已经有一个
//! 活着的中继在跑，直接跳过 —— 免得把用户正在用的实例的发现文件覆盖掉。

use std::time::{Duration, Instant};

use peon_burrow_ipc_types::Request;
use peon_hall_lib::{control, discovery, sidecar};

/// 已经有一个活着的中继在跑吗（那就不要打扰它）。
fn an_instance_is_already_running() -> bool {
    match discovery::load() {
        discovery::Discovery::Fresh(_) => true,
        discovery::Discovery::Stale { .. } | discovery::Discovery::Missing => false,
    }
}

#[test]
#[ignore = "需要本地 core 二进制与一个可用的端口；手动跑见文件头注释"]
fn it_talks_to_a_real_relay_end_to_end() {
    // 测试二进制在 target/<profile>/deps/ 里，邻居不是安装目录 —— 指到 profile 目录去
    if !sidecar::present() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(profile) = exe.parent().and_then(|deps| deps.parent()) {
                let candidate = profile.join(sidecar::binary_name());
                if candidate.is_file() {
                    std::env::set_var("PEON_HALL_SIDECAR", &candidate);
                }
            }
        }
    }
    if !sidecar::present() {
        eprintln!("跳过：找不到中继二进制（先跑 scripts/sync-sidecar.ps1）");
        return;
    }
    if an_instance_is_already_running() {
        eprintln!("跳过：已经有一个中继在跑，不想覆盖它的发现文件");
        return;
    }

    let exe = sidecar::sidecar_path().expect("拿到中继路径");

    // 起中继：端口交给内核分配（`--port 0`），免得和别的进程抢
    let mut child = std::process::Command::new(&exe)
        .args(["run", "--port", "0"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("起不了中继");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    runtime.block_on(async {
        // 等发现文件出现（中继要先建监听、写文件）
        let deadline = Instant::now() + Duration::from_secs(10);
        let endpoint = loop {
            if let discovery::Discovery::Fresh(endpoint) = discovery::load() {
                break endpoint;
            }
            if Instant::now() > deadline {
                panic!("10 秒内没等到发现文件");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };

        // ① 状态：能连上，且字段是活的
        let report = control::status(&endpoint).await.expect("status 应当成功");
        assert!(report.process.running, "中继应当报告自己在运行");
        assert!(report.process.port > 0, "端口应当是内核分配的真实端口");
        assert_eq!(report.process.protocol, 1, "控制面协议版本");
        eprintln!(
            "中继 v{} 监听 {}:{}，连接数 {}",
            report.process.version,
            report.process.host,
            report.process.port,
            report.process.connections
        );

        // ② ping：轻量存活探测
        let pong = control::request(&endpoint, Request::Ping)
            .await
            .expect("ping");
        assert_eq!(pong["pong"], serde_json::json!(true));

        // ③ version：桌面端「关于」用的
        let version = control::request(&endpoint, Request::Version)
            .await
            .expect("version");
        assert!(version["version"].is_string(), "{version}");

        // ④ doctor：诊断视图的数据源
        let doctor = control::request(&endpoint, Request::Doctor { verbose: true })
            .await
            .expect("doctor");
        let checks = doctor["checks"].as_array().expect("checks 应当是数组");
        assert!(!checks.is_empty(), "诊断应当至少有一项");
        eprintln!("诊断返回 {} 项检查", checks.len());

        // ⑤ stop：让服务自己停（界面上的【停止】走的就是这条路，**不需要提权**）
        control::request(
            &endpoint,
            Request::Stop {
                reason: Some("桌面端集成测试".to_owned()),
            },
        )
        .await
        .expect("stop");

        // 停完之后应当连不上了（发现文件被清掉，或者进程已退出）
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let still_fresh = matches!(discovery::load(), discovery::Discovery::Fresh(_));
            let still_alive = control::status(&endpoint).await.is_ok();
            if !still_fresh && !still_alive {
                break;
            }
            if Instant::now() > deadline {
                panic!(
                    "stop 之后 10 秒内中继仍然可连（发现文件 {still_fresh} / 可连 {still_alive}）"
                );
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    let _ = child.kill();
    let _ = child.wait();
}
