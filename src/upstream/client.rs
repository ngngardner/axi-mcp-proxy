use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsString;

use anyhow::{Context, Result, bail};
use rmcp::ServiceExt;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, ClientCapabilities, Implementation, ProtocolVersion, Tool,
};
use rmcp::service::RunningService;
use tokio::sync::OnceCell;

use crate::config::{AuthConfig, AuthType, UpstreamConfig};

/// Env var tracking the chain of config paths in nested proxy spawns.
/// Used to detect circular nesting before it becomes a fork bomb.
pub const ANCESTRY_ENV: &str = "AXI_PROXY_ANCESTRY";

type ClientService = RunningService<rmcp::service::RoleClient, rmcp::model::ClientInfo>;

pub struct Client {
    config: UpstreamConfig,
    ancestry: OsString,
    service: OnceCell<ClientService>,
}

impl Client {
    #[must_use]
    pub fn new(config: UpstreamConfig, ancestry: OsString) -> Self {
        Self {
            config,
            ancestry,
            service: OnceCell::new(),
        }
    }

    async fn connect(&self) -> Result<ClientService> {
        let client_info = rmcp::model::ClientInfo {
            protocol_version: ProtocolVersion::default(),
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
            command.env(ANCESTRY_ENV, &self.ancestry);
            #[cfg(windows)]
            {
                command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
            }
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

    /// Call a tool on the upstream MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or the upstream rejects the call.
    pub async fn call_tool(
        &self,
        tool: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let service = self.get_service().await?;
        let arguments: serde_json::Map<String, serde_json::Value> = args.into_iter().collect();
        let param = CallToolRequestParam {
            name: Cow::Owned(tool.to_owned()),
            arguments: Some(arguments),
        };
        service
            .peer()
            .call_tool(param)
            .await
            .map_err(|e| anyhow::anyhow!("call_tool failed: {e}"))
    }

    /// List all tools available on the upstream MCP server.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection fails or the upstream rejects the request.
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let service = self.get_service().await?;
        service
            .peer()
            .list_all_tools()
            .await
            .map_err(|e| anyhow::anyhow!("list_tools failed: {e}"))
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

fn build_http_client(auth: &AuthConfig) -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();

    match auth.auth_type {
        AuthType::None => {}
        AuthType::Bearer => {
            if let Some(ref token) = auth.token {
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {token}")
                        .parse()
                        .context("invalid bearer token")?,
                );
            }
        }
        AuthType::Basic => {
            if let Some(ref token) = auth.token {
                use base64::Engine;
                let encoded = base64::engine::general_purpose::STANDARD.encode(token.as_bytes());
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    format!("Basic {encoded}")
                        .parse()
                        .context("invalid basic token")?,
                );
            }
        }
        AuthType::Header => {
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
    }

    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .context("failed to build HTTP client")
}

#[cfg(test)]
// Tests use unwrap for brevity — panics are the desired failure mode
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_build_http_client_no_auth() {
        let auth = AuthConfig::default();
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_bearer() {
        let auth = AuthConfig {
            auth_type: AuthType::Bearer,
            token: Some("test-token-123".into()),
            headers: None,
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_bearer_no_token() {
        let auth = AuthConfig {
            auth_type: AuthType::Bearer,
            token: None,
            headers: None,
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_basic() {
        let auth = AuthConfig {
            auth_type: AuthType::Basic,
            token: Some("user:pass".into()),
            headers: None,
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_basic_no_token() {
        let auth = AuthConfig {
            auth_type: AuthType::Basic,
            token: None,
            headers: None,
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_header() {
        let mut h = HashMap::new();
        h.insert("X-Api-Key".into(), "secret".into());
        let auth = AuthConfig {
            auth_type: AuthType::Header,
            token: None,
            headers: Some(h),
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_build_http_client_header_no_headers() {
        let auth = AuthConfig {
            auth_type: AuthType::Header,
            token: None,
            headers: None,
        };
        let client = build_http_client(&auth);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_debug() {
        let config = UpstreamConfig {
            url: Some("http://localhost".into()),
            cmd: None,
            args: vec![],
            auth: AuthConfig::default(),
        };
        let client = Client::new(config, OsString::new());
        let debug = format!("{client:?}");
        assert!(debug.contains("Client"));
    }
}
