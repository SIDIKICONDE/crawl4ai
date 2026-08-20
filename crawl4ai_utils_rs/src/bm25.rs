use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::collections::HashMap;

/// Stem a list of tokens using the Snowball stemming algorithm.
///
/// Faithful Rust port of `snowballstemmer.stemmer(language).stemWord(word)`.
#[pyfunction]
pub fn stem_tokens(tokens: Vec<String>, language: &str) -> PyResult<Vec<String>> {
    let algorithm = parse_language(language)?;
    let stemmer = rust_stemmers::Stemmer::create(algorithm);

    Ok(tokens
        .into_iter()
        .map(|t| stemmer.stem(&t).to_string())
        .collect())
}

/// Compute BM25Okapi scores for a corpus and query.
///
/// Faithful port of `rank_bm25.BM25Okapi` (version 0.2.2).
/// See `rank_bm25.py` for the original Python implementation.
///
/// Args:
///     corpus: tokenized documents (list of word lists)
///     query: tokenized query (list of words)
///     k1: BM25 k1 parameter (default: 1.5)
///     b: BM25 b parameter (default: 0.75)
///     epsilon: floor for negative idf values (default: 0.25)
///
/// Returns:
///     List of BM25 scores (one per document in corpus)
#[pyfunction]
#[pyo3(signature = (corpus, query, k1 = 1.5, b = 0.75, epsilon = 0.25))]
pub fn bm25_scores(
    corpus: Vec<Vec<String>>,
    query: Vec<String>,
    k1: f64,
    b: f64,
    epsilon: f64,
) -> PyResult<Vec<f64>> {
    let corpus_size = corpus.len();
    if corpus_size == 0 {
        return Ok(Vec::new());
    }

    // doc_len[i] = number of tokens in corpus[i]
    let doc_len: Vec<usize> = corpus.iter().map(|doc| doc.len()).collect();
    let total_words: usize = doc_len.iter().sum();
    let avgdl = total_words as f64 / corpus_size as f64;

    // doc_freqs[i] = HashMap(word -> count in corpus[i])
    // nd = HashMap(word -> number of documents containing word)
    let mut doc_freqs: Vec<HashMap<&str, usize>> = Vec::with_capacity(corpus_size);
    let mut nd: HashMap<&str, usize> = HashMap::new();

    for doc in &corpus {
        let mut freq: HashMap<&str, usize> = HashMap::new();
        for word in doc {
            *freq.entry(word.as_str()).or_insert(0) += 1;
        }
        for word in freq.keys() {
            *nd.entry(word).or_insert(0) += 1;
        }
        doc_freqs.push(freq);
    }

    // _calc_idf: log(N - freq + 0.5) - log(freq + 0.5)
    let mut idf: HashMap<&str, f64> = HashMap::with_capacity(nd.len());
    let mut idf_sum = 0.0f64;
    let mut negative_words: Vec<&str> = Vec::new();

    for (word, freq) in &nd {
        let value = ((corpus_size as f64 - *freq as f64 + 0.5) / (*freq as f64 + 0.5)).ln();
        idf.insert(word, value);
        idf_sum += value;
        if value < 0.0 {
            negative_words.push(word);
        }
    }

    // Apply epsilon floor for negative idf words
    if !idf.is_empty() {
        let average_idf = idf_sum / idf.len() as f64;
        let eps = epsilon * average_idf;
        for word in &negative_words {
            if let Some(v) = idf.get_mut(word) {
                *v = eps;
            }
        }
    }

    // get_scores: Σ(q) idf(q) * q_freq * (k1+1) / (q_freq + k1*(1 - b + b*doc_len/avgdl))
    let mut scores = vec![0.0f64; corpus_size];
    let doc_len_f64: Vec<f64> = doc_len.iter().map(|&l| l as f64).collect();

    for q in &query {
        let q_idf = idf.get(q.as_str()).copied().unwrap_or(0.0f64);
        // If idf is 0.0, the Python `or 0` also yields 0.0
        if q_idf == 0.0f64 {
            continue;
        }

        for (i, doc_freq) in doc_freqs.iter().enumerate() {
            let q_freq = doc_freq.get(q.as_str()).copied().unwrap_or(0) as f64;
            if q_freq == 0.0f64 {
                continue;
            }
            let numerator = q_freq * (k1 + 1.0);
            let denominator = q_freq + k1 * (1.0 - b + b * doc_len_f64[i] / avgdl);
            scores[i] += q_idf * numerator / denominator;
        }
    }

    Ok(scores)
}

/// Parse a language string into a `rust_stemmers::Algorithm`.
fn parse_language(language: &str) -> PyResult<rust_stemmers::Algorithm> {
    use rust_stemmers::Algorithm;
    match language.to_lowercase().as_str() {
        "arabic" => Ok(Algorithm::Arabic),
        "danish" => Ok(Algorithm::Danish),
        "dutch" => Ok(Algorithm::Dutch),
        "english" => Ok(Algorithm::English),
        "finnish" => Ok(Algorithm::Finnish),
        "french" => Ok(Algorithm::French),
        "german" => Ok(Algorithm::German),
        "greek" => Ok(Algorithm::Greek),
        "hungarian" => Ok(Algorithm::Hungarian),
        "italian" => Ok(Algorithm::Italian),
        "norwegian" => Ok(Algorithm::Norwegian),
        "portuguese" => Ok(Algorithm::Portuguese),
        "romanian" => Ok(Algorithm::Romanian),
        "russian" => Ok(Algorithm::Russian),
        "spanish" => Ok(Algorithm::Spanish),
        "swedish" => Ok(Algorithm::Swedish),
        "tamil" => Ok(Algorithm::Tamil),
        "turkish" => Ok(Algorithm::Turkish),
        _ => Err(PyValueError::new_err(format!(
            "Unsupported language for stemming: '{}'",
            language
        ))),
    }
}