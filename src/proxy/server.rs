use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use rmcp::Error as McpError;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParam, CallToolResult, Content, Implementation, ListToolsResult,
    PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo, Tool, ToolsCapability,
};
use rmcp::service::{RequestContext, RoleServer};
use serde_json::Value;

use crate::axi::{formatter, help};
use crate::config::Config;
use crate::engine::{graph, resolve, transform::apply_transform};
use crate::toon;
use crate::upstream::pool::Pool;

#[derive(Clone)]
pub struct ProxyServer {
    config: Arc<Config>,
    pool: Arc<Pool>,
    tools: Arc<Vec<Tool>>,
}

impl ProxyServer {
    #[must_use]
    pub fn new(config: Config, pool: Pool) -> Self {
        let tools = build_tool_schemas(&config);
        Self {
            config: Arc::new(config),
            pool: Arc::new(pool),
            tools: Arc::new(tools),
        }
    }
}

impl std::fmt::Debug for ProxyServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProxyServer").finish_non_exhaustive()
    }
}

impl ServerHandler for ProxyServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: None }),
                ..ServerCapabilities::default()
            },
            server_info: Implementation {
                name: "axi-mcp-proxy".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: None,
        }
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: (*self.tools).clone(),
            next_cursor: None,
        }))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_name = request.name.to_string();
        let args = request
            .arguments
            .unwrap_or_default()
            .into_iter()
            .collect::<HashMap<String, Value>>();

        // Built-in: list_upstream_tools
        if tool_name == "list_upstream_tools" {
            if matches!(args.get("help"), Some(Value::Bool(true))) {
                return Ok(CallToolResult::success(vec![Content::text(
                    "list_upstream_tools — List all tools available on connected upstreams\n\n\
                     Discovers and enumerates every tool registered on each upstream MCP server.\n\
                     Output is grouped by upstream name with tool count, name, and description.\n\n\
                     Parameters: none",
                )]));
            }
            return self.handle_list_upstream_tools().await;
        }

        // Find tool config
        let Some(tool_cfg) = self.config.tools.get(&tool_name) else {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "unknown tool: {tool_name}"
            ))]));
        };

        // Check for help parameter
        if matches!(args.get("help"), Some(Value::Bool(true))) {
            return Ok(CallToolResult::success(vec![Content::text(help::help(
                tool_cfg,
            ))]));
        }

        // Extract built-in flags before passing remaining args as params
        let full = matches!(args.get("full"), Some(Value::Bool(true)));

        // Build params from args (remove built-in flags so they don't leak to steps)
        let mut params = args;
        params.remove("help");
        params.remove("full");

        // Execute steps
        match self.execute_tool(tool_cfg, &params, full).await {
            Ok(text) => Ok(CallToolResult::success(vec![Content::text(text)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "execution failed: {e}"
            ))])),
        }
    }
}

impl ProxyServer {
    /// Run a tool by name with the given params. Used by `--run-tool` CLI mode.
    ///
    /// # Errors
    ///
    /// Returns an error if the tool is unknown or execution fails.
    // HashMap is only used internally — no need to be generic over the hasher
    #[allow(clippy::implicit_hasher)]
    pub async fn run_tool(
        &self,
        tool_name: &str,
        params: &HashMap<String, Value>,
    ) -> anyhow::Result<String> {
        if tool_name == "list_upstream_tools" {
            let result = self
                .handle_list_upstream_tools()
                .await
                .map_err(|e| anyhow::anyhow!("MCP error: {e}"))?;
            let text = result
                .content
                .iter()
                .filter_map(|c| match &c.raw {
                    rmcp::model::RawContent::Text(t) => Some(t.text.as_str()),
                    rmcp::model::RawContent::Image(_) | rmcp::model::RawContent::Resource(_) => {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(text);
        }

        let full = matches!(params.get("full"), Some(Value::Bool(true)));

        let tool_cfg = self
            .config
            .tools
            .get(tool_name)
            .ok_or_else(|| anyhow::anyhow!("unknown tool: {tool_name}"))?;

        self.execute_tool(tool_cfg, params, full).await
    }

    // HashMap is only used internally — no need to be generic over the hasher
    #[allow(clippy::implicit_hasher)]
    async fn execute_tool(
        &self,
        tool_cfg: &crate::config::ToolConfig,
        params: &HashMap<String, Value>,
        full: bool,
    ) -> anyhow::Result<String> {
        let layers = graph::build_layers(&tool_cfg.steps)?;
        let mut results: HashMap<String, Value> = HashMap::new();

        for layer in layers {
            let mut handles = Vec::new();
            for step in layer {
                let resolved_args = resolve::resolve_args(&step.args, params, &results)?;

                let pool = Arc::clone(&self.pool);
                let upstream = step.upstream.clone();
                let tool = step.tool.clone();
                let transform = step.transform.clone();
                let step_name = step.name.clone();

                handles.push(tokio::spawn(async move {
                    let call_result = pool.call_tool(&upstream, &tool, resolved_args).await?;
                    let raw_data = extract_result_data(&call_result);
                    let data = apply_transform(raw_data, &transform, full)?;
                    Ok::<_, anyhow::Error>((step_name, data))
                }));
            }

            for handle in handles {
                let (name, data) = handle.await??;
                results.insert(name, data);
            }
        }

        formatter::format(tool_cfg, &results)
    }

    async fn handle_list_upstream_tools(&self) -> Result<CallToolResult, McpError> {
        match self.pool.list_all_tools().await {
            Ok(all_tools) => {
                // Sort upstream names for stable output
                let mut names: Vec<&String> = all_tools.keys().collect();
                names.sort();

                let mut data: serde_json::Map<String, Value> = serde_json::Map::new();
                for name in &names {
                    let tools = &all_tools[name.as_str()];
                    let tool_list: Vec<Value> = tools
                        .iter()
                        .map(|t| {
                            serde_json::json!({
                                "name": t.name,
                                "description": t.description,
                            })
                        })
                        .collect();
                    data.insert((*name).clone(), Value::Array(tool_list));
                }

                let encoded = toon::encode(&Value::Object(data));
                let toon_output = if encoded.is_empty() {
                    "No upstream tools found.".to_owned()
                } else {
                    encoded
                };

                // Summary line: per-upstream counts
                let summary_parts: Vec<String> = names
                    .iter()
                    .map(|name| format!("{} {} tools", all_tools[name.as_str()].len(), name))
                    .collect();

                let mut parts = Vec::new();
                if !summary_parts.is_empty() {
                    parts.push(summary_parts.join(" | "));
                }
                parts.push(format!("{toon_output}\n\n→ list_upstream_tools"));

                Ok(CallToolResult::success(vec![Content::text(
                    parts.join("\n\n"),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "discovery failed: {e}"
            ))])),
        }
    }
}

fn extract_result_data(result: &CallToolResult) -> Value {
    // When multiple content blocks exist, prefer structured data (object/array)
    // over scalars, and longer text over shorter text.
    let mut best: Option<Value> = None;
    let mut best_len: usize = 0;

    for content in &result.content {
        if let rmcp::model::RawContent::Text(text_content) = &content.raw {
            let text = &text_content.text;
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                match &parsed {
                    Value::Object(_) | Value::Array(_) => return parsed,
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                        if text.len() > best_len {
                            best = Some(parsed);
                            best_len = text.len();
                        }
                    }
                }
            } else if text.len() > best_len {
                best = Some(Value::String(text.clone()));
                best_len = text.len();
            }
        }
    }

    best.unwrap_or(Value::Null)
}

fn build_tool_schemas(config: &Config) -> Vec<Tool> {
    let mut tools = Vec::new();

    for (name, tool_cfg) in &config.tools {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &tool_cfg.parameters {
            let type_str = match param.param_type.as_str() {
                "number" => "number",
                "boolean" => "boolean",
                _ => "string",
            };
            properties.insert(
                param.name.clone(),
                serde_json::json!({
                    "type": type_str,
                    "description": param.description,
                }),
            );
            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }

        // Add built-in parameters
        properties.insert(
            "help".into(),
            serde_json::json!({
                "type": "boolean",
                "description": "Show help for this tool",
            }),
        );
        properties.insert(
            "full".into(),
            serde_json::json!({
                "type": "boolean",
                "description": "Show full untruncated output",
            }),
        );

        let schema = serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        });

        let schema_obj: serde_json::Map<String, Value> =
            serde_json::from_value(schema).unwrap_or_default();

        tools.push(Tool::new(
            name.clone(),
            tool_cfg.description.clone(),
            schema_obj,
        ));
    }

    // Built-in list_upstream_tools
    let list_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "help": {
                "type": "boolean",
                "description": "Show help for this tool"
            }
        }
    });
    let list_schema_obj: serde_json::Map<String, Value> =
        serde_json::from_value(list_schema).unwrap_or_default();
    tools.push(Tool::new(
        "list_upstream_tools",
        "List all tools available on upstream MCP servers",
        list_schema_obj,
    ));

    tools
}
