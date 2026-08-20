use pyo3::prelude::*;
use std::collections::HashSet;

/// Clean tokens by removing noise, stop words, and short tokens
#[pyfunction]
pub fn clean_tokens(tokens: Vec<String>) -> Vec<String> {
    let noise: HashSet<&str> = [
        "ccp", "up", "↑", "▲", "⬆️", "a", "an", "at", "by", "in", "of", "on", "to", "the",
    ]
    .iter()
    .cloned()
    .collect();

    let stop_words: HashSet<&str> = [
        "a", "an", "and", "are", "as", "at", "be", "but", "by", "for",
        "if", "in", "into", "is", "it", "no", "not", "of", "on", "or",
        "such", "that", "the", "their", "then", "there", "these", "they",
        "this", "to", "was", "will", "with",
    ]
    .iter()
    .cloned()
    .collect();

    let skip_prefixes = ["↑", "▲", "⬆"];

    tokens
        .into_iter()
        .filter(|t| {
            let lower = t.to_lowercase();
            if t.len() < 2 && !noise.contains(t.as_str()) {
                return false;
            }
            !noise.contains(t.as_str())
                && !stop_words.contains(lower.as_str())
                && !skip_prefixes.iter().any(|p| t.starts_with(p))
        })
        .collect()
}

/// Truncate a string with ellipsis
#[pyfunction]
pub fn truncate(value: &str, threshold: usize) -> String {
    if value.len() > threshold {
        let mut s = value[..threshold].to_string();
        s.push_str("...");
        s
    } else {
        value.to_string()
    }
}
