# Nested proxy — uses another axi config as an upstream.
# Demonstrates: reusing an existing config by spawning it as a subprocess,
# then composing new tools on top of the upstream's tools.
# This pattern lets you build on other people's configs without modifying them.
let axi = import "axi.ncl" in
{
  upstreams = {
    inner = {
      cmd = "bunx",
      args = ["axi-mcp-proxy", "--config", "examples/minimal/config.ncl", "--transport", "stdio"],
    },
  },
  tools = {
    deep_search = {
      description = "Search via the inner proxy, then enrich results",
      parameters = [
        { name = "query", type = "string", description = "Search query", required = true },
      ],
      steps = [
        {
          name = "raw",
          upstream = "inner",
          tool = "search",
          args = { query = "$param.query" },
        },
      ],
      output_fields = [
        { name = "id", description = "Result ID" },
      ],
      aggregates = [
        { label = "results", value = "count($step.raw)" },
      ],
      next_steps = [
        { command = "deep_search", description = "search again" },
      ],
      empty_message = "No results from inner proxy.",
    },
  },
} | axi.Config
