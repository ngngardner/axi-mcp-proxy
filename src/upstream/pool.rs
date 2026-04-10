use anyhow::Result;
use rmcp::model::{CallToolResult, Tool};
use std::collections::HashMap;

use crate::config::UpstreamConfig;
use super::client::Client;

pub struct Pool {
    clients: HashMap<String, Client>,
}

impl Pool {
    pub fn new(upstreams: &HashMap<String, UpstreamConfig>) -> Self {
        let clients = upstreams
            .iter()
            .map(|(name, cfg)| (name.clone(), Client::new(cfg.clone())))
            .collect();
        Self { clients }
    }

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

    pub async fn list_all_tools(&self) -> Result<HashMap<String, Vec<Tool>>> {
        let mut handles = Vec::new();

        for (name, client) in &self.clients {
            let name = name.clone();
            handles.push(async move {
                let tools = client.list_tools().await?;
                Ok::<_, anyhow::Error>((name, tools))
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
