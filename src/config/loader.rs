use anyhow::{Context, Result, bail};
use nickel_lang_core::error::NullReporter;
use nickel_lang_core::eval::cache::CacheImpl;
use nickel_lang_core::program::Program;
use regex::Regex;
use std::collections::HashSet;
use std::path::Path;
use std::sync::LazyLock;

use serde::Deserialize;

use super::types::{AggregateExpr, Config, FilterExpr};

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
    validate(&mut config)?;

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
/// - Step upstream references exist
/// - No dependency cycles
fn validate(config: &mut Config) -> Result<()> {
    // Immutable checks first
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

        validate_arg_refs(tool_name, tool)?;

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

    // Mutable parsing pass
    for (tool_name, tool) in &mut config.tools {
        for (i, step) in tool.steps.iter_mut().enumerate() {
            if let Some(ref mut transform) = step.transform
                && let Some(ref filter_str) = transform.filter
            {
                let parsed = FilterExpr::parse(filter_str).with_context(|| {
                    format!(
                        "tool {tool_name:?} step {i} ({:?}): invalid filter",
                        step.name
                    )
                })?;
                transform.parsed_filter = Some(parsed);
            }
        }

        for agg in &mut tool.aggregates {
            let parsed = AggregateExpr::parse(&agg.value).with_context(|| {
                format!(
                    "tool {tool_name:?}: invalid aggregate expression {:?}",
                    agg.value
                )
            })?;
            agg.parsed_value = Some(parsed);
        }
    }

    Ok(())
}

/// Validate that `$param.X` and `$step.Y` references in step args
/// refer to declared parameter names and available step names.
fn validate_arg_refs(tool_name: &str, tool: &super::types::ToolConfig) -> Result<()> {
    let param_names: HashSet<&str> = tool.parameters.iter().map(|p| p.name.as_str()).collect();

    // Steps available to step i: all steps defined before it + depends_on
    let step_names: Vec<&str> = tool.steps.iter().map(|s| s.name.as_str()).collect();

    for (i, step) in tool.steps.iter().enumerate() {
        // Available steps: those defined before this one, plus explicit depends_on
        let mut available: HashSet<&str> = step_names[..i].iter().copied().collect();
        for dep in &step.depends_on {
            available.insert(dep.as_str());
        }

        for (key, value) in &step.args {
            check_value_refs(tool_name, &step.name, key, value, &param_names, &available)?;
        }
    }

    Ok(())
}

/// Recursively scan a JSON value for `$param.X` and `$step.Y` references.
fn check_value_refs(
    tool_name: &str,
    step_name: &str,
    key: &str,
    value: &serde_json::Value,
    param_names: &HashSet<&str>,
    step_names: &HashSet<&str>,
) -> Result<()> {
    match value {
        serde_json::Value::String(s) => {
            check_string_refs(tool_name, step_name, key, s, param_names, step_names)
        }
        serde_json::Value::Object(m) => {
            for (k, v) in m {
                check_value_refs(tool_name, step_name, k, v, param_names, step_names)?;
            }
            Ok(())
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                check_value_refs(tool_name, step_name, key, v, param_names, step_names)?;
            }
            Ok(())
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            Ok(())
        }
    }
}

/// Scan a string for `$param.X` and `$step.Y` references and validate them.
fn check_string_refs(
    tool_name: &str,
    step_name: &str,
    key: &str,
    s: &str,
    param_names: &HashSet<&str>,
    step_names: &HashSet<&str>,
) -> Result<()> {
    let mut rest = s;
    while let Some(pos) = rest.find('$') {
        rest = &rest[pos..];
        if let Some(raw) = extract_ref(rest, "$param.") {
            let name = raw.strip_suffix('?').unwrap_or(raw);
            // Strip any dotted path segments — only the base name matters
            let base = name.split('.').next().unwrap_or(name);
            if !param_names.contains(base) {
                bail!(
                    "tool {tool_name:?} step {step_name:?} arg {key:?}: \
                     references undeclared parameter {base:?}"
                );
            }
            rest = &rest["$param.".len() + raw.len()..];
        } else if let Some(raw) = extract_ref(rest, "$step.") {
            let step_ref = raw.split('.').next().unwrap_or(raw);
            if !step_names.contains(step_ref) {
                bail!(
                    "tool {tool_name:?} step {step_name:?} arg {key:?}: \
                     references unavailable step {step_ref:?}"
                );
            }
            rest = &rest["$step.".len() + raw.len()..];
        } else {
            rest = &rest[1..];
        }
    }
    Ok(())
}

/// Extract a dotted identifier after a prefix like `$param.` or `$step.`.
/// Similar to `try_extract_ref` in resolve.rs but simplified for validation.
fn extract_ref<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let after = s.strip_prefix(prefix)?;
    let end = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .unwrap_or(after.len());
    if end == 0 {
        return None;
    }
    // Include a trailing `?` as optional-param marker
    let has_trailing_q = after.as_bytes().get(end) == Some(&b'?')
        && after
            .as_bytes()
            .get(end + 1)
            .is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'.');
    let ref_end = if has_trailing_q { end + 1 } else { end };
    let name = after[..ref_end].trim_end_matches('.');
    if name.is_empty() { None } else { Some(name) }
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
    fn test_validate_unknown_upstream_ref() {
        let mut config = Config {
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
        let result = validate(&mut config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown upstream"));
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
        let mut config = Config {
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
        let result = validate(&mut config);
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
}
