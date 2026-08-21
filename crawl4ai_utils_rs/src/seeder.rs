use pyo3::prelude::*;
use std::collections::HashSet;

/// A tiny urlparse-like extraction used by the seeder scorers.
struct UrlParts {
    netloc: String,
    path: String,
    query: String,
}

fn parse_url(url: &str) -> UrlParts {
    let after_scheme = match url.find("://") {
        Some(p) => &url[p + 3..],
        None => url,
    };

    // netloc ends at the first '/', '?' or '#'.
    let netloc_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let netloc = &after_scheme[..netloc_end];

    // path starts after netloc (keeps the leading '/' when present).
    let rest = &after_scheme[netloc_end..];
    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let path = &rest[..path_end];

    // query is everything after '?' up to '#'.
    let query = match rest.find('?') {
        Some(q) => {
            let after_q = &rest[q + 1..];
            let hash = after_q.find('#').unwrap_or(after_q.len());
            &after_q[..hash]
        }
        None => "",
    };

    UrlParts {
        netloc: netloc.to_string(),
        path: path.to_string(),
        query: query.to_string(),
    }
}

/// Character n-grams (n = 3) of a text, as in `_calculate_url_relevance_score`.
fn get_ngrams(text: &str, n: usize) -> HashSet<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = HashSet::new();
    if chars.len() >= n {
        for i in 0..=chars.len() - n {
            out.insert(chars[i..i + n].iter().collect());
        }
    }
    out
}

/// Relevance score between a query and a URL.
///
/// Faithful port of `AsyncUrlSeeder._calculate_url_relevance_score`.
#[pyfunction]
pub fn url_relevance_score(query: &str, url: &str) -> f64 {
    let query_lower = query.to_lowercase();

    let parsed = parse_url(url);
    let domain = parsed.netloc.replace("www.", "");
    let path = parsed.path.trim_matches('/');

    let domain_parts: Vec<&str> = domain.split('.').collect();
    let path_parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    let mut param_parts: Vec<&str> = Vec::new();
    if !parsed.query.is_empty() {
        for param in parsed.query.split('&') {
            if let Some(eq) = param.find('=') {
                let key = &param[..eq];
                let value = &param[eq + 1..];
                param_parts.push(key);
                param_parts.push(value);
            }
        }
    }

    let all_parts: Vec<String> = domain_parts
        .iter()
        .chain(path_parts.iter())
        .chain(param_parts.iter())
        .map(|s| s.to_string())
        .collect();

    let mut scores: Vec<f64> = Vec::new();
    let query_tokens: Vec<&str> = query_lower.split_whitespace().collect();

    // 1. Exact match in any part (highest score)
    for part in &all_parts {
        let part_lower = part.to_lowercase();
        if part_lower.contains(&query_lower) {
            scores.push(1.0);
        } else if query_lower.contains(&part_lower) {
            scores.push(0.9);
        }
    }

    // 2. Token matching
    for token in &query_tokens {
        let mut token_scores: Vec<f64> = Vec::new();
        for part in &all_parts {
            let part_lower = part.to_lowercase();
            if part_lower.contains(token) {
                let coverage = token.chars().count() as f64 / part_lower.chars().count() as f64;
                token_scores.push(0.7 * coverage);
            } else if token.contains(&part_lower) {
                let coverage = part_lower.chars().count() as f64 / token.chars().count() as f64;
                token_scores.push(0.6 * coverage);
            }
        }
        if let Some(max_ts) = token_scores.iter().cloned().fold(None, |m: Option<f64>, v| {
            Some(m.map_or(v, |mv| mv.max(v)))
        }) {
            scores.push(max_ts);
        }
    }

    // 3. Character n-gram similarity (for fuzzy matching)
    let url_text = all_parts.join(" ").to_lowercase();
    if query_lower.chars().count() >= 3 && url_text.chars().count() >= 3 {
        let query_ngrams = get_ngrams(&query_lower, 3);
        let url_ngrams = get_ngrams(&url_text, 3);
        if !query_ngrams.is_empty() && !url_ngrams.is_empty() {
            let intersection = query_ngrams.intersection(&url_ngrams).count();
            let union = query_ngrams.union(&url_ngrams).count();
            let jaccard = if union > 0 {
                intersection as f64 / union as f64
            } else {
                0.0
            };
            scores.push(0.5 * jaccard);
        }
    }

    if scores.is_empty() {
        return 0.0;
    }

    // Weighted average with bias towards higher scores
    scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    let mut weighted_score = 0.0;
    let mut total_weight = 0.0;
    for (i, score) in scores.iter().enumerate() {
        let weight = 1.0 / (i as f64 + 1.0);
        weighted_score += score * weight;
        total_weight += weight;
    }

    let final_score = if total_weight > 0.0 {
        weighted_score / total_weight
    } else {
        0.0
    };
    final_score.min(1.0)
}

/// Check whether a URL is a utility/nonsense URL that shouldn't be crawled.
///
/// Faithful port of `AsyncUrlSeeder._is_nonsense_url`.
#[pyfunction]
pub fn is_nonsense_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    let parsed = parse_url(url);
    let path = parsed.path.to_lowercase();

    // 1. Robot and sitemap files
    if path.ends_with("/robots.txt")
        || path.ends_with("/sitemap.xml")
        || path.ends_with("/sitemap_index.xml")
    {
        return true;
    }

    // 2. Sitemap variations
    if path.contains("/sitemap") && path.ends_with(".xml") || path.contains("/sitemap") && path.ends_with(".xml.gz") || path.contains("/sitemap") && path.ends_with(".txt") {
        return true;
    }

    // 3. Common utility files
    const UTILITY_FILES: [&str; 12] = [
        "ads.txt",
        "humans.txt",
        "security.txt",
        ".well-known/security.txt",
        "crossdomain.xml",
        "browserconfig.xml",
        "manifest.json",
        "apple-app-site-association",
        ".well-known/apple-app-site-association",
        "favicon.ico",
        "apple-touch-icon.png",
        "android-chrome-192x192.png",
    ];
    for file in UTILITY_FILES {
        if path.ends_with(&format!("/{file}")) {
            return true;
        }
    }

    // 9. Hidden files and directories
    if path.split('/').any(|p| !p.is_empty() && p.starts_with('.')) {
        return true;
    }

    // 10. Common non-content paths
    const NON_CONTENT_PATHS: [&str; 21] = [
        "/wp-admin",
        "/wp-includes",
        "/wp-content/uploads",
        "/admin",
        "/login",
        "/signin",
        "/signup",
        "/register",
        "/checkout",
        "/cart",
        "/account",
        "/profile",
        "/search",
        "/404",
        "/error",
        "/.git",
        "/.svn",
        "/.hg",
        "/cgi-bin",
        "/scripts",
        "/includes",
    ];
    if NON_CONTENT_PATHS.iter().any(|ncp| path.contains(ncp)) {
        return true;
    }

    // 11. URL patterns that indicate non-content
    const PRINT_PATTERNS: [&str; 4] = ["?print=", "&print=", "/print/", "_print."];
    if PRINT_PATTERNS.iter().any(|p| url_lower.contains(p)) {
        return true;
    }

    // 12. Very short paths (likely homepage redirects or errors)
    let trimmed = path.trim_matches('/');
    const ALLOWED_SHORT: [&str; 6] = ["/", "/en", "/de", "/fr", "/es", "/it"];
    if trimmed.chars().count() < 3 && !ALLOWED_SHORT.contains(&path.as_str()) {
        return true;
    }

    false
}