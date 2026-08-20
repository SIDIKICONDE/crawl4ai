use pyo3::prelude::*;
use std::collections::HashSet;
use std::collections::VecDeque;

/// Chunk documents into token-limited sections with overlap
#[pyfunction]
#[pyo3(signature = (documents, chunk_token_threshold, overlap, word_token_rate = 0.75, tokenizer = None))]
pub fn chunk_documents(
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

/// Merge chunks into target-sized chunks, with optional token overlap.
///
/// Faithful port of `utils.merge_chunks` (including the overlap handling).
#[pyfunction]
#[pyo3(signature = (docs, target_size, overlap = 0, word_token_ratio = 1.0, splitter = None))]
pub fn merge_chunks(
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

    let num_chunks = total_tokens.div_ceil(target_size).max(1);
    let mut chunks: Vec<Vec<String>> = vec![Vec::new(); num_chunks];
    let mut curr_chunk = 0;
    let mut curr_size = 0;

    for tokens in all_tokens.into_iter().flatten() {
        if curr_size >= target_size && curr_chunk < num_chunks - 1 {
            if overlap > 0 {
                let start = chunks[curr_chunk].len().saturating_sub(overlap);
                let overlap_tokens: Vec<String> = chunks[curr_chunk][start..].to_vec();
                let overlap_len = overlap_tokens.len();
                curr_chunk += 1;
                chunks[curr_chunk].extend(overlap_tokens);
                curr_size = overlap_len;
            } else {
                curr_chunk += 1;
                curr_size = 0;
            }
        }
        chunks[curr_chunk].push(tokens);
        curr_size += 1;
    }

    let mut result = Vec::new();
    for chunk in chunks {
        if chunk.is_empty() {
            continue;
        }
        let text = chunk.join(" ");
        result.push(text);
    }

    Ok(result)
}

/// Merge small chunks into larger ones based on token threshold
#[pyfunction]
pub fn merge_chunks_based_on_token_threshold(
    chunks: Vec<String>,
    token_threshold: usize,
) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    let mut current_chunk: Vec<String> = Vec::new();
    let mut total_token_so_far: f64 = 0.0;

    for chunk in chunks {
        let chunk_token_count = chunk.split_whitespace().count() as f64 * 1.3;
        if total_token_so_far + chunk_token_count < token_threshold as f64 {
            current_chunk.push(chunk);
            total_token_so_far += chunk_token_count;
        } else {
            if !current_chunk.is_empty() {
                merged.push(current_chunk.join("\n\n"));
            }
            current_chunk = vec![chunk];
            total_token_so_far = chunk_token_count;
        }
    }

    if !current_chunk.is_empty() {
        merged.push(current_chunk.join("\n\n"));
    }

    merged
}

/// Split text into words on punctuation/whitespace and HTML/code symbols.
///
/// Faithful port of `utils.advanced_split` (SPLITS table + HTML_CODE_CHARS).
#[pyfunction]
pub fn advanced_split(text: &str) -> Vec<String> {
    let html_code_chars: HashSet<&str> = [
        "•", "►", "▼", "©", "®", "™", "→", "⇒", "≈", "≤", "≥", "+=", "-=", "*=", "/=", "=>",
        "<=>", "!=", "==", "===", "++", "--", "<<", ">>", "&&", "||", "??", "?:", "?.", "…",
        "\u{201c}", "\u{201d}", "\u{2018}", "\u{2019}", "«", "»", "—", "–", "+", "=", "~", "@",
        "#", "$", "%", "^", "&", "*", "(", ")", "{", "}", "[", "]", "|", "\\", "/", "`", "<",
        ">", ",", ".", "?", "!", ":", ";", "-", "_",
    ]
    .iter()
    .cloned()
    .collect();

    let chars: Vec<char> = text.chars().collect();
    let mut result: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        let o = c as u32;
        if o < 256 && splits_byte(o as usize) {
            if !word.is_empty() {
                result.push(std::mem::take(&mut word));
            }
        } else if i < chars.len() - 1 {
            let mut two = String::with_capacity(4);
            two.push(c);
            two.push(chars[i + 1]);
            if html_code_chars.contains(two.as_str()) {
                if !word.is_empty() {
                    result.push(std::mem::take(&mut word));
                }
                i += 1; // Skip next char since we used it
            } else {
                word.push(c);
            }
        } else {
            word.push(c);
        }
        i += 1;
    }

    if !word.is_empty() {
        result.push(word);
    }

    result
}

/// Whether the ASCII code is a split character.
///
/// Reproduces the *actual* bytearray `SPLITS` from `utils.py` (which has a
/// one-off shift: the second row has 16 entries for "33-47", pushing the
/// whole table one position right; `a` at 97 is therefore a split char).
fn splits_byte(o: usize) -> bool {
    match o {
        0..=48 => true,
        49..=58 => false,
        59..=65 => true,
        66..=91 => false,
        92..=97 => true,
        98..=123 => false,
        _ => true,
    }
}
