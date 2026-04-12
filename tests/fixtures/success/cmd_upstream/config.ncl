# Valid: cmd-based upstream (alternative to url-based)
let axi = import "axi.ncl" in
{
  upstreams = {
    shell = {
      cmd = "echo",
      args = ["hello"],
    },
  },
  tools = {
    greet = {
      description = "greet tool",
      steps = [
        { name = "s1", upstream = "shell", tool = "run", args = {} },
      ],
      output_fields = [
        { name = "output", description = "Output" },
      ],
      aggregates = [
        { label = "status", value = "$step.s1" },
      ],
      next_steps = [
        { command = "greet", description = "greet again" },
      ],
      empty_message = "No output.",
    },
  },
} | axi.Config
