use pyo3::prelude::*;
use std::collections::{HashMap, HashSet};

/// Check a URL against an allowed-extension set.
///
/// Faithful port of `ContentTypeFilter._extract_extension` + `apply`.
#[pyfunction]
#[pyo3(signature = (url, ext_map, check_extension = true))]
pub fn content_type_url(url: &str, ext_map: HashSet<String>, check_extension: bool) -> bool {
    if !check_extension {
        return true;
    }
    let ext = extract_extension(url);
    if ext.is_empty() {
        return true;
    }
    ext_map.contains(&ext)
}

/// Port of `ContentTypeFilter._extract_extension`.
fn extract_extension(url: &str) -> String {
    // Remove scheme (http://, https://) if present
    let url = match url.find("://") {
        Some(p) => &url[p + 3..],
        None => url,
    };

    // Remove domain (everything up to the first '/')
    let path_start = url.find('/');
    let path = match path_start {
        Some(p) => &url[p..],
        None => "",
    };

    // Extract last filename in path
    let filename = match path.rfind('/') {
        Some(p) => &path[p + 1..],
        None => "",
    };

    // Extract and validate extension
    if !filename.contains('.') {
        return String::new();
    }
    match filename.rfind('.') {
        Some(p) => filename[p + 1..].to_lowercase(),
        None => String::new(),
    }
}

/// Check whether a domain is `domain` or a subdomain of `parent_domain`.
///
/// Port of `DomainFilter._is_subdomain`.
fn is_subdomain(domain: &str, parent_domain: &str) -> bool {
    domain == parent_domain || domain.ends_with(&format!(".{parent_domain}"))
}

/// Extract the (lowercased) domain from a URL.
///
/// Port of `DomainFilter._extract_domain` (regex `://([^/]+)`).
fn extract_domain(url: &str) -> String {
    match url.find("://") {
        Some(p) => {
            let rest = &url[p + 3..];
            let end = rest.find('/').unwrap_or(rest.len());
            rest[..end].to_lowercase()
        }
        None => String::new(),
    }
}

/// Check a URL against allowed/blocked domain sets.
///
/// Faithful port of `DomainFilter.apply`.
#[pyfunction]
#[pyo3(signature = (url, allowed_domains, blocked_domains))]
pub fn domain_url_allowed(
    url: &str,
    allowed_domains: Option<HashSet<String>>,
    blocked_domains: HashSet<String>,
) -> bool {
    // Skip processing if no filters
    if blocked_domains.is_empty() && allowed_domains.is_none() {
        return true;
    }

    let domain = extract_domain(url);

    // Check for blocked domains, including subdomains
    for blocked in &blocked_domains {
        if is_subdomain(&domain, blocked) {
            return false;
        }
    }

    // If no allowed domains specified, accept all non-blocked
    let Some(allowed) = allowed_domains else {
        return true;
    };

    // Check if domain matches any allowed domain (including subdomains)
    for allowed_domain in &allowed {
        if is_subdomain(&domain, allowed_domain) {
            return true;
        }
    }

    false
}

/// Simplified BM25 score used by `ContentRelevanceFilter._bm25`.
///
/// Faithful port (IDF `log(2 / (tf + 0.5) + 1)`, length-normalized TF).
#[pyfunction]
#[pyo3(signature = (doc_text, query, k1 = 1.2, b = 0.75, avgdl = 1000))]
pub fn bm25_head_score(doc_text: &str, query: &str, k1: f64, b: f64, avgdl: usize) -> f64 {
    let doc_lower = doc_text.to_lowercase();
    let doc_terms: Vec<&str> = doc_lower.split_whitespace().collect();
    let doc_len = doc_terms.len() as f64;

    let mut tf: HashMap<&str, usize> = HashMap::new();
    for term in &doc_terms {
        *tf.entry(term).or_insert(0) += 1;
    }

    let mut score = 0.0;
    let mut seen: HashSet<&str> = HashSet::new();
    for term in query.to_lowercase().split_whitespace() {
        if !seen.insert(term) {
            continue;
        }
        let term_freq = *tf.get(term).unwrap_or(&0) as f64;
        let idf = (2.0 / (term_freq + 0.5) + 1.0).ln();
        let numerator = term_freq * (k1 + 1.0);
        let denominator = term_freq + k1 * (1.0 - b + b * (doc_len / avgdl as f64));
        score += idf * (numerator / denominator);
    }
    score
}