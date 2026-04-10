use anyhow::{Result, bail};
use serde_json::Value;
use std::collections::HashMap;

/// Replace `$param.X` and `$step.Y.Z` references in args.
///
/// # Errors
///
/// Returns an error if a referenced parameter or step is not found.
// HashMap is only used internally — no need to be generic over the hasher
#[allow(clippy::implicit_hasher)]
pub fn resolve_args(
    args: &HashMap<String, Value>,
    params: &HashMap<String, Value>,
    results: &HashMap<String, Value>,
) -> Result<HashMap<String, Value>> {
    let mut resolved = HashMap::with_capacity(args.len());
    for (k, v) in args {
        let rv = resolve_value(v, params, results)?;
        resolved.insert(k.clone(), rv);
    }
    Ok(resolved)
}

fn resolve_value(
    v: &Value,
    params: &HashMap<String, Value>,
    results: &HashMap<String, Value>,
) -> Result<Value> {
    match v {
        Value::String(s) => resolve_string(s, params, results),
        Value::Object(m) => {
            let mut resolved = serde_json::Map::with_capacity(m.len());
            for (k, inner) in m {
                let rv = resolve_value(inner, params, results)?;
                resolved.insert(k.clone(), rv);
            }
            Ok(Value::Object(resolved))
        }
        Value::Array(arr) => {
            let mut resolved = Vec::with_capacity(arr.len());
            for inner in arr {
                resolved.push(resolve_value(inner, params, results)?);
            }
            Ok(Value::Array(resolved))
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(v.clone()),
    }
}

fn resolve_string(
    s: &str,
    params: &HashMap<String, Value>,
    results: &HashMap<String, Value>,
) -> Result<Value> {
    if let Some(name) = s.strip_prefix("$param.") {
        return params
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown parameter: {name}"));
    }
    if let Some(path) = s.strip_prefix("$step.") {
        return traverse_path(path, results);
    }
    Ok(Value::String(s.to_owned()))
}

/// Traverse a dotted path like `"step_name.field"` into a nested `Value`.
///
/// # Errors
///
/// Returns an error if a segment is not found or a non-object is encountered mid-path.
// HashMap is only used internally — no need to be generic over the hasher
#[allow(clippy::implicit_hasher)]
pub fn traverse_path(path: &str, data: &HashMap<String, Value>) -> Result<Value> {
    let (first, rest) = match path.split_once('.') {
        Some((f, r)) => (f, Some(r)),
        None => (path, None),
    };
    let val = data
        .get(first)
        .ok_or_else(|| anyhow::anyhow!("step {first:?} not found in results"))?;
    match rest {
        None => Ok(val.clone()),
        Some(remaining) => match val {
            Value::Object(m) => {
                let map: HashMap<String, Value> =
                    m.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                traverse_path(remaining, &map)
            }
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Array(_) => bail!("cannot traverse into non-object at {first:?}"),
        },
    }
}

#[cfg(test)]
// Tests use unwrap/to_string for brevity — panics are the desired failure mode
#[allow(clippy::unwrap_used, clippy::str_to_string)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_param_ref() {
        let args: HashMap<String, Value> = [("owner".to_string(), json!("$param.owner"))].into();
        let params: HashMap<String, Value> = [("owner".to_string(), json!("alice"))].into();
        let results = HashMap::new();
        let resolved = resolve_args(&args, &params, &results).unwrap();
        assert_eq!(resolved["owner"], json!("alice"));
    }

    #[test]
    fn test_step_ref() {
        let args: HashMap<String, Value> = [("id".to_string(), json!("$step.s1.id"))].into();
        let params = HashMap::new();
        let results: HashMap<String, Value> = [("s1".to_string(), json!({"id": 42}))].into();
        let resolved = resolve_args(&args, &params, &results).unwrap();
        assert_eq!(resolved["id"], json!(42));
    }

    #[test]
    fn test_unknown_param() {
        let args: HashMap<String, Value> = [("x".to_string(), json!("$param.missing"))].into();
        let params = HashMap::new();
        let results = HashMap::new();
        let result = resolve_args(&args, &params, &results);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown parameter")
        );
    }

    #[test]
    fn test_plain_string_passthrough() {
        let args: HashMap<String, Value> = [("x".to_string(), json!("hello"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved["x"], json!("hello"));
    }

    #[test]
    fn test_non_string_passthrough() {
        let args: HashMap<String, Value> = [("n".to_string(), json!(42))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved["n"], json!(42));
    }

    #[test]
    fn test_nested_map_with_refs() {
        let args: HashMap<String, Value> =
            [("nested".to_string(), json!({"owner": "$param.owner"}))].into();
        let params: HashMap<String, Value> = [("owner".to_string(), json!("bob"))].into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(resolved["nested"], json!({"owner": "bob"}));
    }

    #[test]
    fn test_array_with_refs() {
        let args: HashMap<String, Value> =
            [("items".to_string(), json!(["$param.a", "$param.b"]))].into();
        let params: HashMap<String, Value> =
            [("a".to_string(), json!(1)), ("b".to_string(), json!(2))].into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(resolved["items"], json!([1, 2]));
    }

    #[test]
    fn test_traverse_path_not_found() {
        let data = HashMap::new();
        let result = traverse_path("missing", &data);
        assert!(result.is_err());
    }

    #[test]
    fn test_traverse_path_non_object() {
        let data: HashMap<String, Value> = [("x".to_string(), json!(42))].into();
        let result = traverse_path("x.y", &data);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-object"));
    }
}
