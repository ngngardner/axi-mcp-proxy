# EXPECTED: fail — "invalid" is not a valid AuthType (must be none|bearer|basic|header)
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080", auth = { type = "invalid" } } },
  tools = {
    search = {
      description = "search tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.s1)" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
