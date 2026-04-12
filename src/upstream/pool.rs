use anyhow::Result;
use rmcp::model::{CallToolResult, Tool};
use std::collections::HashMap;
use std::ffi::OsString;

use super::client::Client;
use crate::config::UpstreamConfig;

pub struct Pool {
    clients: HashMap<String, Client>,
}

impl Pool {
    #[must_use]
    pub fn new(upstreams: &HashMap<String, UpstreamConfig>, ancestry: &OsString) -> Self {
        let clients = upstreams
            .iter()
            .map(|(name, cfg)| (name.clone(), Client::new(cfg.clone(), ancestry.clone())))
            .collect();
        Self { clients }
    }

    /// Call a tool on the specified upstream.
    ///
    /// # Errors
    ///
    /// Returns an error if the upstream is unknown or the call fails.
    pub async fn call_tool(
        &self,
        upstream: &str,
        tool: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<CallToolResult> {
        let client = self
            .clients
            .get(upstream)
            .ok_or_else(|| anyhow::anyhow!("unknown upstream: {upstream}"))?;
        client.call_tool(tool, args).await
    }

    /// List tools from every upstream, keyed by upstream name.
    ///
    /// # Errors
    ///
    /// Returns an error if any upstream connection or listing fails.
    pub async fn list_all_tools(&self) -> Result<HashMap<String, Vec<Tool>>> {
        let mut handles = Vec::new();

        for (name, client) in &self.clients {
            let owned_name = name.clone();
            handles.push(async move {
                let tools = client.list_tools().await?;
                Ok::<_, anyhow::Error>((owned_name, tools))
            });
        }

        let results = futures::future::join_all(handles).await;
        let mut all_tools = HashMap::new();
        for result in results {
            let (name, tools) = result?;
            all_tools.insert(name, tools);
        }
        Ok(all_tools)
    }
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("clients", &self.clients.keys().collect::<Vec<_>>())
            .finish()
    }
}
