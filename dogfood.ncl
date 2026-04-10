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
      detailed_help = "Shows the top 10 most recent open PRs and issues for a repo. Useful for getting a quick snapshot of activity.",
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
        { command = "get_pull_request {owner} {repo} <number>", description = "PR details" },
        { command = "get_issue {owner} {repo} <number>",        description = "issue details" },
      ],
      empty_message = "No open PRs or issues found.",
      max_items = 10,
    },
  },
} | axi.Config
