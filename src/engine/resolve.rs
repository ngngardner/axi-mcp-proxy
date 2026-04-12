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
    // Drop keys that resolved to null (e.g. absent optional params)
    resolved.retain(|_, v| !v.is_null());
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
    // Exact match: entire string is a single reference → return typed value
    if let Some(raw_name) = s.strip_prefix("$param.")
        && !raw_name.contains("$param.")
        && !raw_name.contains("$step.")
    {
        let (name, _optional) = parse_optional(raw_name);
        // Missing param → null (dropped by resolve_args retain),
        // so upstream runs with no value — principle 5: definitive empty state
        return params
            .get(name)
            .map_or_else(|| Ok(Value::Null), |v| Ok(v.clone()));
    }
    if let Some(path) = s.strip_prefix("$step.")
        && !path.contains("$param.")
        && !path.contains("$step.")
    {
        return traverse_path(path, results);
    }

    // Interpolation: string contains embedded $param.X or $step.X.Y references
    if s.contains("$param.") || s.contains("$step.") {
        return interpolate_string(s, params, results);
    }

    Ok(Value::String(s.to_owned()))
}

/// Replace all `$param.X` and `$step.X.Y.Z` references within a string,
/// converting resolved values to their string representation.
fn interpolate_string(
    s: &str,
    params: &HashMap<String, Value>,
    results: &HashMap<String, Value>,
) -> Result<Value> {
    let mut output = String::with_capacity(s.len());
    let mut rest = s;

    while !rest.is_empty() {
        if let Some(pos) = rest.find('$') {
            output.push_str(&rest[..pos]);
            rest = &rest[pos..];

            if let Some(raw_name) = try_extract_ref(rest, "$param.") {
                let (name, _optional) = parse_optional(raw_name);
                if let Some(val) = params.get(name) {
                    output.push_str(&value_to_string(val));
                }
                rest = &rest["$param.".len() + raw_name.len()..];
            } else if let Some(path) = try_extract_ref(rest, "$step.") {
                let val = traverse_path(path, results)?;
                output.push_str(&value_to_string(&val));
                rest = &rest["$step.".len() + path.len()..];
            } else {
                output.push('$');
                rest = &rest[1..];
            }
        } else {
            output.push_str(rest);
            break;
        }
    }

    Ok(Value::String(output))
}

/// Extract a dotted identifier after a prefix like `$param.` or `$step.`.
/// Identifiers consist of alphanumeric chars, underscores, and dots (for paths).
/// A trailing `?` marks the reference as optional (e.g. `$param.x?`).
/// Stops at whitespace, quotes, or other non-identifier chars.
fn try_extract_ref<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let after = s.strip_prefix(prefix)?;
    let end = after
        .find(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
        .unwrap_or(after.len());
    if end == 0 {
        return None;
    }
    // Include a trailing `?` as an optional-param marker only if it's terminal
    // (end of string or followed by a non-identifier char like `/`, whitespace, etc.)
    let has_terminal_question_mark = after.as_bytes().get(end) == Some(&b'?')
        && after
            .as_bytes()
            .get(end + 1)
            .is_none_or(|&c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'.');
    let ref_end = if has_terminal_question_mark {
        end + 1
    } else {
        end
    };
    // Trim trailing dots (e.g. "$param.owner." in a sentence)
    let name = after[..ref_end].trim_end_matches('.');
    if name.is_empty() { None } else { Some(name) }
}

/// Strip a trailing `?` from a reference name, returning `(name, is_optional)`.
fn parse_optional(raw: &str) -> (&str, bool) {
    raw.strip_suffix('?')
        .map_or((raw, false), |name| (name, true))
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        Value::Array(_) | Value::Object(_) => v.to_string(),
    }
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
    fn test_missing_param_resolves_to_null_and_dropped() {
        let args: HashMap<String, Value> = [("x".to_string(), json!("$param.missing"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert!(
            !resolved.contains_key("x"),
            "missing param should resolve to null and be dropped"
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

    #[test]
    fn test_interpolation_param_in_middle() {
        let args: HashMap<String, Value> = [(
            "url".to_string(),
            json!("repos/$param.owner/$param.repo/actions"),
        )]
        .into();
        let params: HashMap<String, Value> = [
            ("owner".to_string(), json!("ngngardner")),
            ("repo".to_string(), json!("axi-mcp-proxy")),
        ]
        .into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(
            resolved["url"],
            json!("repos/ngngardner/axi-mcp-proxy/actions")
        );
    }

    #[test]
    fn test_interpolation_step_ref() {
        let args: HashMap<String, Value> =
            [("msg".to_string(), json!("result: $step.s1.count items"))].into();
        let results: HashMap<String, Value> = [("s1".to_string(), json!({"count": 42}))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &results).unwrap();
        assert_eq!(resolved["msg"], json!("result: 42 items"));
    }

    #[test]
    fn test_interpolation_mixed_refs() {
        let args: HashMap<String, Value> = [(
            "cmd".to_string(),
            json!("gh api repos/$param.owner/$param.repo?per_page=$step.s1.limit"),
        )]
        .into();
        let params: HashMap<String, Value> = [
            ("owner".to_string(), json!("alice")),
            ("repo".to_string(), json!("myrepo")),
        ]
        .into();
        let results: HashMap<String, Value> = [("s1".to_string(), json!({"limit": 5}))].into();
        let resolved = resolve_args(&args, &params, &results).unwrap();
        assert_eq!(
            resolved["cmd"],
            json!("gh api repos/alice/myrepo?per_page=5")
        );
    }

    #[test]
    fn test_interpolation_in_array() {
        let args: HashMap<String, Value> = [(
            "argv".to_string(),
            json!(["gh", "api", "repos/$param.owner/runs"]),
        )]
        .into();
        let params: HashMap<String, Value> = [("owner".to_string(), json!("bob"))].into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(resolved["argv"], json!(["gh", "api", "repos/bob/runs"]));
    }

    #[test]
    fn test_interpolation_missing_param_becomes_empty() {
        let args: HashMap<String, Value> =
            [("x".to_string(), json!("prefix/$param.missing/suffix"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved["x"], json!("prefix//suffix"));
    }

    #[test]
    fn test_dollar_without_ref_passthrough() {
        let args: HashMap<String, Value> = [("x".to_string(), json!("costs $5"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved["x"], json!("costs $5"));
    }

    #[test]
    fn test_optional_param_absent() {
        let args: HashMap<String, Value> = [("ft".to_string(), json!("$param.file_type?"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert!(
            !resolved.contains_key("ft"),
            "absent optional param should be dropped"
        );
    }

    #[test]
    fn test_optional_param_present() {
        let args: HashMap<String, Value> = [("ft".to_string(), json!("$param.file_type?"))].into();
        let params: HashMap<String, Value> = [("file_type".to_string(), json!("rs"))].into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(resolved["ft"], json!("rs"));
    }

    #[test]
    fn test_optional_param_interpolation_absent() {
        let args: HashMap<String, Value> =
            [("x".to_string(), json!("prefix/$param.x?/suffix"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(resolved["x"], json!("prefix//suffix"));
    }

    #[test]
    fn test_optional_param_interpolation_present() {
        let args: HashMap<String, Value> =
            [("x".to_string(), json!("prefix/$param.x?/suffix"))].into();
        let params: HashMap<String, Value> = [("x".to_string(), json!("val"))].into();
        let resolved = resolve_args(&args, &params, &HashMap::new()).unwrap();
        assert_eq!(resolved["x"], json!("prefix/val/suffix"));
    }

    #[test]
    fn test_missing_required_param_resolves_gracefully() {
        let args: HashMap<String, Value> = [("x".to_string(), json!("$param.x"))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert!(
            !resolved.contains_key("x"),
            "missing required param should resolve to null and be dropped"
        );
    }

    #[test]
    fn test_null_filtering() {
        let args: HashMap<String, Value> =
            [("x".to_string(), Value::Null), ("y".to_string(), json!(1))].into();
        let resolved = resolve_args(&args, &HashMap::new(), &HashMap::new()).unwrap();
        assert!(!resolved.contains_key("x"), "null values should be dropped");
        assert_eq!(resolved["y"], json!(1));
    }
}
