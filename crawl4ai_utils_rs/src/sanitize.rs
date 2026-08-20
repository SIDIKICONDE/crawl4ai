use pyo3::prelude::*;
use pyo3::types::PyList;
use serde_json::Value as JsonValue;

/// Sanitize HTML: escape quotes (faithful port of `utils.sanitize_html`)
#[pyfunction]
pub fn sanitize_html(html: &str) -> String {
    html.replace('"', "\\\"").replace('\'', "\\'")
}

/// Sanitize input encoding (faithful port of `utils.sanitize_input_encode`)
///
/// Python encodes to UTF-8 ignoring errors then decodes back; since a Rust
/// `&str` is always valid UTF-8, the result is unchanged.
#[pyfunction]
pub fn sanitize_input_encode(text: &str) -> String {
    text.to_string()
}

/// Escape JSON special characters (faithful port of `utils.escape_json_string`)
#[pyfunction]
pub fn escape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) <= 0x1f || (0x7f..=0x9f).contains(&(c as u32)) => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Split a JSON string into objects and parse each one
/// (faithful port of `utils.split_and_parse_json_objects`)
#[pyfunction]
pub fn split_and_parse_json_objects(py: Python, json_string: &str) -> PyResult<(PyObject, Vec<String>)> {
    let mut json_string = json_string;

    // Trim the leading '[' and trailing ']'
    if json_string.starts_with("[") && json_string.ends_with("]") {
        json_string = &json_string[1..json_string.len() - 1];
    }
    let json_string = json_string.trim();

    // Split the string into segments that look like individual JSON objects
    let mut segments: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut start_index = 0;

    for (i, ch) in json_string.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    start_index = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    segments.push(json_string[start_index..=i].to_string());
                }
            }
            _ => {}
        }
    }

    // Try parsing each segment
    let mut parsed_objects = Vec::new();
    let mut unparsed_segments = Vec::new();

    for segment in segments {
        match serde_json::from_str::<JsonValue>(&segment) {
            Ok(value) => parsed_objects.push(json_value_to_pyobject(py, &value)?),
            Err(_) => unparsed_segments.push(segment),
        }
    }

    Ok((PyList::new(py, parsed_objects)?.into(), unparsed_segments))
}

fn json_value_to_pyobject(py: Python, value: &JsonValue) -> PyResult<PyObject> {
    use pyo3::IntoPyObjectExt;
    use pyo3::types::PyDict;
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(b) => (*b).into_py_any(py),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_py_any(py)
            } else if let Some(f) = n.as_f64() {
                f.into_py_any(py)
            } else {
                n.to_string().into_py_any(py)
            }
        }
        JsonValue::String(s) => s.clone().into_py_any(py),
        JsonValue::Array(arr) => {
            let py_list = PyList::new(
                py,
                arr.iter()
                    .map(|v| json_value_to_pyobject(py, v))
                    .collect::<Result<Vec<_>, _>>()?,
            )?;
            Ok(py_list.into())
        }
        JsonValue::Object(obj) => {
            let py_dict = PyDict::new(py);
            for (k, v) in obj {
                let py_val = json_value_to_pyobject(py, v)?;
                py_dict.set_item(k, py_val)?;
            }
            Ok(py_dict.into())
        }
    }
}
