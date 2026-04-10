use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::HashMap;

use crate::engine::resolve::traverse_path;

/// Evaluate an aggregate expression against step results.
/// Supports: count($step.X), sum($step.X.field), $step.X.Y (direct ref)
pub fn eval_aggregate(expr: &str, results: &HashMap<String, Value>) -> Result<Value> {
    if let Some(inner) = expr
        .strip_prefix("count(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let val = resolve_ref(inner, results)?;
        let count = match &val {
            Value::Array(arr) => arr.len(),
            _ => 0,
        };
        return Ok(Value::Number(count.into()));
    }

    if let Some(inner) = expr.strip_prefix("sum(").and_then(|s| s.strip_suffix(')')) {
        let val = resolve_ref(inner, results)?;
        let sum = match &val {
            Value::Array(arr) => arr.iter().filter_map(|item| item.as_f64()).sum::<f64>(),
            _ => 0.0,
        };
        return Ok(serde_json::json!(sum));
    }

    if expr.starts_with("$step.") {
        return resolve_ref(expr, results);
    }

    bail!("unknown aggregate expression: {expr}");
}

fn resolve_ref(reference: &str, results: &HashMap<String, Value>) -> Result<Value> {
    let path = reference
        .strip_prefix("$step.")
        .ok_or_else(|| anyhow::anyhow!("invalid reference: {reference}"))?;
    traverse_path(path, results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_count() {
        let results: HashMap<String, Value> = [("items".to_string(), json!([1, 2, 3]))].into();
        let val = eval_aggregate("count($step.items)", &results).unwrap();
        assert_eq!(val, json!(3));
    }

    #[test]
    fn test_count_non_array() {
        let results: HashMap<String, Value> = [("x".to_string(), json!("not an array"))].into();
        let val = eval_aggregate("count($step.x)", &results).unwrap();
        assert_eq!(val, json!(0));
    }

    #[test]
    fn test_sum() {
        let results: HashMap<String, Value> = [("nums".to_string(), json!([1.0, 2.5, 3.5]))].into();
        let val = eval_aggregate("sum($step.nums)", &results).unwrap();
        assert_eq!(val, json!(7.0));
    }

    #[test]
    fn test_sum_non_array() {
        let results: HashMap<String, Value> = [("x".to_string(), json!("not an array"))].into();
        let val = eval_aggregate("sum($step.x)", &results).unwrap();
        assert_eq!(val, json!(0.0));
    }

    #[test]
    fn test_direct_ref() {
        let results: HashMap<String, Value> = [("s1".to_string(), json!({"status": "ok"}))].into();
        let val = eval_aggregate("$step.s1.status", &results).unwrap();
        assert_eq!(val, json!("ok"));
    }

    #[test]
    fn test_unknown_expr() {
        let results = HashMap::new();
        let result = eval_aggregate("bad_expr", &results);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown aggregate")
        );
    }

    #[test]
    fn test_count_missing_step() {
        let results = HashMap::new();
        let result = eval_aggregate("count($step.missing)", &results);
        assert!(result.is_err());
    }

    #[test]
    fn test_direct_ref_missing() {
        let results = HashMap::new();
        let result = eval_aggregate("$step.missing.field", &results);
        assert!(result.is_err());
    }
}
