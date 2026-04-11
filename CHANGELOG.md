# Changelog

## [0.3.1](https://github.com/ngngardner/axi-mcp-proxy/compare/v0.3.0...v0.3.1) (2026-04-11)


### Bug Fixes

* add actions:write permission to release-please workflow ([ef4b1c6](https://github.com/ngngardner/axi-mcp-proxy/commit/ef4b1c6d6c44ce2e0a7444752667722d6277414d))

## [0.3.0](https://github.com/ngngardner/axi-mcp-proxy/compare/v0.2.0...v0.3.0) (2026-04-11)


### Features

* add --run-tool CLI mode for debugging tool execution ([6db5121](https://github.com/ngngardner/axi-mcp-proxy/commit/6db512183a2aa28823a552548e84755ac6ef6eac))
* add cargo-llvm-cov coverage enforcement at 90% threshold ([d5b3415](https://github.com/ngngardner/axi-mcp-proxy/commit/d5b341549cc4ca3729191fe23f068af41f5119cd))
* add ripgrep search tool and fix plain-string formatting ([b7d9b30](https://github.com/ngngardner/axi-mcp-proxy/commit/b7d9b30626b38bf804ac9230abc76189fadca839))
* add string interpolation, dogfood tools, and Windows fixes ([971a430](https://github.com/ngngardner/axi-mcp-proxy/commit/971a4309ac54095d06817cc53b9f5e7042a8c77a))
* add truncate transform and --full escape hatch ([d031ca1](https://github.com/ngngardner/axi-mcp-proxy/commit/d031ca12deaf609f375d31c177beb38798718002))
* support optional parameters with $param.X? syntax ([7ea6398](https://github.com/ngngardner/axi-mcp-proxy/commit/7ea6398b3f9c1fa8149b1c16386840656894fbfd))
* validate $param/$step references at config load time ([3220698](https://github.com/ngngardner/axi-mcp-proxy/commit/32206985dddc19e62a3f627638b6bd2f29633f73))


### Bug Fixes

* example config next_steps reference undefined tools ([1caea1f](https://github.com/ngngardner/axi-mcp-proxy/commit/1caea1f1c70c437b61f19131065ba2e6cf551159))
* install cargo-deny in CI workflow ([c7fe8b3](https://github.com/ngngardner/axi-mcp-proxy/commit/c7fe8b39bc8c63846256ae96373944e45d01cddb))
* override CARGO_TARGET_DIR in release workflow ([2102bd4](https://github.com/ngngardner/axi-mcp-proxy/commit/2102bd470bedfcae09cfcf0c31d3bf749385a065))
* trigger npm publish via workflow_dispatch after release-please ([fc30564](https://github.com/ngngardner/axi-mcp-proxy/commit/fc305642424815cf09e87498179307bdc4f3fa30))
* validate next_steps reference defined tools ([b0bee23](https://github.com/ngngardner/axi-mcp-proxy/commit/b0bee237d325302b16adfe85e6c9435f5f05b275))

## [0.2.0](https://github.com/ngngardner/axi-mcp-proxy/compare/v0.1.0...v0.2.0) (2026-04-10)


### Features

* add automated releases with release-please ([1f469ab](https://github.com/ngngardner/axi-mcp-proxy/commit/1f469ab3b9864642889319c30dfff15e51533d5a))
* add dogfood config for GitHub MCP via gh-mcp ([62e9e67](https://github.com/ngngardner/axi-mcp-proxy/commit/62e9e670606d8fb58d2aac871356921ddbdd7040))


### Bug Fixes

* dotted-path pick, max_items enforcement, TOON encoding ([ce19263](https://github.com/ngngardner/axi-mcp-proxy/commit/ce192634e9b7bcd89d5caf49e2f04efea925a348))
* embed axi.ncl into binary for portable import resolution ([f4ea110](https://github.com/ngngardner/axi-mcp-proxy/commit/f4ea110d694a96c40c21d7bff42e5deb1199896c))
