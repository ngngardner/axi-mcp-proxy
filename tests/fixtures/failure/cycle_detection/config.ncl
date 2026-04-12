# EXPECTED: fail — step "a" depends on "b" and "b" depends on "a" (cycle)
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      steps = [
        { name = "a", upstream = "svc", tool = "x", args = {}, depends_on = ["b"] },
        { name = "b", upstream = "svc", tool = "x", args = {}, depends_on = ["a"] },
      ],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.a)" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
