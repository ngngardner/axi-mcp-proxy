// Binary entrypoint — stdout/stderr output is intentional for CLI
#![allow(clippy::print_stdout, clippy::print_stderr)]

use anyhow::Context;
use axi_mcp_proxy::{config, proxy, upstream};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;

use upstream::client::ANCESTRY_ENV;

#[derive(Parser)]
#[command(
    name = "axi-mcp-proxy",
    version,
    about = "Composing MCP proxy with Axi design principles"
)]
struct Cli {
    /// Path to .ncl config file
    #[arg(long)]
    config: PathBuf,

    /// Transport: stdio or sse
    #[arg(long, default_value = "stdio")]
    transport: String,

    /// Listen address for SSE transport
    #[arg(long, default_value = "0.0.0.0:8080")]
    addr: SocketAddr,

    /// Run a single tool and print the result to stdout (debug mode)
    #[arg(long)]
    run_tool: Option<String>,

    /// JSON params for --run-tool (default: {})
    #[arg(long, default_value = "{}")]
    params: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Detect circular nesting: check if this config is already in the spawn chain.
    let canonical = std::fs::canonicalize(&cli.config).unwrap_or_else(|_| {
        std::path::absolute(&cli.config).unwrap_or_else(|_| cli.config.clone())
    });
    let ancestry: Vec<PathBuf> = std::env::var_os(ANCESTRY_ENV)
        .map(|val| std::env::split_paths(&val).collect())
        .unwrap_or_default();
    if ancestry.iter().any(|p| p == &canonical) {
        let chain: String = ancestry
            .iter()
            .map(|p| p.display().to_string())
            .chain(std::iter::once(canonical.display().to_string()))
            .collect::<Vec<_>>()
            .join(" → ");
        anyhow::bail!(
            "circular proxy nesting detected: {} already in spawn chain\n  chain: {chain}",
            canonical.display()
        );
    }
    let mut new_ancestry = ancestry;
    new_ancestry.push(canonical);
    let ancestry_env =
        std::env::join_paths(&new_ancestry).context("failed to encode proxy ancestry")?;

    let cfg = config::load(&cli.config)?;
    let pool = upstream::pool::Pool::new(&cfg.upstreams, &ancestry_env);
    let server = proxy::server::ProxyServer::new(cfg, pool);

    // Debug mode: run a single tool and exit
    if let Some(tool_name) = cli.run_tool {
        let parsed: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&cli.params)
            .map_err(|e| anyhow::anyhow!("invalid --params JSON: {e}"))?;
        let params = parsed.into_iter().collect();
        match server.run_tool(&tool_name, &params).await {
            Ok(text) => {
                println!("{text}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }

    match cli.transport.as_str() {
        "stdio" => {
            let transport = rmcp::transport::io::stdio();
            let service =
                rmcp::ServiceExt::<rmcp::service::RoleServer>::serve(server, transport).await?;
            service.waiting().await?;
        }
        "sse" => {
            eprintln!("SSE server listening on {}", cli.addr);
            let sse_server = rmcp::transport::SseServer::serve(cli.addr).await?;
            let ct = sse_server.with_service(move || server.clone());
            ct.cancelled().await;
        }
        other => {
            anyhow::bail!("unknown transport: {other} (use stdio or sse)");
        }
    }

    Ok(())
}
