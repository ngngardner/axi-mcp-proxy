# End-to-End Example Test Runner

## Summary

Add a new integration test file `tests/examples_e2e.rs` that automatically discovers example configs, checks upstream availability, spawns the proxy, and smoke-tests every tool via `{"help": true}`. Adding a new example with available upstreams = automatically tested, zero Rust changes.

## How It Works

1. **Discover** — glob `examples/*/config.ncl`, same pattern as `config_integration.rs`
2. **Load** — `config::load(path)` for each config
3. **Check availability** — for each upstream in the config:
   - `cmd`-based: check `which(cmd)` — if missing, skip the entire example with `eprintln!` explaining what's missing
   - `url`-based: skip the entire example (can't know if the server is running)
4. **Spawn proxy** — `setup_proxy(config)` using the existing pattern from `integration_test.rs` (Pool::new + SSE server)
5. **Smoke test** — for each tool in the config, call it with `{"help": true}`, assert no error in the response
6. **Teardown** — drop the proxy and cancel tokens

## Test Structure

Single test function `examples_e2e_smoke` that iterates over all discovered examples. Each example is logged to stderr so you can see which ones ran and which were skipped.

```
running 1 test
  examples/minimal: all upstreams available, testing 1 tool(s)...
  examples/minimal: PASS
  examples/repo_context: skipping (upstream "github" needs url, can't verify)
  examples/dogfood: skipping (upstream "github" cmd "gh" not found)
  examples/nested: all upstreams available, testing 1 tool(s)...
  examples/nested: PASS
test examples_e2e_smoke ... ok
```

## Availability Check Logic

```
for each upstream in config.upstreams:
    if upstream has url → mark example as "skip" (reason: url-based upstream)
    if upstream has cmd → check which(cmd)
        if not found → mark example as "skip" (reason: cmd not found)
```

If any upstream is unavailable, the entire example is skipped. No partial runs.

## What This Tests

- The config loads and deserializes correctly (already covered by `config_integration.rs`, but re-verified here)
- The proxy starts with cmd-based upstreams
- Upstream processes spawn successfully
- Tool discovery works through the proxy
- The help path works for every tool

## What This Doesn't Test

- Tool execution with real params (smoke test uses `{"help": true}` only)
- URL-based upstreams (can't verify if the server is running)
- Output content correctness (just verifies no error)

## Files

- Create: `tests/examples_e2e.rs`
- No changes to existing files
- Reuses `which` crate already in dev-dependencies
