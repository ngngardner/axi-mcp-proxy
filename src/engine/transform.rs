use anyhow::Result;
use serde_json::Value;

use crate::config::TransformConfig;

/// Apply pick, rename, and filter operations to data.
pub fn apply_transform(data: Value, t: &Option<TransformConfig>) -> Result<Value> {
    let t = match t {
        Some(t) => t,
        None => return Ok(data),
    };

    if let Value::Array(arr) = data {
        let mut result = Vec::new();
        for item in arr {
            if let Some(ref filter) = t.filter
                && !eval_filter(&item, filter)
            {
                continue;
            }
            let mut transformed = item;
            if let Some(ref pick) = t.pick {
                transformed = apply_pick(transformed, pick);
            }
            if let Some(ref rename) = t.rename {
                transformed = apply_rename(transformed, rename);
            }
            result.push(transformed);
        }
        return Ok(Value::Array(result));
    }

    // Single object
    if let Some(ref filter) = t.filter
        && !eval_filter(&data, filter)
    {
        return Ok(Value::Null);
    }

    let mut result = data;
    if let Some(ref pick) = t.pick {
        result = apply_pick(result, pick);
    }
    if let Some(ref rename) = t.rename {
        result = apply_rename(result, rename);
    }
    Ok(result)
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
                picked.insert(leaf.to_string(), v);
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

/// Evaluate a simple filter expression: "field == \"value\"" or "field != \"value\""
fn eval_filter(data: &Value, expr: &str) -> bool {
    let Some(m) = data.as_object() else {
        return true;
    };

    if let Some((field, expected)) = parse_op(expr, "==") {
        return m
            .get(&field)
            .map(|v| value_to_string(v) == expected)
            .unwrap_or(false);
    }

    if let Some((field, expected)) = parse_op(expr, "!=") {
        return m
            .get(&field)
            .map(|v| value_to_string(v) != expected)
            .unwrap_or(true);
    }

    true
}

fn parse_op(expr: &str, op: &str) -> Option<(String, String)> {
    let (left, right) = expr.split_once(op)?;
    let field = left.trim().to_string();
    let expected = right.trim().trim_matches('"').to_string();
    Some((field, expected))
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
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
        });
        let result = apply_transform(data, &t).unwrap();
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
        });
        let result = apply_transform(data, &t).unwrap();
        assert_eq!(result, json!({"new_name": "value"}));
    }

    #[test]
    fn test_filter_equals() {
        let data = json!([{"state": "open"}, {"state": "closed"}]);
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state == "open""#.into()),
        });
        let result = apply_transform(data, &t).unwrap();
        assert_eq!(result, json!([{"state": "open"}]));
    }

    #[test]
    fn test_filter_not_equals() {
        let data = json!([{"state": "open"}, {"state": "closed"}]);
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state != "closed""#.into()),
        });
        let result = apply_transform(data, &t).unwrap();
        assert_eq!(result, json!([{"state": "open"}]));
    }

    #[test]
    fn test_nil_transform() {
        let data = json!({"a": 1});
        let result = apply_transform(data.clone(), &None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_single_object_filter_reject() {
        let data = json!({"state": "closed"});
        let t = Some(TransformConfig {
            pick: None,
            rename: None,
            filter: Some(r#"state == "open""#.into()),
        });
        let result = apply_transform(data, &t).unwrap();
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
        });
        let result = apply_transform(data, &t).unwrap();
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
        assert!(eval_filter(&json!(42), "x == \"1\""));
    }

    #[test]
    fn test_filter_missing_field() {
        assert!(!eval_filter(&json!({"a": 1}), "b == \"1\""));
    }

    #[test]
    fn test_filter_missing_field_not_equals() {
        assert!(eval_filter(&json!({"a": 1}), "b != \"1\""));
    }
}
