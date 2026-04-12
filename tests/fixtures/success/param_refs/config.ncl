# Valid: declared $param.query ref in step args
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      parameters = [
        { name = "query", type = "string", description = "Search query", required = true },
      ],
      steps = [{
        name = "s1",
        upstream = "svc",
        tool = "find",
        args = { q = "$param.query" },
      }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.s1)" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
