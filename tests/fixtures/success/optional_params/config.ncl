# Valid: optional parameter ref with trailing ? ($param.filter?)
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      parameters = [
        { name = "filter", type = "string", description = "Optional filter", required = false },
      ],
      steps = [{
        name = "s1",
        upstream = "svc",
        tool = "find",
        args = { ft = "$param.filter?" },
      }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.s1)" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
