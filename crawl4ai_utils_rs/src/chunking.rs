use pyo3::prelude::*;
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

/// Merge chunks into target-sized chunks
#[pyfunction]
#[pyo3(signature = (docs, target_size, _overlap = 0, word_token_ratio = 1.0, splitter = None))]
pub fn merge_chunks(
    py: Python,
    docs: Vec<String>,
    target_size: usize,
    _overlap: usize,
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
            curr_chunk += 1;
            curr_size = 0;
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

/// Split text into n-sized pieces
#[pyfunction]
pub fn advanced_split(text: &str) -> Vec<String> {
    let stop_words = [
        "a", "an", "and", "are", "as", "at", "be", "but", "by", "for",
        "if", "in", "into", "is", "it", "no", "not", "of", "on", "or",
        "such", "that", "the", "their", "then", "there", "these", "they",
        "this", "to", "was", "will", "with",
    ];

    text.split_whitespace()
        .filter(|t| {
            let lower = t.to_lowercase();
            !stop_words.contains(&lower.as_str())
        })
        .map(|t| t.to_string())
        .collect()
}
