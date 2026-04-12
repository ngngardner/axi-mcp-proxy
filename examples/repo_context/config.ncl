# URL-based upstream with bearer auth.
# Demonstrates: env var expansion (${GITHUB_TOKEN}), transform with pick fields,
# hidden output fields (default_visible = false).
let axi = import "axi.ncl" in
{
  upstreams = {
    github = {
      url = "http://localhost:3000/sse",
      auth = { type = "bearer", token = "${GITHUB_TOKEN}" },
    },
  },
  tools = {
    repo_context = {
      description = "Open PRs, CI status, and assigned issues at a glance",
      detailed_help = "Shows the top 10 most recent open PRs with check status, review state, and age. Use --full to see branch names and labels.",
      steps = [
        {
          name = "prs",
          upstream = "github",
          tool = "list_pull_requests",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", limit = 10 },
          transform = { pick = ["number", "title", "author", "updated_at", "status"] },
        },
        {
          name = "issues",
          upstream = "github",
          tool = "list_issues",
          args = { owner = "$param.owner", repo = "$param.repo", state = "open", limit = 10 },
          transform = { pick = ["number", "title", "labels", "updated_at"] },
        },
      ],
      parameters = [
        { name = "owner", type = "string", description = "Repo owner", required = true },
        { name = "repo", type = "string", description = "Repo name", required = true },
      ],
      output_fields = [
        { name = "number",  description = "PR number",     prefix = "#" },
        { name = "title",   description = "PR title",      max_len = 50 },
        { name = "author",  description = "Author login" },
        { name = "age",     description = "Time since update" },
        { name = "status",  description = "Review state" },
        # hidden by default — only in --full
        { name = "branch",  description = "Head branch",   default_visible = false },
        { name = "labels",  description = "PR labels",     default_visible = false },
      ],
      aggregates = [
        { label = "open PRs",    value = "count($step.prs)" },
        { label = "open issues", value = "count($step.issues)" },
      ],
      next_steps = [
        { command = "repo_context {owner} {repo}", description = "refresh repo overview" },
      ],
      empty_message = "No open PRs or issues found.",
      max_items = 10,
    },
  },
} | axi.Config
