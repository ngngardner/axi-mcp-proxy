// End-to-end smoke tests for example configs.
//
// Discovers examples/*/config.ncl, checks upstream availability,
// spawns the proxy, and calls each tool with {"help": true}.
// Skips examples whose upstreams aren't available.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::print_stderr,
    clippy::panic
)]

use axi_mcp_proxy::config;
use axi_mcp_proxy::proxy::server::ProxyServer;
use axi_mcp_proxy::upstream::pool::Pool;
use rmcp::ServiceExt;
use rmcp::model::*;
use serde_json::json;
use std::borrow::Cow;
use std::net::SocketAddr;
use std::path::Path;

fn collect_example_configs() -> Vec<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let Ok(entries) = std::fs::read_dir(&root) else {
        panic!("cannot read examples directory: {}", root.display());
    };
    let mut paths: Vec<std::path::PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|ft| ft.is_dir()))
        .map(|e| e.path().join("config.ncl"))
        .filter(|p| p.exists())
        .collect();
    paths.sort();
    paths
}

/// Check if all upstreams in a config are available.
/// Returns Ok(()) if all are available, Err(reason) if any are missing.
fn check_upstreams(cfg: &config::Config) -> Result<(), String> {
    for (name, upstream) in &cfg.upstreams {
        if upstream.url.is_some() {
            return Err(format!(
                "upstream {name:?} uses url (can't verify availability)"
            ));
        }
        if let Some(ref cmd) = upstream.cmd
            && which::which(cmd).is_err()
        {
            return Err(format!("upstream {name:?} cmd {cmd:?} not found on PATH"));
        }
    }
    Ok(())
}

fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn setup_proxy(cfg: config::Config) -> (SocketAddr, tokio_util::sync::CancellationToken) {
    let pool = Pool::new(&cfg.upstreams);
    let proxy = ProxyServer::new(cfg, pool);
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = rmcp::transport::SseServer::serve(addr).await.unwrap();
    let ct = server.with_service(move || proxy.clone());
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    (addr, ct)
}

async fn call_tool_help(addr: SocketAddr, tool: &str) -> (String, bool) {
    let transport = rmcp::transport::SseTransport::start(format!("http://{addr}/sse"))
        .await
        .expect("connect to proxy");
    let client_info = ClientInfo {
        protocol_version: ProtocolVersion::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "e2e-test".into(),
            version: "0.1.0".into(),
        },
    };
    let service = client_info.serve(transport).await.expect("serve client");
    let peer = service.peer();

    let param = CallToolRequestParam {
        name: Cow::Owned(tool.to_string()),
        arguments: Some(serde_json::Map::from_iter([(
            "help".to_string(),
            json!(true),
        )])),
    };
    let result = peer.call_tool(param).await.expect("call tool");

    let text = result
        .content
        .iter()
        .find_map(|c| {
            if let RawContent::Text(t) = &c.raw {
                Some(t.text.clone())
            } else {
                None
            }
        })
        .unwrap_or_default();

    let is_error = result.is_error.unwrap_or(false);
    drop(service);
    (text, is_error)
}

#[tokio::test]
async fn examples_e2e_smoke() {
    let configs = collect_example_configs();
    assert!(!configs.is_empty(), "no example configs found");

    let mut tested = 0;
    let mut skipped = 0;

    for path in &configs {
        let name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let cfg = config::load(path).unwrap_or_else(|e| {
            panic!("examples/{name}: config::load failed: {e}");
        });

        if let Err(reason) = check_upstreams(&cfg) {
            eprintln!("  examples/{name}: skipping ({reason})");
            skipped += 1;
            continue;
        }

        let tool_names: Vec<String> = cfg.tools.keys().cloned().collect();
        eprintln!(
            "  examples/{name}: all upstreams available, testing {} tool(s)...",
            tool_names.len()
        );

        let (proxy_addr, _ct) = setup_proxy(cfg).await;

        for tool in &tool_names {
            let (text, is_error) = call_tool_help(proxy_addr, tool).await;
            assert!(
                !is_error,
                "examples/{name}: tool {tool:?} returned error: {text}"
            );
            assert!(
                !text.is_empty(),
                "examples/{name}: tool {tool:?} returned empty response"
            );
        }

        eprintln!("  examples/{name}: PASS");
        tested += 1;
    }

    eprintln!("\n  e2e: {tested} tested, {skipped} skipped");
}
