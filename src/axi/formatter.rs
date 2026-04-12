use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::config::ToolConfig;
use crate::engine::aggregate::eval_aggregate;
use crate::toon;

/// Assemble the final Axi output: summary line + TOON body + next steps.
///
/// # Errors
///
/// Returns an error if aggregate evaluation fails.
// HashMap is only used internally — no need to be generic over the hasher
#[allow(clippy::implicit_hasher)]
pub fn format(cfg: &ToolConfig, results: &HashMap<String, Value>) -> Result<String> {
    let all_empty = results.values().all(is_empty);
    if all_empty {
        return Ok(cfg.empty_message.clone());
    }

    let mut parts: Vec<String> = Vec::new();

    // Summary line with aggregates
    if !cfg.aggregates.is_empty() {
        // Convert results to use Value strings for aggregate eval
        let mut summary_parts = Vec::new();
        for agg in &cfg.aggregates {
            if let Some(ref parsed) = agg.parsed_value
                && let Ok(val) = eval_aggregate(parsed, results)
            {
                summary_parts.push(format!("{} {}", value_display(&val), agg.label));
            }
        }
        if !summary_parts.is_empty() {
            parts.push(summary_parts.join(" | "));
        }
    }

    // TOON-encoded body
    let body = build_body(cfg, results);
    if !body.is_empty() {
        parts.push(body);
    }

    // Next steps
    if !cfg.next_steps.is_empty() {
        let next_lines: Vec<String> = cfg
            .next_steps
            .iter()
            .map(|ns| format!("→ {} — {}", ns.command, ns.description))
            .collect();
        parts.push(next_lines.join("\n"));
    }

    Ok(parts.join("\n\n"))
}

fn build_body(cfg: &ToolConfig, results: &HashMap<String, Value>) -> String {
    let mut sections = Vec::new();
    let mut seen = HashSet::new();

    for step in &cfg.steps {
        if !seen.insert(&step.name) {
            continue;
        }
        let Some(data) = results.get(&step.name) else {
            continue;
        };
        if is_empty(data) {
            continue;
        }
        // Plain strings (e.g. from run_process stdout) are passed through
        // directly — TOON encoding would JSON-escape newlines and add quotes.
        let rendered = match data {
            Value::String(s) => s.clone(),
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::Array(_)
            | Value::Object(_) => {
                let truncated =
                    truncate_array(data, cfg.max_items.try_into().unwrap_or(usize::MAX));
                toon::encode(&truncated)
            }
        };
        if !rendered.is_empty() {
            sections.push(rendered);
        }
    }

    sections.join("\n\n")
}

fn truncate_array(v: &Value, max: usize) -> Value {
    match v {
        Value::Array(arr) if arr.len() > max => Value::Array(arr[..max].to_vec()),
        Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Object(_)
        | Value::Array(_) => v.clone(),
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(arr) => arr.is_empty(),
        Value::Object(m) => m.is_empty(),
        Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn value_display(v: &Value) -> String {
    match v {
        Value::String(s) if s.contains('\n') => {
            // Multi-line strings (e.g. stdout from run_process) are shown as a
            // line count rather than dumped verbatim into the summary line.
            let lines = s.lines().count();
            format!("{lines} lines")
        }
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                // Exact integer check via trunc — no precision issue for display formatting
                #[allow(clippy::float_arithmetic, clippy::float_cmp)]
                let is_integer = f == f.trunc();
                if is_integer {
                    // Truncation is intentional — we just verified f is an integer
                    #[allow(clippy::cast_possible_truncation)]
                    return format!("{}", f as i64);
                }
            }
            n.to_string()
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_owned(),
        other @ (Value::Array(_) | Value::Object(_)) => other.to_string(),
    }
}

#[cfg(test)]
// Tests use unwrap/to_string/Default::default for brevity — panics are the desired failure mode
#[allow(
    clippy::unwrap_used,
    clippy::str_to_string,
    clippy::default_trait_access
)]
mod tests {
    use super::*;
    use crate::config::AggregateExpr;
    use crate::config::*;
    use serde_json::json;

    fn minimal_tool() -> ToolConfig {
        ToolConfig {
            description: "test".into(),
            detailed_help: None,
            parameters: vec![],
            steps: vec![StepConfig {
                name: "s1".into(),
                upstream: "svc".into(),
                tool: "find".into(),
                args: Default::default(),
                depends_on: vec![],
                transform: None,
            }],
            output_fields: vec![],
            aggregates: vec![AggregateConfig {
                label: "results".into(),
                value: "count($step.s1)".into(),
                parsed_value: Some(AggregateExpr::Count("s1".into())),
            }],
            next_steps: vec![NextStepConfig {
                command: "detail <id>".into(),
                description: "View details".into(),
                when: None,
            }],
            empty_message: "No results.".into(),
            max_items: 10,
        }
    }

    #[test]
    fn test_empty_results() {
        let cfg = minimal_tool();
        let results: HashMap<String, Value> = [("s1".into(), json!([]))].into();
        let output = format(&cfg, &results).unwrap();
        assert_eq!(output, "No results.");
    }

    #[test]
    fn test_nil_result_values() {
        let cfg = minimal_tool();
        let results: HashMap<String, Value> = [("s1".into(), Value::Null)].into();
        let output = format(&cfg, &results).unwrap();
        assert_eq!(output, "No results.");
    }

    #[test]
    fn test_non_empty_results() {
        let cfg = minimal_tool();
        let results: HashMap<String, Value> = [("s1".into(), json!([{"id": 1}, {"id": 2}]))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("2 results"));
        assert!(output.contains("[2]{id}:"));
        assert!(output.contains("→ detail <id>"));
    }

    #[test]
    fn test_max_items_truncates_output() {
        let mut cfg = minimal_tool();
        cfg.max_items = 3;
        // 5 items, but max_items=3 should truncate
        let results: HashMap<String, Value> = [(
            "s1".into(),
            json!([{"id": 1}, {"id": 2}, {"id": 3}, {"id": 4}, {"id": 5}]),
        )]
        .into();
        let output = format(&cfg, &results).unwrap();
        // Aggregate should still reflect the full count (5)
        assert!(output.contains("5 results"));
        // But the TOON body should only have 3 rows
        assert!(output.contains("[3]{id}:"));
        // Rows for id=4 and id=5 should not appear in the body
        let body = output.split("\n\n").nth(1).unwrap();
        assert!(!body.contains('4'));
        assert!(!body.contains('5'));
    }

    #[test]
    fn test_max_items_no_truncation_when_under_limit() {
        let mut cfg = minimal_tool();
        cfg.max_items = 10;
        let results: HashMap<String, Value> = [("s1".into(), json!([{"id": 1}, {"id": 2}]))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("[2]{id}:"));
    }

    #[test]
    fn test_value_display_multiline_string() {
        let v = json!("line1\nline2\nline3");
        assert_eq!(value_display(&v), "3 lines");
    }

    #[test]
    fn test_value_display_single_line_string() {
        let v = json!("hello");
        assert_eq!(value_display(&v), "hello");
    }

    #[test]
    fn test_value_display_integer() {
        let v = json!(42);
        assert_eq!(value_display(&v), "42");
    }

    #[test]
    fn test_value_display_float() {
        let v = json!(2.72);
        assert_eq!(value_display(&v), "2.72");
    }

    #[test]
    fn test_value_display_bool() {
        assert_eq!(value_display(&json!(true)), "true");
        assert_eq!(value_display(&json!(false)), "false");
    }

    #[test]
    fn test_value_display_null() {
        assert_eq!(value_display(&json!(null)), "null");
    }

    #[test]
    fn test_value_display_object() {
        let v = json!({"a": 1});
        let d = value_display(&v);
        assert!(d.contains("\"a\""));
    }

    #[test]
    fn test_build_body_with_string_data() {
        let mut cfg = minimal_tool();
        cfg.aggregates.clear();
        let results: HashMap<String, Value> = [("s1".into(), json!("plain text output"))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("plain text output"));
    }

    #[test]
    fn test_build_body_with_bool_data() {
        let mut cfg = minimal_tool();
        cfg.aggregates.clear();
        let results: HashMap<String, Value> = [("s1".into(), json!(true))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("true"));
    }

    #[test]
    fn test_build_body_with_number_data() {
        let mut cfg = minimal_tool();
        cfg.aggregates.clear();
        let results: HashMap<String, Value> = [("s1".into(), json!(99))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("99"));
    }

    #[test]
    fn test_format_no_aggregates_no_next_steps() {
        let mut cfg = minimal_tool();
        cfg.aggregates.clear();
        cfg.next_steps.clear();
        let results: HashMap<String, Value> = [("s1".into(), json!([{"x": 1}]))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("[1]{x}:"));
        assert!(!output.contains('→'));
    }

    #[test]
    fn test_truncate_array_non_array() {
        let v = json!({"a": 1});
        let result = truncate_array(&v, 5);
        assert_eq!(result, v);
    }

    #[test]
    fn test_is_empty_object() {
        assert!(is_empty(&json!({})));
        assert!(!is_empty(&json!({"a": 1})));
    }
}
