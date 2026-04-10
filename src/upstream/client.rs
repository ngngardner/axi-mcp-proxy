use anyhow::{Context, Result, bail};
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientCapabilities, Implementation, Tool,
};
use rmcp::service::RunningService;
use rmcp::ServiceExt;
use std::borrow::Cow;
use std::collections::HashMap;
use tokio::sync::OnceCell;

use crate::config::{AuthConfig, UpstreamConfig};

type ClientService = RunningService<rmcp::service::RoleClient, rmcp::model::ClientInfo>;

pub struct Client {
    config: UpstreamConfig,
    service: OnceCell<ClientService>,
}

impl Client {
    pub fn new(config: UpstreamConfig) -> Self {
        Self {
            config,
            service: OnceCell::new(),
        }
    }

    async fn connect(&self) -> Result<ClientService> {
        let client_info = rmcp::model::ClientInfo {
            protocol_version: Default::default(),
            capabilities: ClientCapabilities::default(),
            client_info: Implementation {
                name: "axi-mcp-proxy".into(),
                version: "0.1.0".into(),
            },
        };

        if let Some(ref url) = self.config.url {
            let http_client = build_http_client(&self.config.auth)?;
            let transport =
                rmcp::transport::SseTransport::start_with_client(url.as_str(), http_client)
                    .await
                    .context("SSE transport failed to connect")?;
            let service = client_info
                .serve(transport)
                .await
                .context("SSE client initialization failed")?;
            return Ok(service);
        }

        if let Some(ref cmd) = self.config.cmd {
            let mut command = tokio::process::Command::new(cmd);
            command.args(&self.config.args);
            let transport = rmcp::transport::TokioChildProcess::new(&mut command)
                .context("failed to spawn child process")?;
            let service = client_info
                .serve(transport)
                .await
                .context("stdio client initialization failed")?;
            return Ok(service);
        }

        bail!("upstream has neither url nor cmd configured")
    }

    async fn get_service(&self) -> Result<&ClientService> {
        self.service.get_or_try_init(|| self.connect()).await
    }

    pub async fn call_tool(
        &self,
        tool: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let service = self.get_service().await?;
        let arguments: serde_json::Map<String, serde_json::Value> = args.into_iter().collect();
        let param = CallToolRequestParam {
            name: Cow::Owned(tool.to_string()),
            arguments: Some(arguments),
        };
        service
            .peer()
            .call_tool(param)
            .await
            .map_err(|e| anyhow::anyhow!("call_tool failed: {e}"))
    }

    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let service = self.get_service().await?;
        service
            .peer()
            .list_all_tools()
            .await
            .map_err(|e| anyhow::anyhow!("list_tools failed: {e}"))
    }
}

fn build_http_client(auth: &AuthConfig) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();

    match auth.auth_type.as_str() {
        "bearer" => {
            if let Some(ref token) = auth.token {
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {token}").parse().context("invalid bearer token")?,
                );
            }
        }
        "basic" => {
            if let Some(ref token) = auth.token {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(token.as_bytes());
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Basic {encoded}").parse().context("invalid basic token")?,
                );
            }
        }
        "header" => {
            if let Some(ref h) = auth.headers {
                for (k, v) in h {
                    headers.insert(
                        reqwest::header::HeaderName::from_bytes(k.as_bytes())
                            .context("invalid header name")?,
                        v.parse().context("invalid header value")?,
                    );
                }
            }
        }
        _ => {}
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")
}
