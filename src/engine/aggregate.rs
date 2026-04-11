use crate::config::AggregateExpr;
use crate::engine::resolve::traverse_path;
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Evaluate a pre-parsed aggregate expression against step results.
///
/// # Errors
///
/// Returns an error if the referenced step is not found.
#[allow(clippy::implicit_hasher)]
pub fn eval_aggregate(expr: &AggregateExpr, results: &HashMap<String, Value>) -> Result<Value> {
    match expr {
        AggregateExpr::Count(path) => {
            let val = traverse_path(path, results)?;
            let count = match &val {
                Value::Array(arr) => arr.len(),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Object(_) => 0,
            };
            Ok(Value::Number(count.into()))
        }
        AggregateExpr::Sum(path) => {
            let val = traverse_path(path, results)?;
            let sum = match &val {
                Value::Array(arr) => arr.iter().filter_map(Value::as_f64).sum::<f64>(),
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Object(_) => 0.0,
            };
            Ok(serde_json::json!(sum))
        }
        AggregateExpr::Direct(path) => traverse_path(path, results),
    }
}

#[cfg(test)]
// Tests use unwrap/to_string for brevity — panics are the desired failure mode
#[allow(clippy::unwrap_used, clippy::str_to_string)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_count() {
        let results: HashMap<String, Value> = [("items".to_string(), json!([1, 2, 3]))].into();
        let expr = AggregateExpr::Count("items".into());
        let val = eval_aggregate(&expr, &results).unwrap();
        assert_eq!(val, json!(3));
    }

    #[test]
    fn test_count_non_array() {
        let results: HashMap<String, Value> = [("x".to_string(), json!("not an array"))].into();
        let expr = AggregateExpr::Count("x".into());
        let val = eval_aggregate(&expr, &results).unwrap();
        assert_eq!(val, json!(0));
    }

    #[test]
    fn test_sum() {
        let results: HashMap<String, Value> = [("nums".to_string(), json!([1.0, 2.5, 3.5]))].into();
        let expr = AggregateExpr::Sum("nums".into());
        let val = eval_aggregate(&expr, &results).unwrap();
        assert_eq!(val, json!(7.0));
    }

    #[test]
    fn test_sum_non_array() {
        let results: HashMap<String, Value> = [("x".to_string(), json!("not an array"))].into();
        let expr = AggregateExpr::Sum("x".into());
        let val = eval_aggregate(&expr, &results).unwrap();
        assert_eq!(val, json!(0.0));
    }

    #[test]
    fn test_direct_ref() {
        let results: HashMap<String, Value> = [("s1".to_string(), json!({"status": "ok"}))].into();
        let expr = AggregateExpr::Direct("s1.status".into());
        let val = eval_aggregate(&expr, &results).unwrap();
        assert_eq!(val, json!("ok"));
    }

    #[test]
    fn test_count_missing_step() {
        let results = HashMap::new();
        let expr = AggregateExpr::Count("missing".into());
        let result = eval_aggregate(&expr, &results);
        assert!(result.is_err());
    }

    #[test]
    fn test_direct_ref_missing() {
        let results = HashMap::new();
        let expr = AggregateExpr::Direct("missing.field".into());
        let result = eval_aggregate(&expr, &results);
        assert!(result.is_err());
    }
}
