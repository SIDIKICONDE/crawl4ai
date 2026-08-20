use pyo3::prelude::*;

/// Sanitize HTML: strip dangerous tags while preserving content
#[pyfunction]
pub fn sanitize_html(html: &str) -> String {
    // Strip script, style, iframe, object, embed tags
    let re = regex::RegexBuilder::new(
        r"</?(?:script|style|iframe|object|embed|noscript|link|meta)[^>]*>"
    )
    .case_insensitive(true)
    .build()
    .unwrap();
    re.replace_all(html, "").to_string()
}

/// Sanitize input encoding
#[pyfunction]
pub fn sanitize_input_encode(text: &str) -> String {
    // Remove control characters except newline, tab, carriage return
    text.chars()
        .filter(|&c| {
            c == '\n'
                || c == '\t'
                || c == '\r'
                || c.is_ascii() && (c as u32 >= 32 || c as u32 == 10 || c as u32 == 13 || c as u32 == 9)
        })
        .collect()
}

/// Escape JSON special characters
#[pyfunction]
pub fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\u0000"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Split JSON string and parse objects
#[pyfunction]
pub fn split_and_parse_json_objects(json_string: &str) -> (Vec<String>, Vec<String>) {
    use serde_json::Value as JsonValue;

    let mut parsed = Vec::new();
    let mut unparsed = Vec::new();

    // Try to parse as a JSON array first
    if let Ok(JsonValue::Array(arr)) = serde_json::from_str::<JsonValue>(json_string) {
        for item in arr {
            parsed.push(serde_json::to_string(&item).unwrap_or_default());
        }
        return (parsed, unparsed);
    }

    // Try to find individual JSON objects { ... } using brace counting
    let mut depth = 0i32;
    let mut start = None;
    for (i, c) in json_string.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start {
                        let candidate = &json_string[s..=i];
                        if serde_json::from_str::<JsonValue>(candidate).is_ok() {
                            parsed.push(candidate.to_string());
                        } else {
                            unparsed.push(candidate.to_string());
                        }
                        start = None;
                    }
                }
            }
            _ => {}
        }
    }

    (parsed, unparsed)
}
