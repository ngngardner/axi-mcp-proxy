use anyhow::Result;
use serde_json::Value;

use crate::config::{FilterExpr, FilterOp, TransformConfig};

/// Apply filter, pick, rename, and truncate operations to data.
///
/// When `full` is true, the truncate step is skipped (principle 3: `--full` escape hatch).
///
/// # Errors
///
/// Currently infallible but returns `Result` for future extensibility.
pub fn apply_transform(data: Value, t: &Option<TransformConfig>, full: bool) -> Result<Value> {
    let Some(transform) = t else {
        return Ok(data);
    };

    if let Value::Array(arr) = data {
        let mut result = Vec::new();
        for item in arr {
            if let Some(ref filter) = transform.parsed_filter
                && !eval_filter(&item, filter)
            {
                continue;
            }
            let transformed = apply_item(item, transform, full);
            result.push(transformed);
        }
        return Ok(Value::Array(result));
    }

    // Single object
    if let Some(ref filter) = transform.parsed_filter
        && !eval_filter(&data, filter)
    {
        return Ok(Value::Null);
    }

    Ok(apply_item(data, transform, full))
}

fn apply_item(data: Value, transform: &TransformConfig, full: bool) -> Value {
    let mut result = data;
    if let Some(ref pick) = transform.pick {
        result = apply_pick(result, pick);
    }
    if let Some(ref rename) = transform.rename {
        result = apply_rename(result, rename);
    }
    if !full && let Some(ref truncate) = transform.truncate {
        result = apply_truncate(result, truncate);
    }
    result
}

fn apply_pick(data: Value, fields: &[String]) -> Value {
    let Some(m) = data.as_object() else {
        return data;
    };
    let mut picked = serde_json::Map::with_capacity(fields.len());
    for f in fields {
        if let Some((head, tail)) = f.split_once('.') {
            // Dotted path: "user.login" → pick m["user"]["login"], insert as "login"
            if let Some(parent) = m.get(head)
                && let Some(v) = resolve_path(parent, tail)
            {
                let leaf = f.rsplit('.').next().unwrap_or(f);
                picked.insert(leaf.to_owned(), v);
            }
        } else if let Some(v) = m.get(f) {
            picked.insert(f.clone(), v.clone());
        }
    }
    Value::Object(picked)
}

/// Resolve a dotted path like "a.b.c" into a nested Value.
fn resolve_path(val: &Value, path: &str) -> Option<Value> {
    let mut current = val;
    for segment in path.split('.') {
        current = current.as_object()?.get(segment)?;
    }
    Some(current.clone())
}

fn apply_rename(data: Value, renames: &std::collections::HashMap<String, String>) -> Value {
    let Some(m) = data.as_object() else {
        return data;
    };
    let mut result = serde_json::Map::with_capacity(m.len());
    for (k, v) in m {
        let new_key = renames.get(k).unwrap_or(k);
        result.insert(new_key.clone(), v.clone());
    }
    Value::Object(result)
}

fn apply_truncate(data: Value, limits: &std::collections::HashMap<String, usize>) -> Value {
    let Some(m) = data.as_object() else {
        return data;
    };
    let mut result = m.clone();
    for (field, &max) in limits {
        let needs_truncate = result
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|s| s.len() > max || s.contains('\n'));
        if needs_truncate {
            let s = result[field].as_str().unwrap_or_default();
            result.insert(field.clone(), Value::String(truncate_string(s, max)));
        }
    }
    Value::Object(result)
}

/// Truncate a string: first at the earliest newline, then at `max` chars.
fn truncate_string(s: &str, max: usize) -> String {
    let first_line = s.split('\n').next().unwrap_or(s);
    if first_line.len() <= max {
        return first_line.to_owned();
    }
    // Truncate at char boundary
    first_line.chars().take(max).collect()
}

fn eval_filter(data: &Value, expr: &FilterExpr) -> bool {
    let Some(m) = data.as_object() else {
        return true;
    };
    match expr.op {
        FilterOp::Eq => m
            .get(&expr.field)
            .is_some_and(|v| value_to_string(v) == expr.value),
        FilterOp::Ne => m
            .get(&expr.field)
            .is_none_or(|v| value_to_string(v) != expr.value),
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_owned(),
        other @ (Value::Array(_) | Value::Object(_)) => other.to_string(),
    }
}

#[cfg(test)]
// Tests use unwrap/to_string for brevity — panics are the desired failure mode
#[allow(clippy::unwrap_used, clippy::str_to_string)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_pick_on_array() {
        let data =
            json!([{"id": 1, "name": "a", "extra": true}, {"id": 2, "name": "b", "extra": false}]);
        let t = Some(TransformConfig {
            pick: Some(vec!["id".into(), "name".into()]),
            rename: None,
            filter: None,
            truncate: None,
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(
            result,
            json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}])
        );
    }

    #[test]
    fn test_rename_on_object() {
        let data = json!({"old_name": "value"});
        let t = Some(TransformConfig {
            pick: None,
            rename: Some(HashMap::from([("old_name".into(), "new_name".into())])),
            filter: None,
            truncate: None,
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!({"new_name": "value"}));
    }

    #[test]
    fn test_filter_equals() {
        let data = json!([{"state": "open"}, {"state": "closed"}]);
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state == "open""#.into()),
            truncate: None,
            parsed_filter: Some(FilterExpr {
                field: "state".into(),
                op: FilterOp::Eq,
                value: "open".into(),
            }),
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!([{"state": "open"}]));
    }

    #[test]
    fn test_filter_not_equals() {
        let data = json!([{"state": "open"}, {"state": "closed"}]);
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state != "closed""#.into()),
            truncate: None,
            parsed_filter: Some(FilterExpr {
                field: "state".into(),
                op: FilterOp::Ne,
                value: "closed".into(),
            }),
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!([{"state": "open"}]));
    }

    #[test]
    fn test_nil_transform() {
        let data = json!({"a": 1});
        let result = apply_transform(data.clone(), &None, false).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_single_object_filter_reject() {
        let data = json!({"state": "closed"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state == "open""#.into()),
            truncate: None,
            parsed_filter: Some(FilterExpr {
                field: "state".into(),
                op: FilterOp::Eq,
                value: "open".into(),
            }),
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_pick_dotted_path() {
        let data = json!({"user": {"login": "alice", "id": 1}, "title": "Fix bug"});
        let result = apply_pick(data, &["title".into(), "user.login".into()]);
        assert_eq!(result, json!({"title": "Fix bug", "login": "alice"}));
    }

    #[test]
    fn test_pick_deep_dotted_path() {
        let data = json!({"a": {"b": {"c": 42}}});
        let result = apply_pick(data, &["a.b.c".into()]);
        assert_eq!(result, json!({"c": 42}));
    }

    #[test]
    fn test_pick_dotted_missing() {
        let data = json!({"user": {"login": "alice"}});
        let result = apply_pick(data, &["user.missing".into()]);
        assert_eq!(result, json!({}));
    }

    #[test]
    fn test_pick_dotted_produces_flat_tabular_data() {
        // Regression: nested objects prevent TOON tabular encoding.
        // Dotted picks must flatten to primitives so the result is tabular.
        let data = json!([
            {"number": 1, "user": {"login": "alice", "id": 10}},
            {"number": 2, "user": {"login": "bob", "id": 20}},
        ]);
        let t = Some(TransformConfig {
            pick: Some(vec!["number".into(), "user.login".into()]),
            rename: None,
            filter: None,
            truncate: None,
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        // Every value should be a primitive (no nested objects/arrays)
        for item in result.as_array().unwrap() {
            for (_k, v) in item.as_object().unwrap() {
                assert!(
                    !v.is_object() && !v.is_array(),
                    "dotted pick should produce flat values, got: {v}"
                );
            }
        }
        assert_eq!(
            result,
            json!([{"number": 1, "login": "alice"}, {"number": 2, "login": "bob"}])
        );
    }

    #[test]
    fn test_pick_non_map() {
        assert_eq!(apply_pick(json!(42), &["a".into()]), json!(42));
    }

    #[test]
    fn test_rename_non_map() {
        let renames = HashMap::from([("a".into(), "b".into())]);
        assert_eq!(apply_rename(json!(42), &renames), json!(42));
    }

    #[test]
    fn test_filter_non_map() {
        let expr = FilterExpr {
            field: "x".into(),
            op: FilterOp::Eq,
            value: "1".into(),
        };
        assert!(eval_filter(&json!(42), &expr));
    }

    #[test]
    fn test_filter_missing_field() {
        let expr = FilterExpr {
            field: "b".into(),
            op: FilterOp::Eq,
            value: "1".into(),
        };
        assert!(!eval_filter(&json!({"a": 1}), &expr));
    }

    #[test]
    fn test_filter_missing_field_not_equals() {
        let expr = FilterExpr {
            field: "b".into(),
            op: FilterOp::Ne,
            value: "1".into(),
        };
        assert!(eval_filter(&json!({"a": 1}), &expr));
    }

    #[test]
    fn test_truncate_at_max_len() {
        let data = json!([{"sha": "abcdef1234567890", "msg": "short"}]);
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("sha".into(), 7)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!([{"sha": "abcdef1", "msg": "short"}]));
    }

    #[test]
    fn test_truncate_at_newline() {
        let data = json!({"msg": "first line\n\nsecond paragraph"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("msg".into(), 200)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!({"msg": "first line"}));
    }

    #[test]
    fn test_truncate_no_op_when_shorter() {
        let data = json!({"name": "hello"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("name".into(), 100)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!({"name": "hello"}));
    }

    #[test]
    fn test_truncate_non_string_unchanged() {
        let data = json!({"count": 12345});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("count".into(), 3)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!({"count": 12345}));
    }

    #[test]
    fn test_truncate_missing_field() {
        let data = json!({"a": "hello"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("missing".into(), 5)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, false).unwrap();
        assert_eq!(result, json!({"a": "hello"}));
    }

    #[test]
    fn test_truncate_skipped_when_full() {
        let data = json!({"msg": "first line\n\nsecond paragraph"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: None,
            truncate: Some(HashMap::from([("msg".into(), 20)])),
            parsed_filter: None,
        });
        let result = apply_transform(data, &t, true).unwrap();
        assert_eq!(result, json!({"msg": "first line\n\nsecond paragraph"}));
    }
}
