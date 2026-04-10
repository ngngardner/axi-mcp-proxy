use regex::Regex;
use serde_json::Value;
use std::fmt::Write;
use std::sync::LazyLock;

// Static regex compilation — pattern is a constant literal, expect cannot fail
#[allow(clippy::expect_used)]
static NUMERIC_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?$").expect("valid regex"));
// Static regex compilation — pattern is a constant literal, expect cannot fail
#[allow(clippy::expect_used)]
static LEADING_ZERO_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0\d+$").expect("valid regex"));
// Static regex compilation — pattern is a constant literal, expect cannot fail
#[allow(clippy::expect_used)]
static KEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_.]*$").expect("valid regex"));

/// Encode a `serde_json::Value` to a TOON v3.0 string.
#[must_use]
pub fn encode(v: &Value) -> String {
    let mut buf = String::new();
    match v {
        Value::Object(m) => encode_object(&mut buf, m, 0),
        Value::Array(arr) => encode_root_array(&mut buf, arr),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            buf.push_str(&format_scalar(v));
        }
    }
    buf
}

fn encode_object(buf: &mut String, m: &serde_json::Map<String, Value>, depth: usize) {
    let keys = ordered_keys(m);
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            buf.push('\n');
        }
        let indent = "  ".repeat(depth);
        encode_field(buf, &indent, &format_key(k), &m[k.as_str()], depth);
    }
}

fn encode_field(buf: &mut String, indent: &str, key: &str, v: &Value, depth: usize) {
    match v {
        Value::Object(m) => {
            // write! to String is infallible
            let _ = write!(buf, "{indent}{key}:");
            if m.is_empty() {
                return;
            }
            buf.push('\n');
            encode_object(buf, m, depth + 1);
        }
        Value::Array(arr) => {
            encode_array(buf, indent, key, arr, depth);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            let _ = write!(buf, "{indent}{key}: {}", format_scalar(v));
        }
    }
}

fn encode_root_array(buf: &mut String, arr: &[Value]) {
    if arr.is_empty() {
        buf.push_str("[0]:");
        return;
    }
    if all_primitive(arr) {
        let _ = write!(buf, "[{}]: ", arr.len());
        for (i, v) in arr.iter().enumerate() {
            if i > 0 {
                buf.push(',');
            }
            buf.push_str(&format_scalar(v));
        }
        return;
    }
    if let Some(fields) = is_tabular(arr) {
        let _ = write!(buf, "[{}]{{{}}}:", arr.len(), fields.join(","));
        for item in arr {
            buf.push('\n');
            // is_tabular already verified all items are objects
            #[allow(clippy::expect_used)]
            let m = item.as_object().expect("tabular items are objects");
            buf.push_str("  ");
            for (j, f) in fields.iter().enumerate() {
                if j > 0 {
                    buf.push(',');
                }
                buf.push_str(&format_scalar(&m[f.as_str()]));
            }
        }
        return;
    }
    let _ = write!(buf, "[{}]:", arr.len());
    for item in arr {
        buf.push('\n');
        encode_list_item(buf, item, 1);
    }
}

fn encode_array(buf: &mut String, indent: &str, key: &str, arr: &[Value], depth: usize) {
    if arr.is_empty() {
        let _ = write!(buf, "{indent}{key}[0]:");
        return;
    }
    if all_primitive(arr) {
        let _ = write!(buf, "{indent}{key}[{}]: ", arr.len());
        for (i, v) in arr.iter().enumerate() {
            if i > 0 {
                buf.push(',');
            }
            buf.push_str(&format_scalar(v));
        }
        return;
    }
    if let Some(fields) = is_tabular(arr) {
        let field_keys: Vec<String> = fields.iter().map(|f| format_key(f)).collect();
        let _ = write!(
            buf,
            "{indent}{key}[{}]{{{}}}:",
            arr.len(),
            field_keys.join(",")
        );
        let child_indent = "  ".repeat(depth + 1);
        for item in arr {
            buf.push('\n');
            // is_tabular already verified all items are objects
            #[allow(clippy::expect_used)]
            let m = item.as_object().expect("tabular items are objects");
            buf.push_str(&child_indent);
            for (j, f) in fields.iter().enumerate() {
                if j > 0 {
                    buf.push(',');
                }
                buf.push_str(&format_scalar(&m[f.as_str()]));
            }
        }
        return;
    }
    let _ = write!(buf, "{indent}{key}[{}]:", arr.len());
    for item in arr {
        buf.push('\n');
        encode_list_item(buf, item, depth + 1);
    }
}

fn encode_list_item(buf: &mut String, v: &Value, depth: usize) {
    let indent = "  ".repeat(depth);
    match v {
        Value::Object(m) => {
            if m.is_empty() {
                let _ = write!(buf, "{indent}-");
                return;
            }
            let keys = ordered_keys(m);
            let _ = write!(buf, "{indent}- ");
            let first_key = &keys[0];
            let first_val = &m[first_key.as_str()];
            match first_val {
                Value::Object(fv) => {
                    let _ = write!(buf, "{}:", format_key(first_key));
                    if !fv.is_empty() {
                        buf.push('\n');
                        encode_object(buf, fv, depth + 2);
                    }
                }
                Value::Array(arr) => {
                    encode_array(buf, "", &format_key(first_key), arr, depth + 1);
                }
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                    let _ = write!(
                        buf,
                        "{}: {}",
                        format_key(first_key),
                        format_scalar(first_val)
                    );
                }
            }
            for k in &keys[1..] {
                buf.push('\n');
                let child_indent = "  ".repeat(depth + 1);
                encode_field(
                    buf,
                    &child_indent,
                    &format_key(k),
                    &m[k.as_str()],
                    depth + 1,
                );
            }
        }
        Value::Array(arr) => {
            let _ = write!(buf, "{indent}- ");
            encode_array(buf, "", "", arr, depth);
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
            let _ = write!(buf, "{indent}- {}", format_scalar(v));
        }
    }
}

fn all_primitive(arr: &[Value]) -> bool {
    arr.iter().all(|v| !v.is_object() && !v.is_array())
}

fn is_tabular(arr: &[Value]) -> Option<Vec<String>> {
    if arr.is_empty() {
        return None;
    }
    let first = arr[0].as_object()?;
    if first.is_empty() {
        return None;
    }
    let fields = ordered_keys(first);
    for v in arr {
        let m = v.as_object()?;
        if m.len() != fields.len() {
            return None;
        }
        for f in &fields {
            let val = m.get(f.as_str())?;
            if val.is_object() || val.is_array() {
                return None;
            }
        }
    }
    Some(fields)
}

fn needs_quoting(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s != s.trim() {
        return true;
    }
    if s == "true" || s == "false" || s == "null" {
        return true;
    }
    if NUMERIC_PATTERN.is_match(s) || LEADING_ZERO_PATTERN.is_match(s) {
        return true;
    }
    if s == "-" || s.starts_with('-') {
        return true;
    }
    for c in s.chars() {
        if matches!(c, ':' | '"' | '\\' | '[' | '{' | ',' | '\n' | '\r' | '\t') {
            return true;
        }
    }
    false
}

fn format_key(k: &str) -> String {
    if KEY_PATTERN.is_match(k) {
        k.to_owned()
    } else {
        quote_string(k)
    }
}

fn quote_string(s: &str) -> String {
    let mut buf = String::with_capacity(s.len() + 2);
    buf.push('"');
    for c in s.chars() {
        match c {
            '\\' => buf.push_str(r"\\"),
            '"' => buf.push_str(r#"\""#),
            '\n' => buf.push_str(r"\n"),
            '\r' => buf.push_str(r"\r"),
            '\t' => buf.push_str(r"\t"),
            _ => buf.push(c),
        }
    }
    buf.push('"');
    buf
}

fn format_scalar(v: &Value) -> String {
    match v {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f.is_nan() || f.is_infinite() {
                    return "null".to_owned();
                }
                // Zero-check on a finite f64 — no precision issue
                #[allow(clippy::float_arithmetic, clippy::float_cmp)]
                if f == 0.0 {
                    return "0".to_owned();
                }
                format!("{f}")
            } else {
                n.to_string()
            }
        }
        Value::String(s) => {
            if needs_quoting(s) {
                quote_string(s)
            } else {
                s.clone()
            }
        }
        Value::Array(_) | Value::Object(_) => format!("{v}"),
    }
}

fn ordered_keys(m: &serde_json::Map<String, Value>) -> Vec<String> {
    let mut keys: Vec<String> = m.keys().cloned().collect();
    keys.sort();
    keys
}

#[cfg(test)]
// Tests use unwrap for brevity — panics are the desired failure mode
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_encode(json_input: &str, want: &str) {
        let v: Value = serde_json::from_str(json_input).unwrap();
        let got = encode(&v);
        assert_eq!(got, want, "encode({json_input})");
    }

    #[test]
    fn test_simple_object() {
        test_encode(r#"{"id":123,"name":"Ada"}"#, "id: 123\nname: Ada");
    }

    #[test]
    fn test_nested_object() {
        test_encode(
            r#"{"user":{"id":123,"name":"Ada"}}"#,
            "user:\n  id: 123\n  name: Ada",
        );
    }

    #[test]
    fn test_primitive_array() {
        test_encode(
            r#"{"tags":["admin","ops","dev"]}"#,
            "tags[3]: admin,ops,dev",
        );
    }

    #[test]
    fn test_tabular_array() {
        test_encode(
            r#"{"items":[{"id":1,"name":"Alice"},{"id":2,"name":"Bob"}]}"#,
            "items[2]{id,name}:\n  1,Alice\n  2,Bob",
        );
    }

    #[test]
    fn test_expanded_list() {
        let v: Value =
            serde_json::from_str(r#"{"items":[{"id":1,"tags":["a","b"]},{"id":2,"tags":["c"]}]}"#)
                .unwrap();
        let result = encode(&v);
        assert!(result.contains("items[2]:"), "got:\n{result}");
    }

    #[test]
    fn test_empty_array() {
        test_encode(r#"{"items":[]}"#, "items[0]:");
    }

    #[test]
    fn test_string_quoting() {
        assert_eq!(format_scalar(&json!("")), r#""""#);
        assert_eq!(format_scalar(&json!("true")), r#""true""#);
        assert_eq!(format_scalar(&json!("false")), r#""false""#);
        assert_eq!(format_scalar(&json!("null")), r#""null""#);
        assert_eq!(format_scalar(&json!("123")), r#""123""#);
        assert_eq!(format_scalar(&json!("hello world")), "hello world");
        assert_eq!(format_scalar(&json!("has:colon")), r#""has:colon""#);
        assert_eq!(format_scalar(&json!("has\"quote")), r#""has\"quote""#);
    }

    #[test]
    fn test_numbers() {
        assert_eq!(format_scalar(&json!(0)), "0");
        assert_eq!(format_scalar(&json!(1.5)), "1.5");
        assert_eq!(format_scalar(&json!(100)), "100");
        assert_eq!(format_scalar(&json!(0.1)), "0.1");
    }

    #[test]
    fn test_root_array() {
        test_encode("[1,2,3]", "[3]: 1,2,3");
    }

    #[test]
    fn test_root_tabular() {
        test_encode(
            r#"[{"a":1,"b":2},{"a":3,"b":4}]"#,
            "[2]{a,b}:\n  1,2\n  3,4",
        );
    }

    #[test]
    fn test_empty_object() {
        test_encode("{}", "");
    }

    #[test]
    fn test_root_primitive() {
        assert_eq!(encode(&json!("hello")), "hello");
        assert_eq!(encode(&json!(42)), "42");
        assert_eq!(encode(&json!(true)), "true");
        assert_eq!(encode(&Value::Null), "null");
    }

    #[test]
    fn test_root_expanded_array() {
        let v: Value = serde_json::from_str(r#"[{"a":1,"b":2},{"a":3,"c":4}]"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("[2]:"), "got:\n{got}");
        assert!(got.contains("- a: 1"), "got:\n{got}");
    }

    #[test]
    fn test_root_empty_array() {
        test_encode("[]", "[0]:");
    }

    #[test]
    fn test_list_item_empty_object() {
        let v: Value = serde_json::from_str(r#"{"items":[{},{}]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("items[2]:"), "got:\n{got}");
        assert!(got.contains("  -"), "got:\n{got}");
    }

    #[test]
    fn test_list_item_nested_object_first_field() {
        let v: Value = serde_json::from_str(r#"{"items":[{"meta":{"x":1},"name":"a"}]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("items[1]:"), "got:\n{got}");
        assert!(got.contains("- meta:"), "got:\n{got}");
        assert!(got.contains("x: 1"), "got:\n{got}");
    }

    #[test]
    fn test_list_item_array_first_field() {
        let v: Value = serde_json::from_str(r#"{"items":[{"arr":[1,2],"name":"a"}]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("- arr[2]: 1,2"), "got:\n{got}");
    }

    #[test]
    fn test_list_item_is_array() {
        let v: Value = serde_json::from_str(r#"{"pairs":[[1,2],[3,4]]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("pairs[2]:"), "got:\n{got}");
        assert!(got.contains("[2]: 1,2"), "got:\n{got}");
    }

    #[test]
    fn test_list_item_empty_nested_object() {
        let v: Value = serde_json::from_str(r#"{"items":[{"meta":{},"name":"a"}]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("- meta:"), "got:\n{got}");
    }

    #[test]
    fn test_list_item_primitive() {
        let v: Value = serde_json::from_str(r#"{"items":[{"a":1},42,"hello"]}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("- a: 1"), "got:\n{got}");
        assert!(got.contains("- 42"), "got:\n{got}");
        assert!(got.contains("- hello"), "got:\n{got}");
    }

    #[test]
    fn test_needs_quoting_edge_cases() {
        assert!(needs_quoting(" leading"));
        assert!(needs_quoting("trailing "));
        assert!(needs_quoting("-dash"));
        assert!(needs_quoting("-"));
        assert!(needs_quoting("has[bracket"));
        assert!(needs_quoting("has{brace"));
        assert!(needs_quoting("has,comma"));
        assert!(needs_quoting("has\nnewline"));
        assert!(needs_quoting("has\rreturn"));
        assert!(needs_quoting("has\ttab"));
        assert!(needs_quoting("0123"));
        assert!(needs_quoting("-3.14"));
        assert!(needs_quoting("1e10"));
        assert!(!needs_quoting("plain text"));
        assert!(!needs_quoting("café"));
    }

    #[test]
    fn test_format_key_quoted() {
        assert_eq!(format_key("valid_key"), "valid_key");
        assert_eq!(format_key("dotted.key"), "dotted.key");
        assert_eq!(format_key("has space"), r#""has space""#);
        assert_eq!(format_key("has:colon"), r#""has:colon""#);
        assert_eq!(format_key("123start"), r#""123start""#);
        assert_eq!(format_key(""), r#""""#);
    }

    #[test]
    fn test_quote_string_escapes() {
        assert_eq!(quote_string(r"back\slash"), r#""back\\slash""#);
        assert_eq!(quote_string("new\nline"), r#""new\nline""#);
        assert_eq!(quote_string("car\rreturn"), r#""car\rreturn""#);
        assert_eq!(quote_string("tab\there"), r#""tab\there""#);
        assert_eq!(quote_string(r#"say "hi""#), r#""say \"hi\"""#);
    }

    #[test]
    fn test_format_scalar_edge_cases() {
        assert_eq!(format_scalar(&Value::Null), "null");
        assert_eq!(format_scalar(&json!(true)), "true");
        assert_eq!(format_scalar(&json!(false)), "false");
    }

    #[test]
    fn test_nested_array_in_object() {
        test_encode(
            r#"{"name":"Ada","tags":["a","b","c"]}"#,
            "name: Ada\ntags[3]: a,b,c",
        );
    }

    #[test]
    fn test_field_empty_nested_object() {
        let v: Value = serde_json::from_str(r#"{"meta":{},"name":"Ada"}"#).unwrap();
        let got = encode(&v);
        assert!(got.contains("meta:"), "got:\n{got}");
        assert!(got.contains("name: Ada"), "got:\n{got}");
    }
}
