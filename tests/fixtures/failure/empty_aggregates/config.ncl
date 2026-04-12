# EXPECTED: fail — Nickel contract rejects empty aggregates array (NotEmpty)
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    bad = {
      description = "bad tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [],
      next_steps = [{ command = "x", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
