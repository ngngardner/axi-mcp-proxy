use anyhow::{Context, Result, bail};
use nickel_lang_core::error::NullReporter;
use nickel_lang_core::eval::cache::CacheImpl;
use nickel_lang_core::program::Program;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;

use super::types::Config;

// Static regex compilation — pattern is a constant literal, expect cannot fail
#[allow(clippy::expect_used)]
static ENV_VAR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\$\{([^}]+)\}").expect("valid regex"));

/// Load and validate a Nickel config file.
///
/// Evaluates the .ncl file using nickel-lang-core (no subprocess),
/// deserializes into Config, resolves env vars, and runs structural validation.
///
/// # Errors
///
/// Returns an error if the file is not `.ncl`, evaluation fails,
/// deserialization fails, or structural validation fails.
pub fn load(path: &Path) -> Result<Config> {
    let ext = path.extension().and_then(|e| e.to_str());
    if ext != Some("ncl") {
        bail!("unsupported config format: {} (use .ncl)", path.display());
    }

    let import_dir = path
        .parent()
        .context("config file has no parent directory")?;

    let mut prog = Program::<CacheImpl>::new_from_file(path, std::io::stderr(), NullReporter {})
        .context("failed to load nickel program")?;

    // Write bundled axi.ncl to a temp dir so `import "axi.ncl"` resolves
    // for users who don't have lib/ locally (e.g. running via bunx).
    let lib_dir = std::env::temp_dir().join("axi-mcp-proxy-lib");
    std::fs::create_dir_all(&lib_dir)?;
    std::fs::write(lib_dir.join("axi.ncl"), include_str!("../../lib/axi.ncl"))?;

    prog.add_import_paths([import_dir.to_path_buf(), lib_dir].into_iter());

    let value = prog
        .eval_full_for_export()
        .map_err(|err| anyhow::anyhow!("nickel evaluation failed: {err:?}"))?;

    let mut config: Config = Config::deserialize(value).context("failed to deserialize config")?;

    resolve_env_vars(&mut config);
    validate(&config)?;

    Ok(config)
}

/// Expand `${VAR_NAME}` patterns in auth token and header values.
fn resolve_env_vars(config: &mut Config) {
    for upstream in config.upstreams.values_mut() {
        if let Some(ref mut token) = upstream.auth.token {
            *token = expand_env(&ENV_VAR_PATTERN, token);
        }
        if let Some(ref mut headers) = upstream.auth.headers {
            for value in headers.values_mut() {
                *value = expand_env(&ENV_VAR_PATTERN, value);
            }
        }
    }
}

fn expand_env(pattern: &Regex, s: &str) -> String {
    pattern
        .replace_all(s, |caps: &regex::Captures| {
            let var_name = &caps[1];
            std::env::var(var_name).unwrap_or_else(|_| caps[0].to_owned())
        })
        .into_owned()
}

/// Structural validation that Nickel contracts don't cover:
/// - Upstream url/cmd mutual exclusion
/// - Step upstream references exist
/// - No dependency cycles
fn validate(config: &Config) -> Result<()> {
    for (name, upstream) in &config.upstreams {
        let has_url = upstream.url.is_some();
        let has_cmd = upstream.cmd.is_some();
        if !has_url && !has_cmd {
            bail!("upstream {name:?}: must set either url or cmd");
        }
        if has_url && has_cmd {
            bail!("upstream {name:?}: url and cmd are mutually exclusive");
        }
    }

    for (tool_name, tool) in &config.tools {
        for (i, step) in tool.steps.iter().enumerate() {
            if !config.upstreams.contains_key(&step.upstream) {
                bail!(
                    "tool {tool_name:?} step {i} ({:?}): references unknown upstream {:?}",
                    step.name,
                    step.upstream
                );
            }
        }

        check_cycles(&tool.steps).with_context(|| format!("tool {tool_name:?}"))?;

        // Validate next_steps reference known tools
        for ns in &tool.next_steps {
            let referenced_tool = ns.command.split_whitespace().next().unwrap_or("");
            if !config.tools.contains_key(referenced_tool) {
                bail!(
                    "tool {tool_name:?} next_step: command {:?} references unknown tool {:?}",
                    ns.command,
                    referenced_tool
                );
            }
        }
    }

    Ok(())
}

fn check_cycles(steps: &[super::types::StepConfig]) -> Result<()> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    fn visit<'a>(
        name: &'a str,
        deps: &std::collections::HashMap<&'a str, Vec<&'a str>>,
        color: &mut std::collections::HashMap<&'a str, Color>,
    ) -> Result<()> {
        color.insert(name, Color::Gray);
        if let Some(dep_list) = deps.get(name) {
            for &dep in dep_list {
                match color[dep] {
                    Color::Gray => {
                        bail!("dependency cycle detected involving step {dep:?}");
                    }
                    Color::White => {
                        visit(dep, deps, color)?;
                    }
                    Color::Black => {}
                }
            }
        }
        color.insert(name, Color::Black);
        Ok(())
    }

    let names: HashSet<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let mut deps: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for step in steps {
        for dep in &step.depends_on {
            if !names.contains(dep.as_str()) {
                bail!("step {:?} depends on unknown step {:?}", step.name, dep);
            }
            deps.entry(step.name.as_str())
                .or_default()
                .push(dep.as_str());
        }
    }

    let mut color: std::collections::HashMap<&str, Color> =
        names.iter().map(|&n| (n, Color::White)).collect();

    for &name in &names {
        if color[name] == Color::White {
            visit(name, &deps, &mut color)?;
        }
    }

    Ok(())
}

#[cfg(test)]
// Tests use unwrap/expect/to_string/Default::default for brevity — panics are the desired failure mode
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::str_to_string,
    clippy::default_trait_access
)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_ncl(dir: &Path, filename: &str, content: &str) -> std::path::PathBuf {
        let path = dir.join(filename);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn write_axi_ncl(dir: &Path) {
        let axi_content =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("lib/axi.ncl"))
                .unwrap();
        write_ncl(dir, "axi.ncl", &axi_content);
    }

    fn minimal_valid_config() -> &'static str {
        r#"
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
"#
    }

    #[test]
    fn test_load_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        let path = write_ncl(dir.path(), "config.ncl", minimal_valid_config());

        let config = load(&path).expect("should load valid config");
        assert_eq!(config.upstreams.len(), 1);
        assert!(config.upstreams.contains_key("svc"));

        let tool = &config.tools["search"];
        assert_eq!(tool.description, "search tool");
        assert_eq!(tool.steps.len(), 1);
        assert_eq!(tool.steps[0].name, "s1");
        assert_eq!(tool.max_items, 10); // default
        assert_eq!(tool.output_fields.len(), 1);
        assert!(tool.output_fields[0].default_visible); // default = true
    }

    #[test]
    fn test_load_rejects_json() {
        let result = load(Path::new("config.json"));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unsupported config format")
        );
    }

    #[test]
    fn test_contract_rejects_empty_aggregates() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        let config = r#"
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
"#;
        let path = write_ncl(dir.path(), "config.ncl", config);
        let result = load(&path);
        assert!(result.is_err(), "empty aggregates should fail: {result:?}");
    }

    #[test]
    fn test_contract_rejects_empty_next_steps() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        let config = r#"
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    bad = {
      description = "bad tool",
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "y" }],
      next_steps = [],
      empty_message = "none",
    },
  },
} | axi.Config
"#;
        let path = write_ncl(dir.path(), "config.ncl", config);
        let result = load(&path);
        assert!(result.is_err(), "empty next_steps should fail: {result:?}");
    }

    #[test]
    fn test_contract_rejects_too_many_visible_fields() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        // 7 visible fields — contract caps at 6
        let config = r#"
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
"#;
        let path = write_ncl(dir.path(), "config.ncl", config);
        let result = load(&path);
        assert!(result.is_err(), "7 visible fields should fail: {result:?}");
    }

    #[test]
    fn test_validate_unknown_upstream_ref() {
        let config = Config {
            upstreams: std::collections::HashMap::from([(
                "svc".to_string(),
                super::super::types::UpstreamConfig {
                    url: Some("http://localhost".to_string()),
                    cmd: None,
                    args: vec![],
                    auth: Default::default(),
                },
            )]),
            tools: std::collections::HashMap::from([(
                "t1".to_string(),
                super::super::types::ToolConfig {
                    description: "d".to_string(),
                    detailed_help: None,
                    parameters: vec![],
                    steps: vec![super::super::types::StepConfig {
                        name: "s1".to_string(),
                        upstream: "nonexistent".to_string(),
                        tool: "x".to_string(),
                        args: Default::default(),
                        depends_on: vec![],
                        transform: None,
                    }],
                    output_fields: vec![],
                    aggregates: vec![],
                    next_steps: vec![],
                    empty_message: "none".to_string(),
                    max_items: 10,
                },
            )]),
        };
        let result = validate(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown upstream"));
    }

    #[test]
    fn test_validate_upstream_missing_transport() {
        let config = Config {
            upstreams: std::collections::HashMap::from([(
                "svc".to_string(),
                super::super::types::UpstreamConfig {
                    url: None,
                    cmd: None,
                    args: vec![],
                    auth: Default::default(),
                },
            )]),
            tools: std::collections::HashMap::new(),
        };
        let result = validate(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must set either url or cmd")
        );
    }

    #[test]
    fn test_validate_upstream_both_transports() {
        let config = Config {
            upstreams: std::collections::HashMap::from([(
                "svc".to_string(),
                super::super::types::UpstreamConfig {
                    url: Some("http://x".to_string()),
                    cmd: Some("y".to_string()),
                    args: vec![],
                    auth: Default::default(),
                },
            )]),
            tools: std::collections::HashMap::new(),
        };
        let result = validate(&config);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("mutually exclusive")
        );
    }

    #[test]
    fn test_cycle_detection() {
        let steps = vec![
            super::super::types::StepConfig {
                name: "a".to_string(),
                upstream: "svc".to_string(),
                tool: "x".to_string(),
                args: Default::default(),
                depends_on: vec!["b".to_string()],
                transform: None,
            },
            super::super::types::StepConfig {
                name: "b".to_string(),
                upstream: "svc".to_string(),
                tool: "x".to_string(),
                args: Default::default(),
                depends_on: vec!["a".to_string()],
                transform: None,
            },
        ];
        let result = check_cycles(&steps);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_validate_unknown_next_step_tool() {
        let config = Config {
            upstreams: std::collections::HashMap::from([(
                "svc".to_string(),
                super::super::types::UpstreamConfig {
                    url: Some("http://localhost".to_string()),
                    cmd: None,
                    args: vec![],
                    auth: Default::default(),
                },
            )]),
            tools: std::collections::HashMap::from([(
                "search".to_string(),
                super::super::types::ToolConfig {
                    description: "d".to_string(),
                    detailed_help: None,
                    parameters: vec![],
                    steps: vec![super::super::types::StepConfig {
                        name: "s1".to_string(),
                        upstream: "svc".to_string(),
                        tool: "x".to_string(),
                        args: Default::default(),
                        depends_on: vec![],
                        transform: None,
                    }],
                    output_fields: vec![],
                    aggregates: vec![],
                    next_steps: vec![super::super::types::NextStepConfig {
                        command: "nonexistent_tool arg1".to_string(),
                        description: "bad ref".to_string(),
                        when: None,
                    }],
                    empty_message: "none".to_string(),
                    max_items: 10,
                },
            )]),
        };
        let result = validate(&config);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("unknown tool"),
            "expected 'unknown tool' in: {msg}"
        );
        assert!(
            msg.contains("nonexistent_tool"),
            "expected tool name in: {msg}"
        );
    }

    #[test]
    #[allow(unsafe_code)]
    fn test_env_var_expansion() {
        // SAFETY: test runs single-threaded; set_var is unsafe in edition 2024
        unsafe { std::env::set_var("AXI_TEST_TOKEN", "secret123") };
        assert_eq!(
            expand_env(&ENV_VAR_PATTERN, "Bearer ${AXI_TEST_TOKEN}"),
            "Bearer secret123"
        );
        // SAFETY: cleanup, test runs single-threaded
        unsafe { std::env::remove_var("AXI_TEST_TOKEN") };
    }

    #[test]
    #[allow(unsafe_code)]
    fn test_env_var_expansion_missing() {
        // SAFETY: test runs single-threaded; remove_var is unsafe in edition 2024
        unsafe { std::env::remove_var("MISSING_VAR_AXI_TEST") };
        assert_eq!(
            expand_env(&ENV_VAR_PATTERN, "${MISSING_VAR_AXI_TEST}"),
            "${MISSING_VAR_AXI_TEST}"
        );
    }

    #[test]
    fn test_contract_rejects_invalid_param_type() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        let config = r#"
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080" } },
  tools = {
    search = {
      description = "search tool",
      parameters = [
        { name = "q", type = "integer", description = "query", required = true },
      ],
      steps = [{ name = "s1", upstream = "svc", tool = "find", args = {} }],
      output_fields = [{ name = "id", description = "ID" }],
      aggregates = [{ label = "x", value = "count($step.s1)" }],
      next_steps = [{ command = "search", description = "y" }],
      empty_message = "none",
    },
  },
} | axi.Config
"#;
        let path = write_ncl(dir.path(), "config.ncl", config);
        let result = load(&path);
        assert!(
            result.is_err(),
            "invalid param type should fail: {result:?}"
        );
    }

    #[test]
    fn test_contract_rejects_invalid_auth_type() {
        let dir = tempfile::tempdir().unwrap();
        write_axi_ncl(dir.path());
        let config = r#"
let axi = import "axi.ncl" in
{
  upstreams = { svc = { url = "http://localhost:8080", auth = { type = "invalid" } } },
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
"#;
        let path = write_ncl(dir.path(), "config.ncl", config);
        let result = load(&path);
        assert!(result.is_err(), "invalid auth type should fail: {result:?}");
    }
}
