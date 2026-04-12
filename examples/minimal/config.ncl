# Minimal axi config — one URL upstream, one tool.
# Copy this as a starting point for your own config.
let axi = import "axi.ncl" in
{
  upstreams = {
    svc = { url = "http://localhost:8080" },
  },
  tools = {
    search = {
      description = "search tool",
      steps = [
        { name = "s1", upstream = "svc", tool = "find", args = {} },
      ],
      output_fields = [
        { name = "id", description = "Result ID" },
      ],
      aggregates = [
        { label = "results", value = "count($step.s1)" },
      ],
      next_steps = [
        { command = "search <query>", description = "Search again" },
      ],
      empty_message = "No results.",
    },
  },
} | axi.Config
