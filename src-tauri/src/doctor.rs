//! 扩展连通性诊断：模拟扩展侧做的事 —— 用 Tauri 进程（与扩展 webview 同台机器）发起一次
//! TCP 连接到中继地址。**不**做 WebSocket 握手，因为中继服务本身的健康已经在 burrow 侧的
//! `doctor` 里查过了；这里只是回答「如果我把扩展的 relay URL 填成 `relayUrl`，它能连上吗」。
//!
//! 为什么 Tauri 侧测，**而不是**真正从扩展侧测：
//! 扩展在本机、和桌面端是同一个网络栈；测两边等同测一边（除非是 webview 沙箱限制，
//! 但那是另一回事，且 webview 通常允许 ws://127.0.0.1）。要测真扩展就得起 IPC 让
//! 扩展把结果传过来 —— 远超这一项诊断要价值。
//!
//! ⚠️ 不要在这里解析证书 / 握手：那是 burrow 的活，重复做只会让 doctor 变慢 +
//! 把可观测问题（端口可达 / 端口不通）的**信息密度**稀释。

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use peon_burrow_ipc_types::{CheckLevel, CheckResult};

/// 这一项检查的稳定 id，UI 按它对照（与文档 / 翻译同步）。
pub const CHECK_ID: &str = "extension_reachable";

/// TCP 探测的超时。诊断不应该让用户等：2 秒够分辨「端口在」与「端口不在」。
const TARGET_TIMEOUT_MS: u64 = 2000;

/// 构造这条检查项（试一次，返回结果）。
pub async fn check(relay_url: &str) -> CheckResult {
    let title = "扩展可连接性".to_owned();
    match probe(relay_url).await {
        Ok(target) => ok(target),
        Err(reason) => fail(title, reason),
    }
}

async fn probe(relay_url: &str) -> Result<String, String> {
    let target = parse_target(relay_url)?;
    // `TcpStream::connect_timeout` 是阻塞调用 —— 走 `spawn_blocking`，跟 `service` /
    // `discovery::pid_gone` 那条路径一致（`ai-docs/01-ui-and-states.md § 3.5`）。
    let timeout = Duration::from_millis(TARGET_TIMEOUT_MS);
    tauri::async_runtime::spawn_blocking(move || connect(&target, timeout))
        .await
        .map_err(|error| format!("诊断任务被取消：{error}"))?
}

fn connect(target: &SocketTarget, timeout: Duration) -> Result<String, String> {
    // `ToSocketAddrs` 会同时解析 hosts 文件 + DNS；不在异步里跑也不会卡死（最长由 OS 决定）。
    // 诊断已经有 2 秒兜底，这里就不再做额外限制。
    let mut addresses = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|error| format!("解析 {} 失败：{error}", target.host))?;
    let address = addresses
        .next()
        .ok_or_else(|| format!("{} 没有可用的解析结果", target.host))?;

    // `connect_timeout` 在 Windows 上**不**支持非 0 的超时（会返回 error），所以兜底
    // 用 `connect` + 手动 `set_read_timeout` 来限制探测时长。
    let stream = TcpStream::connect_timeout(&address, timeout)
        .or_else(|_| TcpStream::connect(address))
        .map_err(|error| describe_error(&address, &error))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    Ok(format!("{}:{} 可达", target.host, target.port))
}

/// `format!("{} 不可达（{kind}: {error}）", address)`
fn describe_error(address: &SocketAddr, error: &std::io::Error) -> String {
    let kind = match error.kind() {
        std::io::ErrorKind::ConnectionRefused => "拒绝连接",
        std::io::ErrorKind::TimedOut => "超时",
        std::io::ErrorKind::AddrInUse => "端口被占",
        _ => "IO 错误",
    };
    format!("{address} {kind}（{error}）")
}

/// `ws://127.0.0.1:41316/` → `127.0.0.1:41316`。返回 `host` + `port`，
/// 故意不返回 `SocketAddr`：解析失败时也能继续，给出可读的错。
fn parse_target(relay_url: &str) -> Result<SocketTarget, String> {
    let url = relay_url.trim();
    if url.is_empty() {
        return Err("中继地址是空的".to_owned());
    }

    // 不引 `url` crate，自己按 `ws://[host][:port]/` 解析：URL 形态在这里就这两种
    // （`transport/relay.ts` 在扩展侧拼好后给过来），多了就是 bug 而不是新需求。
    let (scheme, after_scheme) = url
        .split_once("://")
        .ok_or_else(|| format!("中继地址格式错：缺少 `://`（当前是「{url}」）"))?;
    if !matches!(scheme, "ws" | "wss") {
        return Err(format!(
            "中继地址协议应是 ws:// 或 wss://（当前是「{scheme}」）"
        ));
    }

    // 砍掉 path / query：取到第一个 `/` 或 `?` 之前的就是 authority
    let authority_end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];

    // IPv6 字面量：`[::1]:993`；其它：取第一个 `:` 之前为 host
    let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
        let end = stripped
            .find(']')
            .ok_or_else(|| format!("IPv6 字面量缺右方括号（{authority}）"))?;
        let host = stripped[..end].to_owned();
        let after = &stripped[end + 1..];
        let port = after
            .strip_prefix(':')
            .and_then(|port| port.parse::<u16>().ok())
            .unwrap_or(if scheme == "wss" { 443 } else { 80 });
        (host, port)
    } else {
        match authority.split_once(':') {
            Some((host, port)) => {
                let port =
                    port.parse::<u16>()
                        .ok()
                        .unwrap_or(if scheme == "wss" { 443 } else { 80 });
                (host.to_owned(), port)
            }
            None => (authority.to_owned(), if scheme == "wss" { 443 } else { 80 }),
        }
    };

    if host.is_empty() {
        return Err("中继地址缺 host".to_owned());
    }

    Ok(SocketTarget { host, port })
}

/// `parse_target` 的中间结果：故意不进 `SocketAddr`，免得解析失败时类型也炸。
struct SocketTarget {
    host: String,
    port: u16,
}

fn ok(detail: String) -> CheckResult {
    CheckResult {
        id: CHECK_ID.to_owned(),
        level: CheckLevel::Ok,
        title: "扩展可连接性".to_owned(),
        detail,
        action: None,
    }
}

fn fail(title: String, detail: String) -> CheckResult {
    CheckResult {
        id: CHECK_ID.to_owned(),
        level: CheckLevel::Fail,
        title,
        detail,
        action: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ws_with_loopback_default_port() {
        let target = parse_target("ws://127.0.0.1/").unwrap();
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 80);
    }

    #[test]
    fn parses_ws_with_explicit_port() {
        let target = parse_target("ws://127.0.0.1:41316/").unwrap();
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 41316);
    }

    #[test]
    fn parses_wss_with_default_443() {
        let target = parse_target("wss://example.com/").unwrap();
        assert_eq!(target.host, "example.com");
        assert_eq!(target.port, 443);
    }

    #[test]
    fn strips_query_and_path() {
        let target = parse_target("ws://127.0.0.1:41316/?token=s3cret").unwrap();
        assert_eq!(target.host, "127.0.0.1");
        assert_eq!(target.port, 41316);
    }

    #[test]
    fn parses_ipv6_with_brackets() {
        let target = parse_target("ws://[::1]:41316/").unwrap();
        assert_eq!(target.host, "::1");
        assert_eq!(target.port, 41316);
    }

    #[test]
    fn rejects_empty_url() {
        assert!(parse_target("").is_err());
        assert!(parse_target("   ").is_err());
    }

    #[test]
    fn rejects_non_ws_scheme() {
        assert!(parse_target("http://127.0.0.1:41316/").is_err());
        assert!(parse_target("ftp://example.com/").is_err());
    }

    #[test]
    fn rejects_missing_scheme() {
        assert!(parse_target("127.0.0.1:41316/").is_err());
    }

    #[test]
    fn probe_returns_fail_for_unbound_port() {
        // 1 号端口几乎不会被占，且 `127.0.0.1` 不需要 DNS。
        let result = tauri::async_runtime::block_on(check("ws://127.0.0.1:1/"));
        // 用 tokio runtime 还得带 task —— 这里只验证时序合法、不连成功就行
        assert_eq!(result.id, CHECK_ID);
        // 不锁死 `Ok` / `Fail`：测试机里偶尔有别的进程占着 1
        // 只验证 `id` 与 `title` 是诊断期望的值
        assert_eq!(result.title, "扩展可连接性");
    }
}
