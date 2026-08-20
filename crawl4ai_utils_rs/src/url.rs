use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

const TRACKING_PARAMS: &[&str] = &[
    "utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content",
    "gclid", "fbclid", "ref", "ref_src",
];

const SPECIAL_DOMAIN_PARTS: &[&str] = &[
    "co", "com", "org", "gov", "edu", "net", "mil", "int",
    "ac", "ad", "ae", "af", "ag",
];

/// Extract the base domain from a URL
#[pyfunction]
pub fn get_base_domain(url: &str) -> String {
    let domain = match url::Url::parse(url) {
        Ok(u) => u.host_str().unwrap_or("").to_lowercase(),
        Err(_) => return String::new(),
    };
    if domain.is_empty() {
        return String::new();
    }
    let domain = domain.split(':').next().unwrap_or(&domain).to_string();
    let domain = domain.strip_prefix("www.").unwrap_or(&domain).to_string();
    let parts: Vec<&str> = domain.split('.').collect();
    if parts.len() > 2 && SPECIAL_DOMAIN_PARTS.contains(&parts[parts.len() - 2]) {
        return parts[parts.len() - 3..].join(".");
    }
    if parts.len() >= 2 {
        return parts[parts.len() - 2..].join(".");
    }
    parts.join(".")
}

/// Check if a URL is external relative to a base domain
#[pyfunction]
pub fn is_external_url(url: &str, base_domain: &str) -> bool {
    let special = ["mailto:", "tel:", "ftp:", "file:", "data:", "javascript:"];
    let lower = url.to_lowercase();
    if special.iter().any(|p| lower.starts_with(p)) {
        return true;
    }
    match url::Url::parse(url) {
        Ok(parsed) => {
            let netloc = parsed.host_str().unwrap_or("").to_lowercase();
            if netloc.is_empty() {
                return false;
            }
            let url_domain = netloc.split(':').next().unwrap_or("").replace("www.", "");
            let base = base_domain
                .to_lowercase()
                .split(':')
                .next()
                .unwrap_or("")
                .replace("www.", "");
            !url_domain.ends_with(&base)
        }
        Err(_) => false,
    }
}

fn strip_query_tracking(
    query: &str,
    drop_query_tracking: bool,
    sort_query: bool,
    extra_drop_params: Option<&Vec<String>>,
) -> String {
    if query.is_empty() {
        return String::new();
    }
    let mut params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if drop_query_tracking {
        let mut drop: Vec<String> = TRACKING_PARAMS.iter().map(|s| s.to_string()).collect();
        if let Some(extra) = extra_drop_params {
            drop.extend(extra.iter().map(|p| p.to_lowercase()));
        }
        params.retain(|(k, _)| !drop.contains(&k.to_lowercase()));
    }

    if sort_query {
        params.sort_by(|a, b| a.0.cmp(&b.0));
    }

    if params.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &params {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    }
}

/// Normalize URL with full options
#[pyfunction]
#[pyo3(signature = (href, base_url, drop_query_tracking = true, sort_query = true, keep_fragment = false, extra_drop_params = None, preserve_https = false, original_scheme = None))]
#[allow(clippy::too_many_arguments)]
pub fn normalize_url(
    href: &str,
    base_url: &str,
    drop_query_tracking: bool,
    sort_query: bool,
    keep_fragment: bool,
    extra_drop_params: Option<Vec<String>>,
    preserve_https: bool,
    original_scheme: Option<String>,
) -> PyResult<Option<String>> {
    let href_stripped = href.trim();
    if href_stripped.is_empty() {
        return Ok(None);
    }

    let base = url::Url::parse(base_url).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;
    let full_url = base.join(href_stripped).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;

    // Preserve HTTPS
    if preserve_https
        && original_scheme.as_deref() == Some("https")
        && full_url.scheme() == "http"
    {
        let base_netloc = base.host_str().unwrap_or("").to_lowercase();
        let full_netloc = full_url.host_str().unwrap_or("").to_lowercase();
        if full_netloc == base_netloc && !href_stripped.starts_with("//") {
            // Rebuild with https scheme
            let mut s = String::from("https://");
            let port = full_url
                .port()
                .map(|p| format!(":{}", p))
                .unwrap_or_default();
            s.push_str(&full_netloc);
            s.push_str(&port);
            s.push_str(full_url.path());
            if let Some(q) = full_url.query() {
                s.push('?');
                s.push_str(q);
            }
            if keep_fragment {
                if let Some(f) = full_url.fragment() {
                    s.push('#');
                    s.push_str(f);
                }
            }
            return Ok(Some(s));
        }
    }

    // Lowercase netloc
    let netloc = full_url.host_str().unwrap_or("").to_lowercase();
    let port = full_url
        .port()
        .map(|p| format!(":{}", p))
        .unwrap_or_default();
    let netloc_with_port = format!("{}{}", netloc, port);

    let path = full_url.path().to_string();

    // Query processing
    let query = full_url.query().unwrap_or("");
    let query_processed =
        strip_query_tracking(query, drop_query_tracking, sort_query, extra_drop_params.as_ref());

    // Fragment
    let fragment = if keep_fragment {
        full_url.fragment().unwrap_or("").to_string()
    } else {
        String::new()
    };

    // Reassemble
    let scheme = full_url.scheme();
    let mut result = String::new();
    result.push_str(scheme);
    result.push_str("://");
    result.push_str(&netloc_with_port);
    result.push_str(&path);
    if !query_processed.is_empty() {
        result.push('?');
        result.push_str(&query_processed);
    }
    if !fragment.is_empty() {
        result.push('#');
        result.push_str(&fragment);
    }
    Ok(Some(result))
}

fn strip_deep_crawl_tracking(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }
    let mut params: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let tracking = ["utm_source", "utm_medium", "utm_campaign", "ref", "fbclid"];
    params.retain(|(k, _)| !tracking.contains(&k.as_str()));
    if params.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in &params {
            serializer.append_pair(k, v);
        }
        serializer.finish()
    }
}

/// Normalize URL for deep crawl (simpler)
#[pyfunction]
#[pyo3(signature = (href, base_url, preserve_https = false, original_scheme = None))]
pub fn normalize_url_for_deep_crawl(
    href: &str,
    base_url: &str,
    preserve_https: bool,
    original_scheme: Option<String>,
) -> PyResult<Option<String>> {
    let href_stripped = href.trim();
    if href_stripped.is_empty() {
        return Ok(None);
    }

    let base = url::Url::parse(base_url).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;
    let full_url = base.join(href_stripped).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;

    let mut full_url = full_url;

    // Preserve HTTPS
    if preserve_https
        && original_scheme.as_deref() == Some("https")
        && full_url.scheme() == "http"
    {
        let base_netloc = base.host_str().unwrap_or("").to_lowercase();
        let full_netloc = full_url.host_str().unwrap_or("").to_lowercase();
        if full_netloc == base_netloc && !href_stripped.starts_with("//") {
            full_url.set_scheme("https").ok();
        }
    }

    // Lowercase netloc
    let netloc = full_url.host_str().unwrap_or("").to_lowercase();
    let port = full_url
        .port()
        .map(|p| format!(":{}", p))
        .unwrap_or_default();
    let netloc_with_port = format!("{}{}", netloc, port);

    // Path or '/'
    let path = full_url.path();
    let path = if path.is_empty() { "/" } else { path };

    // Query: remove tracking params
    let query = full_url.query().unwrap_or("");
    let query_processed = strip_deep_crawl_tracking(query);

    // Reassemble without fragment
    let scheme = full_url.scheme();
    let mut result = String::new();
    result.push_str(scheme);
    result.push_str("://");
    result.push_str(&netloc_with_port);
    result.push_str(path);
    if !query_processed.is_empty() {
        result.push('?');
        result.push_str(&query_processed);
    }
    Ok(Some(result))
}

/// Efficient URL normalization (lru_cache-aware variant)
#[pyfunction]
#[pyo3(signature = (href, base_url, preserve_https = false, original_scheme = None))]
pub fn efficient_normalize_url_for_deep_crawl(
    href: &str,
    base_url: &str,
    preserve_https: bool,
    original_scheme: Option<String>,
) -> PyResult<Option<String>> {
    // Same as normalize_url_for_deep_crawl but keeps the query as-is
    let href_stripped = href.trim();
    if href_stripped.is_empty() {
        return Ok(None);
    }

    let base = url::Url::parse(base_url).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;
    let full_url = base.join(href_stripped).map_err(|e| {
        PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string())
    })?;

    let mut full_url = full_url;

    if preserve_https
        && original_scheme.as_deref() == Some("https")
        && full_url.scheme() == "http"
    {
        let base_netloc = base.host_str().unwrap_or("").to_lowercase();
        let full_netloc = full_url.host_str().unwrap_or("").to_lowercase();
        if full_netloc == base_netloc && !href_stripped.starts_with("//") {
            full_url.set_scheme("https").ok();
        }
    }

    let netloc = full_url.host_str().unwrap_or("").to_lowercase();
    let port = full_url
        .port()
        .map(|p| format!(":{}", p))
        .unwrap_or_default();
    let netloc_with_port = format!("{}{}", netloc, port);

    let path = full_url.path();
    let path = if path.is_empty() { "/" } else { path };

    let query = full_url.query().unwrap_or("");

    let scheme = full_url.scheme();
    let mut result = String::new();
    result.push_str(scheme);
    result.push_str("://");
    result.push_str(&netloc_with_port);
    result.push_str(path);
    if !query.is_empty() {
        result.push('?');
        result.push_str(query);
    }
    Ok(Some(result))
}

/// Quick link extraction from HTML for prefetch mode
#[pyfunction]
pub fn quick_extract_links(html: &str, base_url: &str) -> PyResult<PyObject> {
    let py = unsafe { Python::assume_gil_acquired() };

    let document = scraper::Html::parse_document(html);
    let selector = scraper::Selector::parse("a[href]").unwrap();

    // Determine base domain from the page URL
    let base_domain = get_base_domain(base_url);

    // Check for <base href> in the HTML
    let base_sel = scraper::Selector::parse("head > base[href]").unwrap();
    let effective_base = if let Some(base_el) = document.select(&base_sel).next() {
        if let Some(href_val) = base_el.value().attr("href") {
            let href_val = href_val.trim();
            if !href_val.is_empty() {
                if let Ok(base_url_obj) = url::Url::parse(base_url) {
                    if let Ok(joined) = base_url_obj.join(href_val) {
                        joined.as_str().to_string()
                    } else {
                        base_url.to_string()
                    }
                } else {
                    base_url.to_string()
                }
            } else {
                base_url.to_string()
            }
        } else {
            base_url.to_string()
        }
    } else {
        base_url.to_string()
    };

    let mut internal: Vec<PyObject> = Vec::new();
    let mut external: Vec<PyObject> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for element in document.select(&selector) {
        let href = element.value().attr("href").unwrap_or("").trim().to_string();
        if href.is_empty()
            || href.starts_with('#')
            || href.starts_with("javascript:")
            || href.starts_with("mailto:")
            || href.starts_with("tel:")
        {
            continue;
        }

        // Normalize
        let normalized = match efficient_normalize_url_for_deep_crawl(
            &href,
            &effective_base,
            false,
            None,
        ) {
            Ok(Some(url)) => url,
            _ => continue,
        };

        if seen.contains(&normalized) {
            continue;
        }
        seen.insert(normalized.clone());

        // Extract text
        let text: String = element
            .text()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .chars()
            .take(200)
            .collect();

        let link_dict = PyDict::new(py);
        link_dict.set_item("href", &normalized).ok();
        link_dict.set_item("text", &text).ok();

        let is_external = is_external_url(&normalized, &base_domain);
        if is_external {
            external.push(link_dict.into());
        } else {
            internal.push(link_dict.into());
        }
    }

    let result = PyDict::new(py);
    result.set_item("internal", PyList::new(py, &internal)?).ok();
    result.set_item("external", PyList::new(py, &external)?).ok();
    Ok(result.into())
}
