# EXPECTED: fail — next_step command references unknown tool "nonexistent_tool"
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.s1)" }],
      next_steps = [{ command = "nonexistent_tool arg1", description = "bad ref" }],
      empty_message = "none",
    },
  },
} | axi.Config
