# axi-mcp-proxy

A composing MCP proxy that makes it easy to build tools that follow [Axi](https://github.com/kunchenguid/axi/tree/main) design principles. You declare composite tools in a Nickel config file, and the proxy handles fan-out, parallelism, TOON formatting, aggregation, and next-step suggestions — enforced by Nickel contracts that reject invalid configs at eval time before the proxy even starts.

## Why

Raw MCP tool output is verbose JSON that burns through agent context windows. Axi defines principles for token-efficient, agent-friendly tool output. This proxy enforces several of those principles structurally:

| Principle | How |
|---|---|
| Token-efficient output | All configured tool output goes through the TOON formatter |
| Pre-aggregates | Nickel contract requires non-empty `aggregates` |
| Empty states | Nickel contract requires `empty_message` |
| Next steps | Nickel contract requires non-empty `next_steps` |
| Consistent help | Nickel contract requires `description`; help text is auto-generated from config |
| Content first | Tools run by default; help only returned when `help: true` is explicitly passed |
| Structured errors | All errors use MCP `ToolResultError` consistently |

## Quick start

Run via npx/bunx (no build required):

```bash
npx axi-mcp-proxy --config config.ncl
```

Or build from source:

```bash
cargo build --release
./target/release/axi-mcp-proxy --config config.ncl
```

Nickel configs (`.ncl`) are validated against Axi contracts at eval time. The Nickel evaluator is linked in-process via `nickel-lang-core`. The `axi.ncl` contracts are embedded in the binary, so `import "axi.ncl"` works everywhere.

### Use with Claude Code

Add a `.mcp.json` to your project root:

```json
{
  "mcpServers": {
    "my-proxy": {
      "type": "stdio",
      "command": "npx",
      "args": ["axi-mcp-proxy", "--config", "my-config.ncl", "--transport", "stdio"]
    }
  }
}
```

Then restart Claude Code — your composite tools will appear alongside built-in tools.

## Example

Define a composite tool that fans out to multiple upstream calls:

```nickel
let axi = import "axi.ncl" in
{
  upstreams = {
    github = {
      cmd = "gh",
      args = ["mcp"],
    },
  },
  tools = {
    repo_context = {
      description = "Open PRs, CI status, and assigned issues at a glance",
      parameters = [
        { name = "owner", type = "string", description = "Repo owner", required = true },
        { name = "repo",  type = "string", description = "Repo name",  required = true },
      ],
      steps = [
        {
          name = "prs",
          upstream = "github",
          tool = "list_pull_requests",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", per_page = 10 },
          transform = { pick = ["number", "title", "user.login", "updated_at"] },
        },
        {
          name = "issues",
          upstream = "github",
          tool = "list_issues",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", per_page = 10 },
          transform = { pick = ["number", "title", "labels", "updated_at"] },
        },
      ],
      aggregates = [
        { label = "open PRs",    value = "count($step.prs)" },
        { label = "open issues", value = "count($step.issues)" },
      ],
      next_steps = [
        { command = "repo_context {owner} {repo}", description = "refresh repo overview" },
      ],
      empty_message = "No open PRs or issues found.",
    },
  },
} | axi.Config
```

The proxy fans out both steps in parallel and returns TOON-encoded output:

```
3 open PRs | 2 open issues

[3]{login,number,title,updated_at}:
  alice,42,Fix auth timeout,2026-04-10T12:00:00Z
  bob,41,Add retry logic,2026-04-09T08:30:00Z
  carol,40,Update deps,2026-04-07T15:00:00Z

[2]{labels,number,title,updated_at}:
  [bug],18,Flaky CI,2026-04-10T09:00:00Z
  [enhancement],15,Add dark mode,2026-04-08T11:00:00Z

→ repo_context {owner} {repo} — refresh repo overview
```

## Key concepts

- **Steps** declare upstream tool calls with a dependency DAG (`depends_on`). Independent steps run in parallel.
- **Transforms** (`pick`, `rename`, `filter`) trim upstream responses before output.
- **Aggregates** (`count(...)`, `sum(...)`) produce the summary line at the top.
- **Next steps** guide the agent toward follow-up actions.
- **TOON encoding** compresses arrays of uniform objects into tabular form, cutting token usage vs raw JSON.
- **`${ENV_VAR}`** syntax in auth fields expands environment variables at startup.
- A built-in `list_upstream_tools` tool is always available for upstream discovery.

## Credits

Based on [Axi](https://github.com/kunchenguid/axi/tree/main) by [@kunchenguid](https://github.com/kunchenguid).

## Development

### Prerequisites

Install coverage tooling for the pre-push hook:

```bash
cargo install cargo-llvm-cov
rustup component add llvm-tools-preview
```

### Running coverage locally

```bash
cargo llvm-cov --fail-under-lines 90
```

This runs all tests with LLVM instrumentation and fails if line coverage drops below 90%.
