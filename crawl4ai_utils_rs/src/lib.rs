#![allow(deprecated)]

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::Value as JsonValue;
use std::collections::VecDeque;

/// Chunk documents into token-limited sections with overlap
#[pyfunction]
#[pyo3(signature = (documents, chunk_token_threshold, overlap, word_token_rate = 0.75, tokenizer = None))]
fn chunk_documents(
    py: Python,
    documents: Vec<String>,
    chunk_token_threshold: usize,
    overlap: usize,
    word_token_rate: f64,
    tokenizer: Option<PyObject>,
) -> PyResult<Vec<String>> {
    let mut token_queue: VecDeque<String> = VecDeque::new();
    let mut contribution_queue: VecDeque<f64> = VecDeque::new();
    let mut current_token_count = 0.0;
    let mut results = Vec::new();

    for doc in documents {
        let (tokens, contributions): (Vec<String>, Vec<f64>) = if let Some(ref tok) = tokenizer {
            let tokens: Vec<String> = tok.call1(py, (doc.clone(),))?.extract(py)?;
            let contribs = vec![1.0; tokens.len()];
            (tokens, contribs)
        } else {
            let tokens: Vec<String> = doc.split_whitespace().map(|s| s.to_string()).collect();
            let contribs = vec![word_token_rate; tokens.len()];
            (tokens, contribs)
        };

        for (token, contrib) in tokens.into_iter().zip(contributions) {
            token_queue.push_back(token);
            contribution_queue.push_back(contrib);
            current_token_count += contrib;
        }

        while current_token_count >= chunk_token_threshold as f64 {
            let mut chunk_tokens: VecDeque<String> = VecDeque::new();
            let mut chunk_contrib: VecDeque<f64> = VecDeque::new();
            let mut chunk_total = 0.0;

            while let Some(&next_contrib) = contribution_queue.front() {
                if chunk_total + next_contrib > chunk_token_threshold as f64 {
                    break;
                }
                chunk_total += next_contrib;
                chunk_contrib.push_back(contribution_queue.pop_front().unwrap());
                chunk_tokens.push_back(token_queue.pop_front().unwrap());
            }

            if chunk_contrib.is_empty() {
                chunk_contrib.push_back(contribution_queue.pop_front().unwrap());
                chunk_tokens.push_back(token_queue.pop_front().unwrap());
            }

            let mut overlap_total = 0.0;
            let mut overlap_idx = 0;
            for &contrib in chunk_contrib.iter().rev() {
                if overlap_total + contrib > overlap as f64 {
                    break;
                }
                overlap_total += contrib;
                overlap_idx += 1;
            }

            if overlap_idx > 0 {
                let overlap_tokens: Vec<String> = chunk_tokens
                    .iter()
                    .rev()
                    .take(overlap_idx)
                    .cloned()
                    .collect();
                let overlap_contrib: Vec<f64> = chunk_contrib
                    .iter()
                    .rev()
                    .take(overlap_idx)
                    .cloned()
                    .collect();

                for token in overlap_tokens.into_iter().rev() {
                    token_queue.push_front(token);
                }
                for contrib in overlap_contrib.into_iter().rev() {
                    contribution_queue.push_front(contrib);
                }
                current_token_count += overlap_total;
            }

            current_token_count -= chunk_contrib.iter().sum::<f64>();

            let chunk_text = if overlap_idx > 0 {
                chunk_tokens
                    .iter()
                    .take(chunk_tokens.len() - overlap_idx)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                chunk_tokens.iter().cloned().collect::<Vec<_>>().join(" ")
            };

            if !chunk_text.is_empty() {
                results.push(chunk_text);
            }
        }
    }

    if !token_queue.is_empty() {
        results.push(token_queue.into_iter().collect::<Vec<_>>().join(" "));
    }

    Ok(results)
}

/// Merge chunks into target-sized chunks with optional overlap
#[pyfunction]
#[pyo3(signature = (docs, target_size, overlap = 0, word_token_ratio = 1.0, splitter = None))]
fn merge_chunks(
    py: Python,
    docs: Vec<String>,
    target_size: usize,
    overlap: usize,
    word_token_ratio: f64,
    splitter: Option<PyObject>,
) -> PyResult<Vec<String>> {
    let splitter_fn = |s: &str| -> Vec<String> {
        if let Some(splitter) = &splitter {
            splitter.call1(py, (s,)).unwrap().extract(py).unwrap()
        } else {
            s.split_whitespace().map(|x| x.to_string()).collect()
        }
    };

    let mut all_tokens: Vec<Vec<String>> = Vec::new();
    let mut token_counts: Vec<usize> = Vec::new();
    let mut total_tokens = 0;

    for doc in docs {
        let tokens = splitter_fn(&doc);
        let count = (tokens.len() as f64 * word_token_ratio) as usize;
        if count > 0 {
            token_counts.push(count);
            all_tokens.push(tokens);
            total_tokens += count;
        }
    }

    if total_tokens == 0 {
        return Ok(Vec::new());
    }

    let num_chunks = ((total_tokens + target_size - 1) / target_size).max(1);
    let mut chunks: Vec<Vec<String>> = vec![Vec::new(); num_chunks];
    let mut curr_chunk = 0;
    let mut curr_size = 0;

    for tokens in all_tokens.into_iter().flatten() {
        if curr_size >= target_size && curr_chunk < num_chunks - 1 {
            if overlap > 0 {
                let overlap_tokens: Vec<String> = chunks[curr_chunk]
                    .iter()
                    .rev()
                    .take(overlap)
                    .cloned()
                    .collect();
                curr_chunk += 1;
                chunks[curr_chunk].extend(overlap_tokens.into_iter().rev());
                curr_size = overlap.min(chunks[curr_chunk].len());
            } else {
                curr_chunk += 1;
                curr_size = 0;
            }
        }
        chunks[curr_chunk].push(tokens);
        curr_size += 1;
    }

    Ok(chunks
        .into_iter()
        .filter(|c| !c.is_empty())
        .map(|c| c.join(" "))
        .collect())
}

/// Fast text splitting using lookup tables
#[pyfunction]
fn advanced_split(text: String) -> PyResult<Vec<String>> {
    const SPLITS: [u8; 416] = [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    ];

    let html_code_chars: std::collections::HashSet<&str> = [
        "•", "►", "▼", "©", "®", "™", "→", "⇒", "≈", "≤", "≥", "+=", "-=", "*=", "/=", "=>", "<=>",
        "!=", "==", "===", "++", "--", "<<", ">>", "&&", "||", "??", "?:", "?.", "…", "\"", "\"",
        "'", "'", "«", "»", "—", "–", "+", "=", "~", "@", "#", "$", "%", "^", "&", "*", "(", ")",
        "{", "}", "[", "]", "|", "\\", "/", "`", "<", ">", ",", ".", "?", "!", ":", ";", "-", "_",
    ]
    .into_iter()
    .collect();

    let mut result = Vec::new();
    let mut word = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let ch = chars[i];
        let o = ch as usize;

        if o < 256 && SPLITS[o] == 1 {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
        } else if i + 1 < chars.len() {
            let two_chars: String = chars[i..=i + 1].iter().collect();
            if html_code_chars.contains(two_chars.as_str()) {
                if !word.is_empty() {
                    result.push(std::mem::take(&mut word));
                }
                i += 1;
            } else {
                word.push(ch);
            }
        } else {
            word.push(ch);
        }
        i += 1;
    }

    if !word.is_empty() {
        result.push(word);
    }

    Ok(result)
}

/// Sanitize HTML by escaping quotes
#[pyfunction]
fn sanitize_html(html: String) -> String {
    html.replace('"', "\\\"").replace('\'', "\\'")
}

/// Sanitize input encoding
#[pyfunction]
fn sanitize_input_encode(text: String) -> PyResult<String> {
    Ok(text
        .chars()
        .filter(|c| {
            c.is_ascii()
                || c.is_alphanumeric()
                || c.is_whitespace()
                || ".,;:!?=[]{}()<>/-_\"'".contains(*c)
        })
        .collect())
}

/// Escape string for JSON
#[pyfunction]
fn escape_json_string(s: String) -> String {
    let mut result = String::with_capacity(s.len() + 16);
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\u{08}' => result.push_str("\\b"),
            '\u{0c}' => result.push_str("\\f"),
            c if c.is_control() => {
                let code = c as u32;
                result.push_str(&format!("\\u{:04x}", code));
            }
            c => result.push(c),
        }
    }
    result
}

/// Split and parse JSON objects from a string
#[pyfunction]
fn split_and_parse_json_objects(
    py: Python,
    json_string: String,
) -> PyResult<(PyObject, Vec<String>)> {
    let mut json_string = json_string.trim().to_string();

    if json_string.starts_with('[') && json_string.ends_with(']') {
        json_string = json_string[1..json_string.len() - 1].trim().to_string();
    }

    let mut segments = Vec::new();
    let mut depth = 0;
    let mut start_index = 0;
    let chars: Vec<char> = json_string.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
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
                    segments.push(chars[start_index..=i].iter().collect::<String>());
                }
            }
            _ => {}
        }
    }

    let mut parsed_objects = Vec::new();
    let mut unparsed_segments = Vec::new();

    for segment in segments {
        match serde_json::from_str::<JsonValue>(&segment) {
            Ok(obj) => {
                let py_obj = json_value_to_pyobject(py, &obj)?;
                parsed_objects.push(py_obj);
            }
            Err(_) => unparsed_segments.push(segment),
        }
    }

    let py_list = PyList::new(py, parsed_objects)?;
    Ok((py_list.into(), unparsed_segments))
}

fn json_value_to_pyobject(py: Python, value: &JsonValue) -> PyResult<PyObject> {
    use pyo3::ToPyObject;
    match value {
        JsonValue::Null => Ok(py.None()),
        JsonValue::Bool(b) => Ok(b.to_object(py)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_object(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_object(py))
            } else {
                Ok(n.to_string().to_object(py))
            }
        }
        JsonValue::String(s) => Ok(s.to_object(py)),
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

/// Calculate optimal semaphore count based on system resources
#[pyfunction]
fn calculate_semaphore_count() -> PyResult<usize> {
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let memory_gb = get_system_memory_bytes()? as f64 / (1024.0 * 1024.0 * 1024.0);
    let base_count = (cpu_count / 2).max(1);
    let memory_based_cap = (memory_gb / 2.0) as usize;
    Ok(base_count.min(memory_based_cap).max(1))
}

/// Get system memory in bytes
#[pyfunction]
fn get_system_memory() -> PyResult<u64> {
    get_system_memory_bytes()
}

fn get_system_memory_bytes() -> PyResult<u64> {
    #[cfg(target_os = "linux")]
    {
        use std::fs::File;
        use std::io::{BufRead, BufReader};
        let file = File::open("/proc/meminfo")
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line =
                line.map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb = parts[1].parse::<u64>().map_err(|e| {
                        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
                    })?;
                    return Ok(kb * 1024);
                }
            }
        }
        Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
            "MemTotal not found in /proc/meminfo",
        ))
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
        let output_str = String::from_utf8_lossy(&output.stdout);
        output_str
            .trim()
            .parse::<u64>()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    #[cfg(target_os = "windows")]
    {
        use std::ffi::c_void;
        use std::mem;

        #[link(name = "kernel32")]
        extern "system" {
            fn GlobalMemoryStatusEx(lpBuffer: *mut MemoryStatusEx) -> i32;
        }

        #[repr(C)]
        struct MemoryStatusEx {
            dwLength: u32,
            dwMemoryLoad: u32,
            ullTotalPhys: u64,
            ullAvailPhys: u64,
            ullTotalPageFile: u64,
            ullAvailPageFile: u64,
            ullTotalVirtual: u64,
            ullAvailVirtual: u64,
            ullAvailExtendedVirtual: u64,
        }

        let mut status = MemoryStatusEx {
            dwLength: mem::size_of::<MemoryStatusEx>() as u32,
            dwMemoryLoad: 0,
            ullTotalPhys: 0,
            ullAvailPhys: 0,
            ullTotalPageFile: 0,
            ullAvailPageFile: 0,
            ullTotalVirtual: 0,
            ullAvailVirtual: 0,
            ullAvailExtendedVirtual: 0,
        };

        unsafe {
            if GlobalMemoryStatusEx(&mut status) != 0 {
                Ok(status.ullTotalPhys)
            } else {
                Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
                    "GlobalMemoryStatusEx failed",
                ))
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(PyErr::new::<pyo3::exceptions::PyOSError, _>(
            "Unsupported operating system",
        ))
    }
}

/// Get home folder for Crawl4AI
#[pyfunction]
fn get_home_folder() -> PyResult<String> {
    let home = dirs::home_dir().ok_or_else(|| {
        PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Could not determine home directory")
    })?;
    let crawl4ai_dir = home.join(".crawl4ai");
    std::fs::create_dir_all(&crawl4ai_dir)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    std::fs::create_dir_all(crawl4ai_dir.join("cache"))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    std::fs::create_dir_all(crawl4ai_dir.join("models"))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    Ok(crawl4ai_dir.to_string_lossy().to_string())
}

/// Python module definition
#[pymodule]
fn crawl4ai_utils(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(chunk_documents, m)?)?;
    m.add_function(wrap_pyfunction!(merge_chunks, m)?)?;
    m.add_function(wrap_pyfunction!(advanced_split, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_html, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize_input_encode, m)?)?;
    m.add_function(wrap_pyfunction!(escape_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(split_and_parse_json_objects, m)?)?;
    m.add_function(wrap_pyfunction!(calculate_semaphore_count, m)?)?;
    m.add_function(wrap_pyfunction!(get_system_memory, m)?)?;
    m.add_function(wrap_pyfunction!(get_home_folder, m)?)?;
    Ok(())
}
