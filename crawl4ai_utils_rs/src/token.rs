use pyo3::prelude::*;
use std::collections::HashSet;

/// Clean tokens by removing noise, stop words, and short tokens.
///
/// Faithful port of `utils.clean_tokens`:
/// keeps a token only if it has more than 2 characters, is not in the
/// noise set, is not in STOP_WORDS (all case-sensitive), and does not
/// start with "↑", "▲" or "⬆".
#[pyfunction]
pub fn clean_tokens(tokens: Vec<String>) -> Vec<String> {
    let noise: HashSet<&str> = [
        "ccp", "up", "↑", "▲", "⬆️", "a", "an", "at", "by", "in", "of", "on", "to", "the",
    ]
    .iter()
    .cloned()
    .collect();

    let stop_words: HashSet<&str> = [
        // Articles / basic
        "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "has", "he", "in",
        "is", "it", "its", "of", "on", "that", "the", "to", "was", "were", "will", "with",
        // Pronouns
        "i", "you", "she", "we", "they", "me", "him", "her", "us", "them", "my", "your",
        "his", "our", "their", "mine", "yours", "hers", "ours", "theirs", "myself",
        "yourself", "himself", "herself", "itself", "ourselves", "themselves",
        // Common verbs
        "am", "been", "being", "have", "had", "having", "do", "does", "did", "doing",
        // Prepositions
        "about", "above", "across", "after", "against", "along", "among", "around",
        "before", "behind", "below", "beneath", "beside", "between", "beyond", "down",
        "during", "except", "inside", "into", "near", "off", "out", "outside", "over",
        "past", "through", "toward", "under", "underneath", "until", "upon", "within",
        // Conjunctions
        "but", "or", "nor", "yet", "so", "although", "because", "since", "unless",
        // Other common words
        "this", "these", "those", "what", "which", "who", "whom", "whose", "when",
        "where", "why", "how", "all", "any", "both", "each", "few", "more", "most",
        "other", "some", "such", "can", "cannot", "can't", "could", "couldn't", "may",
        "might", "must", "mustn't", "shall", "should", "shouldn't", "won't", "would",
        "wouldn't", "not", "n't", "no", "none",
    ]
    .iter()
    .cloned()
    .collect();

    tokens
        .into_iter()
        .filter(|t| {
            t.chars().count() > 2
                && !noise.contains(t.as_str())
                && !stop_words.contains(t.as_str())
                && !t.starts_with("↑")
                && !t.starts_with("▲")
                && !t.starts_with("⬆")
        })
        .collect()
}

/// Truncate a string with ellipsis.
///
/// Counts characters (not bytes) like Python's `len()`, and never
/// panics on multi-byte UTF-8 input.
#[pyfunction]
pub fn truncate(value: &str, threshold: usize) -> String {
    if value.chars().count() > threshold {
        let mut s: String = value.chars().take(threshold).collect();
        s.push_str("...");
        s
    } else {
        value.to_string()
    }
}
