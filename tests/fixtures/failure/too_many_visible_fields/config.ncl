# EXPECTED: fail — 7 visible output_fields exceeds MaxVisibleFields cap of 6
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    bad = {
      description = "bad tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [
        { name = "a", description = "a" },
        { name = "b", description = "b" },
        { name = "c", description = "c" },
        { name = "d", description = "d" },
        { name = "e", description = "e" },
        { name = "f", description = "f" },
        { name = "g", description = "g" },
      ],
      aggregates = [{ label = "x", value = "y" }],
      next_steps = [{ command = "x", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
