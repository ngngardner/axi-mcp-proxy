use axi_mcp_proxy::config::*;
use axi_mcp_proxy::proxy::server::ProxyServer;
use axi_mcp_proxy::upstream::pool::Pool;
use rmcp::handler::server::ServerHandler;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Mock upstream servers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct MockServer {
    name: String,
    tools: Arc<Vec<(Tool, MockHandler)>>,
}

type MockHandler = Arc<dyn Fn(Value) -> CallToolResult + Send + Sync>;

impl MockServer {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            tools: Arc::new(Vec::new()),
        }
    }

    fn add_tool(mut self, name: &str, description: &str, handler: impl Fn(Value) -> CallToolResult + Send + Sync + 'static) -> Self {
        let tool = Tool::new(
            name.to_string(),
            description.to_string(),
            serde_json::Map::from_iter([
                ("type".to_string(), json!("object")),
            ]),
        );
        Arc::get_mut(&mut self.tools).unwrap().push((tool, Arc::new(handler)));
        self
    }
}

impl ServerHandler for MockServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: None }),
                ..Default::default()
            },
            server_info: Implementation {
                name: self.name.clone(),
                version: "0.1.0".into(),
            },
            instructions: None,
        }
    }

    fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, rmcp::Error>> + Send + '_ {
        let tools: Vec<Tool> = self.tools.iter().map(|(t, _)| t.clone()).collect();
        std::future::ready(Ok(ListToolsResult {
            tools,
            next_cursor: None,
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResult, rmcp::Error>> + Send + '_ {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let tool_name = request.name.to_string();
        let result = self
            .tools
            .iter()
            .find(|(t, _)| t.name == tool_name)
            .map(|(_, handler)| handler(args.clone()))
            .unwrap_or_else(|| {
                CallToolResult::error(vec![Content::text(format!("unknown tool: {tool_name}"))])
            });
        std::future::ready(Ok(result))
    }
}

fn text_result(text: &str) -> CallToolResult {
    CallToolResult::success(vec![Content::text(text)])
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn find_free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn start_mock_server(mock: MockServer) -> (SocketAddr, tokio_util::sync::CancellationToken) {
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = rmcp::transport::SseServer::serve(addr).await.unwrap();
    let ct = server.with_service(move || mock.clone());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, ct)
}

fn make_config(
    upstreams: Vec<(&str, SocketAddr)>,
    tools: HashMap<String, ToolConfig>,
) -> Config {
    let mut upstream_map = HashMap::new();
    for (name, addr) in upstreams {
        upstream_map.insert(
            name.to_string(),
            UpstreamConfig {
                url: Some(format!("http://{addr}/sse")),
                cmd: None,
                args: vec![],
                auth: AuthConfig::default(),
            },
        );
    }
    Config {
        upstreams: upstream_map,
        tools,
    }
}

async fn setup_proxy(cfg: Config) -> (SocketAddr, tokio_util::sync::CancellationToken) {
    let pool = Pool::new(&cfg.upstreams);
    let proxy = ProxyServer::new(cfg, pool);
    let port = find_free_port();
    let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let server = rmcp::transport::SseServer::serve(addr).await.unwrap();
    let ct = server.with_service(move || proxy.clone());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    (addr, ct)
}

async fn call_proxy_tool(
    addr: SocketAddr,
    tool: &str,
    args: Value,
) -> (String, bool) {
    let transport = rmcp::transport::SseTransport::start(format!("http://{addr}/sse"))
        .await
        .expect("connect to proxy");
    let client_info = rmcp::model::ClientInfo {
        protocol_version: Default::default(),
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "test-client".into(),
            version: "0.1.0".into(),
        },
    };
    let service = client_info.serve(transport).await.expect("serve client");
    let peer = service.peer();

    let param = CallToolRequestParam {
        name: Cow::Owned(tool.to_string()),
        arguments: args.as_object().cloned(),
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

// ---------------------------------------------------------------------------
// Mock server factories
// ---------------------------------------------------------------------------

fn mock_github() -> MockServer {
    MockServer::new("mock-github")
        .add_tool("list_pull_requests", "List pull requests", |_| {
            text_result(r#"[
                {"number": 42, "title": "Add feature X", "author": "alice", "updated_at": "2026-04-01T10:00:00Z"},
                {"number": 37, "title": "Fix bug Y", "author": "bob", "updated_at": "2026-03-30T08:00:00Z"},
                {"number": 35, "title": "Update docs", "author": "carol", "updated_at": "2026-03-28T15:00:00Z"}
            ]"#)
        })
        .add_tool("list_issues", "List issues", |_| {
            text_result(r#"[
                {"number": 101, "title": "Performance regression", "labels": "bug,urgent", "updated_at": "2026-04-02T12:00:00Z"},
                {"number": 98, "title": "Add dark mode", "labels": "enhancement", "updated_at": "2026-03-29T09:00:00Z"}
            ]"#)
        })
}

fn mock_ci() -> MockServer {
    MockServer::new("mock-ci").add_tool("list_builds", "List recent builds", |_| {
        text_result(r#"[
            {"id": 501, "status": "passed", "branch": "main", "duration_s": 120},
            {"id": 500, "status": "failed", "branch": "feature-x", "duration_s": 45},
            {"id": 499, "status": "passed", "branch": "main", "duration_s": 118}
        ]"#)
    })
}

fn mock_empty() -> MockServer {
    MockServer::new("mock-empty").add_tool("list_items", "List items (always empty)", |_| {
        text_result("[]")
    })
}

fn mock_filterable() -> MockServer {
    MockServer::new("mock-filterable").add_tool("list_tasks", "List tasks with mixed states", |_| {
        text_result(r#"[
            {"id": 1, "title": "Open task A", "state": "open", "priority": 1},
            {"id": 2, "title": "Closed task B", "state": "closed", "priority": 2},
            {"id": 3, "title": "Open task C", "state": "open", "priority": 3},
            {"id": 4, "title": "Closed task D", "state": "closed", "priority": 1}
        ]"#)
    })
}

fn mock_slow() -> MockServer {
    MockServer::new("mock-slow")
        .add_tool("get_id", "Returns an ID after a short delay", |_| {
            text_result(r#"{"id": "abc-123"}"#)
        })
        .add_tool("get_details", "Get details by ID", |args| {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
            text_result(&format!(
                r#"{{"id": "{id}", "name": "Widget", "status": "active"}}"#
            ))
        })
}

fn mock_numeric() -> MockServer {
    MockServer::new("mock-numeric").add_tool("get_scores", "Get numeric scores", |_| {
        text_result("[10, 20, 30, 40]")
    })
}

// ---------------------------------------------------------------------------
// Integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_repo_context() {
    let (gh_addr, _ct) = start_mock_server(mock_github()).await;

    let tools = HashMap::from([("repo_context".to_string(), ToolConfig {
        description: "Get repository context".into(),
        detailed_help: None,
        parameters: vec![
            ParamConfig { name: "owner".into(), param_type: "string".into(), description: "Repo owner".into(), required: true },
            ParamConfig { name: "repo".into(), param_type: "string".into(), description: "Repo name".into(), required: true },
        ],
        steps: vec![
            StepConfig {
                name: "prs".into(), upstream: "github".into(), tool: "list_pull_requests".into(),
                args: HashMap::from([
                    ("owner".into(), json!("$param.owner")),
                    ("repo".into(), json!("$param.repo")),
                ]),
                depends_on: vec![],
                transform: Some(TransformConfig { pick: Some(vec!["number".into(), "title".into(), "author".into(), "updated_at".into()]), rename: None, filter: None }),
            },
            StepConfig {
                name: "issues".into(), upstream: "github".into(), tool: "list_issues".into(),
                args: HashMap::from([
                    ("owner".into(), json!("$param.owner")),
                    ("repo".into(), json!("$param.repo")),
                ]),
                depends_on: vec![],
                transform: Some(TransformConfig { pick: Some(vec!["number".into(), "title".into(), "labels".into(), "updated_at".into()]), rename: None, filter: None }),
            },
        ],
        output_fields: vec![],
        aggregates: vec![
            AggregateConfig { label: "open PRs".into(), value: "count($step.prs)".into() },
            AggregateConfig { label: "open issues".into(), value: "count($step.issues)".into() },
        ],
        next_steps: vec![
            NextStepConfig { command: "get_pull_request {owner} {repo} <number>".into(), description: "PR details".into(), when: None },
        ],
        empty_message: "No open PRs or issues found.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("github", gh_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "repo_context", json!({"owner": "testorg", "repo": "testrepo"})).await;

    assert!(text.contains("3 open PRs"), "got:\n{text}");
    assert!(text.contains("2 open issues"), "got:\n{text}");
    assert!(text.contains("alice"), "got:\n{text}");
    assert!(text.contains("→ get_pull_request"), "got:\n{text}");
}

#[tokio::test]
async fn test_help_flag() {
    let tools = HashMap::from([("test_tool".to_string(), ToolConfig {
        description: "A test tool for testing".into(),
        detailed_help: None,
        parameters: vec![ParamConfig { name: "query".into(), param_type: "string".into(), description: "Search query".into(), required: true }],
        steps: vec![],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "results".into(), value: "count($step.search)".into() }],
        next_steps: vec![NextStepConfig { command: "details <id>".into(), description: "View details".into(), when: None }],
        empty_message: "No results.".into(),
        max_items: 10,
    })]);

    let cfg = Config { upstreams: HashMap::new(), tools };
    let (proxy_addr, _ct) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "test_tool", json!({"help": true})).await;

    assert!(text.contains("A test tool for testing"), "got:\n{text}");
    assert!(text.contains("query"), "got:\n{text}");
}

#[tokio::test]
async fn test_multi_upstream() {
    let (gh_addr, _ct1) = start_mock_server(mock_github()).await;
    let (ci_addr, _ct2) = start_mock_server(mock_ci()).await;

    let tools = HashMap::from([("project_status".to_string(), ToolConfig {
        description: "Combined project status".into(),
        detailed_help: None,
        parameters: vec![
            ParamConfig { name: "owner".into(), param_type: "string".into(), description: "Owner".into(), required: true },
            ParamConfig { name: "repo".into(), param_type: "string".into(), description: "Repo".into(), required: true },
        ],
        steps: vec![
            StepConfig {
                name: "prs".into(), upstream: "github".into(), tool: "list_pull_requests".into(),
                args: HashMap::from([("owner".into(), json!("$param.owner")), ("repo".into(), json!("$param.repo"))]),
                depends_on: vec![],
                transform: Some(TransformConfig { pick: Some(vec!["number".into(), "title".into(), "author".into()]), rename: None, filter: None }),
            },
            StepConfig {
                name: "builds".into(), upstream: "ci".into(), tool: "list_builds".into(),
                args: HashMap::from([("repo".into(), json!("$param.repo"))]),
                depends_on: vec![],
                transform: Some(TransformConfig { pick: Some(vec!["id".into(), "status".into(), "branch".into()]), rename: None, filter: None }),
            },
        ],
        output_fields: vec![],
        aggregates: vec![
            AggregateConfig { label: "open PRs".into(), value: "count($step.prs)".into() },
            AggregateConfig { label: "recent builds".into(), value: "count($step.builds)".into() },
        ],
        next_steps: vec![NextStepConfig { command: "build_details <id>".into(), description: "Build details".into(), when: None }],
        empty_message: "No data.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("github", gh_addr), ("ci", ci_addr)], tools);
    let (proxy_addr, _ct3) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "project_status", json!({"owner": "testorg", "repo": "testrepo"})).await;

    assert!(text.contains("3 open PRs"), "got:\n{text}");
    assert!(text.contains("3 recent builds"), "got:\n{text}");
    assert!(text.contains("alice"), "got:\n{text}");
    assert!(text.contains("passed") && text.contains("failed"), "got:\n{text}");
}

#[tokio::test]
async fn test_empty_results() {
    let (empty_addr, _ct) = start_mock_server(mock_empty()).await;

    let tools = HashMap::from([("search".to_string(), ToolConfig {
        description: "Search items".into(),
        detailed_help: None,
        parameters: vec![ParamConfig { name: "query".into(), param_type: "string".into(), description: "Query".into(), required: true }],
        steps: vec![StepConfig {
            name: "results".into(), upstream: "empty".into(), tool: "list_items".into(),
            args: HashMap::from([("query".into(), json!("$param.query"))]),
            depends_on: vec![], transform: None,
        }],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "items".into(), value: "count($step.results)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "Nothing found for your query.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("empty", empty_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "search", json!({"query": "nonexistent"})).await;

    assert_eq!(text, "Nothing found for your query.");
}

#[tokio::test]
async fn test_transform_filter() {
    let (filter_addr, _ct) = start_mock_server(mock_filterable()).await;

    let tools = HashMap::from([("open_tasks".to_string(), ToolConfig {
        description: "List open tasks only".into(),
        detailed_help: None,
        parameters: vec![],
        steps: vec![StepConfig {
            name: "tasks".into(), upstream: "filterable".into(), tool: "list_tasks".into(),
            args: HashMap::new(), depends_on: vec![],
            transform: Some(TransformConfig {
                pick: Some(vec!["id".into(), "title".into(), "state".into()]),
                rename: None,
                filter: Some(r#"state == "open""#.into()),
            }),
        }],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "open tasks".into(), value: "count($step.tasks)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No open tasks.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("filterable", filter_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "open_tasks", json!({})).await;

    assert!(text.contains("2 open tasks"), "got:\n{text}");
    assert!(!text.contains("Closed task"), "got:\n{text}");
    assert!(text.contains("Open task A"), "got:\n{text}");
}

#[tokio::test]
async fn test_transform_rename() {
    let (gh_addr, _ct) = start_mock_server(mock_github()).await;

    let tools = HashMap::from([("prs_renamed".to_string(), ToolConfig {
        description: "PRs with renamed fields".into(),
        detailed_help: None,
        parameters: vec![
            ParamConfig { name: "owner".into(), param_type: "string".into(), description: "Owner".into(), required: true },
            ParamConfig { name: "repo".into(), param_type: "string".into(), description: "Repo".into(), required: true },
        ],
        steps: vec![StepConfig {
            name: "prs".into(), upstream: "github".into(), tool: "list_pull_requests".into(),
            args: HashMap::from([("owner".into(), json!("$param.owner")), ("repo".into(), json!("$param.repo"))]),
            depends_on: vec![],
            transform: Some(TransformConfig {
                pick: Some(vec!["number".into(), "title".into(), "author".into()]),
                rename: Some(HashMap::from([("number".into(), "pr_number".into()), ("author".into(), "created_by".into())])),
                filter: None,
            }),
        }],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "PRs".into(), value: "count($step.prs)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No PRs.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("github", gh_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "prs_renamed", json!({"owner": "testorg", "repo": "testrepo"})).await;

    assert!(text.contains("pr_number"), "got:\n{text}");
    assert!(text.contains("created_by"), "got:\n{text}");
}

#[tokio::test]
async fn test_step_dependency() {
    let (slow_addr, _ct) = start_mock_server(mock_slow()).await;

    let tools = HashMap::from([("lookup".to_string(), ToolConfig {
        description: "Look up details by first fetching an ID".into(),
        detailed_help: None,
        parameters: vec![],
        steps: vec![
            StepConfig {
                name: "fetch_id".into(), upstream: "slow".into(), tool: "get_id".into(),
                args: HashMap::new(), depends_on: vec![], transform: None,
            },
            StepConfig {
                name: "fetch_details".into(), upstream: "slow".into(), tool: "get_details".into(),
                args: HashMap::from([("id".into(), json!("$step.fetch_id.id"))]),
                depends_on: vec!["fetch_id".into()], transform: None,
            },
        ],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "steps".into(), value: "count($step.fetch_id)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No data.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("slow", slow_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "lookup", json!({})).await;

    assert!(text.contains("abc-123"), "got:\n{text}");
    assert!(text.contains("Widget"), "got:\n{text}");
}

#[tokio::test]
async fn test_sum_aggregate() {
    let (num_addr, _ct) = start_mock_server(mock_numeric()).await;

    let tools = HashMap::from([("score_total".to_string(), ToolConfig {
        description: "Sum scores".into(),
        detailed_help: None,
        parameters: vec![],
        steps: vec![StepConfig {
            name: "scores".into(), upstream: "numeric".into(), tool: "get_scores".into(),
            args: HashMap::new(), depends_on: vec![], transform: None,
        }],
        output_fields: vec![],
        aggregates: vec![
            AggregateConfig { label: "items".into(), value: "count($step.scores)".into() },
            AggregateConfig { label: "total".into(), value: "sum($step.scores)".into() },
        ],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No scores.".into(),
        max_items: 10,
    })]);

    let cfg = make_config(vec![("numeric", num_addr)], tools);
    let (proxy_addr, _ct2) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "score_total", json!({})).await;

    assert!(text.contains("4 items"), "got:\n{text}");
    assert!(text.contains("100 total"), "got:\n{text}");
}

#[tokio::test]
async fn test_list_upstream_tools() {
    let (gh_addr, _ct1) = start_mock_server(mock_github()).await;
    let (ci_addr, _ct2) = start_mock_server(mock_ci()).await;

    let cfg = make_config(vec![("github", gh_addr), ("ci", ci_addr)], HashMap::new());
    let (proxy_addr, _ct3) = setup_proxy(cfg).await;

    let (text, _) = call_proxy_tool(proxy_addr, "list_upstream_tools", json!({})).await;

    assert!(text.contains("github"), "got:\n{text}");
    assert!(text.contains("ci"), "got:\n{text}");
    assert!(text.contains("list_pull_requests"), "got:\n{text}");
    assert!(text.contains("list_issues"), "got:\n{text}");
    assert!(text.contains("list_builds"), "got:\n{text}");
    assert!(text.contains("2 github tools"), "got:\n{text}");
    assert!(text.contains("1 ci tools"), "got:\n{text}");
    assert!(text.contains("→ list_upstream_tools"), "got:\n{text}");
}

// ---------------------------------------------------------------------------
// Wire protocol tests with @modelcontextprotocol/server-everything
// ---------------------------------------------------------------------------

fn find_bun() -> Option<String> {
    which::which("bun").ok().map(|p| p.to_string_lossy().to_string())
}

fn wire_upstream(bun_path: &str) -> Config {
    Config {
        upstreams: HashMap::from([("everything".to_string(), UpstreamConfig {
            url: None,
            cmd: Some(bun_path.to_string()),
            args: vec!["x".into(), "mcp-server-everything".into()],
            auth: AuthConfig::default(),
        })]),
        tools: HashMap::new(),
    }
}

#[tokio::test]
async fn test_wire_echo() {
    let Some(bun) = find_bun() else {
        eprintln!("bun not found, skipping wire protocol test");
        return;
    };

    let mut cfg = wire_upstream(&bun);
    cfg.tools.insert("wire_echo".to_string(), ToolConfig {
        description: "Echo via real MCP server-everything".into(),
        detailed_help: None,
        parameters: vec![ParamConfig {
            name: "message".into(), param_type: "string".into(),
            description: "Message to echo".into(), required: true,
        }],
        steps: vec![StepConfig {
            name: "echo".into(), upstream: "everything".into(), tool: "echo".into(),
            args: HashMap::from([("message".into(), json!("$param.message"))]),
            depends_on: vec![], transform: None,
        }],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "x".into(), value: "count($step.echo)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No echo.".into(),
        max_items: 10,
    });

    let (proxy_addr, _ct) = setup_proxy(cfg).await;
    let (text, _) = call_proxy_tool(proxy_addr, "wire_echo", json!({"message": "hello from axi"})).await;

    assert!(text.contains("hello from axi"), "got:\n{text}");
}

#[tokio::test]
async fn test_wire_discovery() {
    let Some(bun) = find_bun() else {
        eprintln!("bun not found, skipping wire protocol test");
        return;
    };

    let cfg = wire_upstream(&bun);
    let (proxy_addr, _ct) = setup_proxy(cfg).await;
    let (text, _) = call_proxy_tool(proxy_addr, "list_upstream_tools", json!({})).await;

    assert!(text.contains("echo"), "got:\n{text}");
    assert!(text.contains("everything"), "got:\n{text}");
}

#[tokio::test]
async fn test_wire_get_sum() {
    let Some(bun) = find_bun() else {
        eprintln!("bun not found, skipping wire protocol test");
        return;
    };

    let mut cfg = wire_upstream(&bun);
    cfg.tools.insert("wire_sum".to_string(), ToolConfig {
        description: "Sum numbers via real MCP server".into(),
        detailed_help: None,
        parameters: vec![
            ParamConfig { name: "a".into(), param_type: "number".into(), description: "First number".into(), required: true },
            ParamConfig { name: "b".into(), param_type: "number".into(), description: "Second number".into(), required: true },
        ],
        steps: vec![StepConfig {
            name: "sum".into(), upstream: "everything".into(), tool: "get-sum".into(),
            args: HashMap::from([("a".into(), json!("$param.a")), ("b".into(), json!("$param.b"))]),
            depends_on: vec![], transform: None,
        }],
        output_fields: vec![],
        aggregates: vec![AggregateConfig { label: "x".into(), value: "count($step.sum)".into() }],
        next_steps: vec![NextStepConfig { command: "x".into(), description: "y".into(), when: None }],
        empty_message: "No result.".into(),
        max_items: 10,
    });

    let (proxy_addr, _ct) = setup_proxy(cfg).await;
    let (text, _) = call_proxy_tool(proxy_addr, "wire_sum", json!({"a": 17, "b": 25})).await;

    assert!(text.contains("42"), "got:\n{text}");
}
