# EXPECTED: fail — "bad_expr" is not a valid AggregateExpr
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "bad_expr" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
