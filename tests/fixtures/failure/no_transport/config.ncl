# EXPECTED: fail — upstream has neither url nor cmd (UpstreamsUrlXorCmd contract)
let axi = import "axi.ncl" in
{
  upstreams = { svc = { } },
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
