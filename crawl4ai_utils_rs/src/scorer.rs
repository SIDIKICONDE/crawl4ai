use pyo3::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Score lookup tables from `deep_crawling/scorers.py`.
const SCORE_LOOKUP: [f64; 4] = [1.0, 0.5, 0.3333333333333333, 0.25];
const FRESHNESS_SCORES: [f64; 6] = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5];

/// Count how many of the keywords are contained in the URL.
///
/// Faithful port of `KeywordRelevanceScorer._calculate_score`.
#[pyfunction]
#[pyo3(signature = (url, keywords, case_sensitive = false))]
pub fn keyword_relevance_score(url: &str, keywords: Vec<String>, case_sensitive: bool) -> f64 {
    let url = if case_sensitive {
        url.to_string()
    } else {
        url.to_lowercase()
    };
    let keywords: Vec<String> = keywords
        .into_iter()
        .map(|k| if case_sensitive { k } else { k.to_lowercase() })
        .collect();

    let matches = keywords.iter().filter(|k| url.contains(k.as_str())).count();
    if matches == 0 {
        return 0.0;
    }
    if matches == keywords.len() {
        return 1.0;
    }
    matches as f64 / keywords.len() as f64
}

/// Compute the path depth score for a URL.
///
/// Faithful port of `PathDepthScorer._calculate_score` (including `_quick_depth`).
#[pyfunction]
#[pyo3(signature = (url, optimal_depth = 3))]
pub fn path_depth_score(url: &str, optimal_depth: usize) -> f64 {
    // Find the first '/' after the scheme ("://").
    let search_from = url.find("://").map_or(2, |p| p + 3);
    let pos = url
        .get(search_from..)
        .and_then(|rest| rest.find('/').map(|p| search_from + p));

    let depth = match pos {
        Some(p) => quick_depth(&url[p..]),
        None => 0,
    };

    let distance = (depth as isize - optimal_depth as isize).unsigned_abs();
    if distance < SCORE_LOOKUP.len() {
        SCORE_LOOKUP[distance]
    } else {
        1.0 / (1.0 + distance as f64)
    }
}

/// Port of `PathDepthScorer._quick_depth`: count path segments.
fn quick_depth(path: &str) -> usize {
    if path.is_empty() || path == "/" {
        return 0;
    }
    if !path.contains('/') {
        return 0;
    }
    let mut depth = 0usize;
    let mut last_was_slash = true;
    for c in path.chars() {
        if c == '/' {
            if !last_was_slash {
                depth += 1;
            }
            last_was_slash = true;
        } else {
            last_was_slash = false;
        }
    }
    if !last_was_slash {
        depth += 1;
    }
    depth
}

/// Extract the file extension from a URL.
///
/// Faithful port of `ContentTypeScorer._quick_extension`.
pub fn quick_extension(url: &str) -> String {
    let Some(pos) = url.rfind('.') else {
        return String::new();
    };
    let mut end = url.len();
    for (i, c) in url[pos + 1..].char_indices() {
        if c == '?' || c == '#' || c == ';' || !c.is_alphanumeric() {
            end = pos + 1 + i;
            break;
        }
    }
    url[pos + 1..end].to_lowercase()
}

/// Compute the content-type score for a URL.
///
/// Faithful port of `ContentTypeScorer._calculate_score`.
///
/// `regex_types` must be a list of `(pattern, score)` tuples, already sorted
/// by descending score (as done in the Python constructor).
#[pyfunction]
#[pyo3(signature = (url, exact_types, regex_types))]
pub fn content_type_score(
    url: &str,
    exact_types: HashMap<String, f64>,
    regex_types: Vec<(String, f64)>,
) -> f64 {
    let ext = quick_extension(url);
    if !ext.is_empty() {
        if let Some(&score) = exact_types.get(&ext) {
            return score;
        }
    }
    for (pattern, score) in regex_types {
        if let Ok(re) = Regex::new(&pattern) {
            if re.is_match(url) {
                return score;
            }
        }
    }
    0.0
}

/// Date regex from `FreshnessScorer` (combined pattern for all date formats).
fn date_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(
            r"(?:/|[-_])((?:19|20)\d{2})(?:(?:/|[-_])(?:\d{2})(?:(?:/|[-_])(?:\d{2}))?)?",
        )
        .expect("valid freshness date regex")
    })
}

/// Compute the freshness score for a URL.
///
/// Faithful port of `FreshnessScorer._calculate_score` (including `_extract_year`).
#[pyfunction]
#[pyo3(signature = (url, current_year = 2024))]
pub fn freshness_score(url: &str, current_year: usize) -> f64 {
    let mut latest_year: Option<usize> = None;
    for caps in date_pattern().captures_iter(url) {
        if let Some(g) = caps.get(1) {
            if let Ok(year) = g.as_str().parse::<usize>() {
                if year <= current_year
                    && latest_year.is_none_or(|ly| year > ly)
                {
                    latest_year = Some(year);
                }
            }
        }
    }

    match latest_year {
        None => 0.5,
        Some(year) => {
            let year_diff = current_year - year;
            if year_diff < FRESHNESS_SCORES.len() {
                FRESHNESS_SCORES[year_diff]
            } else {
                (1.0 - year_diff as f64 * 0.1).max(0.1)
            }
        }
    }
}

/// Extract the (lowercased, port-stripped) domain from a URL.
///
/// Faithful port of `DomainAuthorityScorer._extract_domain`.
fn extract_domain(url: &str) -> String {
    let start = url.find("://").map_or(0, |p| p + 3);
    let rest = &url[start..];

    // Find the first of '/', '?', '#' after the domain start.
    let end = rest
        .find('/')
        .or_else(|| rest.find('?'))
        .or_else(|| rest.find('#'))
        .unwrap_or(rest.len());
    let domain = &rest[..end];

    let port_idx = domain.rfind(':');
    let domain = match port_idx {
        Some(p) => &domain[..p],
        None => domain,
    };
    domain.to_lowercase()
}

/// Compute the domain authority score for a URL.
///
/// Faithful port of `DomainAuthorityScorer._calculate_score`.
/// `domain_weights` must already map lowercased domains to scores
/// (as done in the Python constructor).
#[pyfunction]
#[pyo3(signature = (url, domain_weights, default_weight = 0.5))]
pub fn domain_authority_score(
    url: &str,
    domain_weights: HashMap<String, f64>,
    default_weight: f64,
) -> f64 {
    let domain = extract_domain(url);
    domain_weights.get(&domain).copied().unwrap_or(default_weight)
}