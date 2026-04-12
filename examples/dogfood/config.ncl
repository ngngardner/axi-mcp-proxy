# Full multi-tool config used by axi-mcp-proxy itself.
# Demonstrates: cmd-based upstreams, multiple tools with parameters,
# transforms (pick, truncate), shell command integration, aggregates.
let axi = import "axi.ncl" in
{
  upstreams = {
    github = {
      cmd = "gh",
      args = ["mcp"],
    },
    shell = {
      cmd = "bunx",
      args = ["mcp-server-commands"],
    },
  },
  tools = {
    repo_context = {
      description = "Open PRs, CI status, and assigned issues at a glance",
      detailed_help = "Shows open PRs, open issues, latest release, recent commits, and CI status for a repo.",
      steps = [
        {
          name = "prs",
          upstream = "github",
          tool = "list_pull_requests",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", per_page = 10 },
          transform = { pick = ["number", "title", "user.login", "updated_at", "draft"] },
        },
        {
          name = "issues",
          upstream = "github",
          tool = "list_issues",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", per_page = 10 },
          transform = { pick = ["number", "title", "labels", "updated_at"] },
        },
        {
          name = "latest_release",
          upstream = "github",
          tool = "list_releases",
          args = { owner = "$param.owner", repo = "$param.repo", per_page = 1 },
          transform = { pick = ["tag_name", "name", "published_at"] },
        },
        {
          name = "commits",
          upstream = "github",
          tool = "list_commits",
          args = { owner = "$param.owner", repo = "$param.repo", perPage = 5 },
          transform = {
            pick = ["sha", "commit.message", "author.login"],
            truncate = { "sha" = 7, "message" = 72 },
          },
        },
        {
          name = "ci",
          upstream = "shell",
          tool = "run_process",
          args = { argv = ["gh", "api", "repos/$param.owner/$param.repo/actions/runs?per_page=3", "--jq", "[.workflow_runs[] | {name, status, conclusion, head_branch, run_number}]"] },
        },
      ],
      parameters = [
        { name = "owner", type = "string", description = "Repo owner", required = true },
        { name = "repo", type = "string", description = "Repo name", required = true },
      ],
      output_fields = [
        { name = "number",     description = "Number",        prefix = "#" },
        { name = "title",      description = "Title",         max_len = 50 },
        { name = "author",     description = "Author" },
        { name = "updated_at", description = "Last updated" },
        { name = "status",     description = "State" },
        { name = "labels",     description = "Labels",        default_visible = false },
      ],
      aggregates = [
        { label = "open PRs",    value = "count($step.prs)" },
        { label = "open issues", value = "count($step.issues)" },
      ],
      next_steps = [
        { command = "workflow_status {owner} {repo}", description = "CI run details" },
        { command = "check",                          description = "run CI checks locally" },
        { command = "search {pattern}",               description = "search code with ripgrep" }
      ],
      empty_message = "No open PRs or issues found.",
      max_items = 10,
    },

    workflow_status = {
      description = "Recent CI/CD workflow runs and their status",
      detailed_help = "Shows the most recent GitHub Actions workflow runs for a repo, including status, conclusion, branch, and run number.",
      steps = [
        {
          name = "runs",
          upstream = "shell",
          tool = "run_process",
          args = { argv = ["gh", "api", "repos/$param.owner/$param.repo/actions/runs?per_page=10", "--jq", "[.workflow_runs[] | {name, status, conclusion, head_branch, event, created_at, run_number, html_url}]"] },
        },
      ],
      parameters = [
        { name = "owner", type = "string", description = "Repo owner", required = true },
        { name = "repo", type = "string", description = "Repo name", required = true },
      ],
      output_fields = [
        { name = "run_number",  description = "Run #",      prefix = "#" },
        { name = "name",        description = "Workflow" },
        { name = "conclusion",  description = "Result" },
        { name = "head_branch", description = "Branch" },
        { name = "event",       description = "Trigger" },
        { name = "created_at",  description = "Started",     default_visible = false },
        { name = "html_url",    description = "URL",         default_visible = false },
      ],
      aggregates = [
        { label = "runs", value = "count($step.runs)" },
      ],
      next_steps = [
        { command = "repo_context {owner} {repo}", description = "full repo overview" },
      ],
      empty_message = "No workflow runs found.",
      max_items = 10,
    },

    check = {
      description = "Run pre-commit CI checks (fmt, clippy, ast-grep, deny)",
      detailed_help = "Runs prek (pre-commit) with --all-files to execute the full CI check suite: cargo fmt, cargo clippy, ast-grep scan, and cargo deny.",
      steps = [
        {
          name = "result",
          upstream = "shell",
          tool = "run_process",
          args = { argv = ["prek", "run", "--all-files"] },
        },
      ],
      parameters = [],
      output_fields = [
        { name = "output", description = "Check output" },
      ],
      aggregates = [
        { label = "status", value = "$step.result" },
      ],
      next_steps = [
        { command = "check", description = "re-run checks after fixes" },
      ],
      empty_message = "No output from checks.",
    },

    search = {
      description = "Search code with ripgrep — find patterns across files",
      detailed_help = "Searches the codebase using ripgrep (rg). Returns file:line:content matches.",
      steps = [
        {
          name = "results",
          upstream = "shell",
          tool = "run_process",
          args = { argv = ["rg", "-n", "--no-heading", "$param.pattern", "."] },
        },
      ],
      parameters = [
        { name = "pattern", type = "string", description = "Search pattern (regex)", required = true },
      ],
      output_fields = [
        { name = "output", description = "Search results" },
      ],
      aggregates = [
        { label = "results", value = "$step.results" },
      ],
      next_steps = [
        { command = "search {pattern}", description = "refine search" },
      ],
      empty_message = "No matches found.",
      max_items = 50,
    },
  },
} | axi.Config
