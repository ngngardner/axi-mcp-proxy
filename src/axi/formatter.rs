use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::config::ToolConfig;
use crate::engine::aggregate::eval_aggregate;
use crate::toon;

/// Assemble the final Axi output: summary line + TOON body + next steps.
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
            if let Ok(val) = eval_aggregate(&agg.value, results) {
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
        let truncated = truncate_array(data, cfg.max_items as usize);
        let encoded = toon::encode(&truncated);
        if !encoded.is_empty() {
            sections.push(encoded);
        }
    }

    sections.join("\n\n")
}

fn truncate_array(v: &Value, max: usize) -> Value {
    match v {
        Value::Array(arr) if arr.len() > max => Value::Array(arr[..max].to_vec()),
        _ => v.clone(),
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(arr) => arr.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

fn value_display(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(f) = n.as_f64()
                && f == f.trunc()
            {
                return format!("{}", f as i64);
            }
            n.to_string()
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(!body.contains("4"));
        assert!(!body.contains("5"));
    }

    #[test]
    fn test_max_items_no_truncation_when_under_limit() {
        let mut cfg = minimal_tool();
        cfg.max_items = 10;
        let results: HashMap<String, Value> = [("s1".into(), json!([{"id": 1}, {"id": 2}]))].into();
        let output = format(&cfg, &results).unwrap();
        assert!(output.contains("[2]{id}:"));
    }
}
