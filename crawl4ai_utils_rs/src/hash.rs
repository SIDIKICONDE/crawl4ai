use pyo3::prelude::*;

/// Generate a content hash using xxhash
#[pyfunction]
pub fn generate_content_hash(content: &str) -> String {
    use xxhash_rust::xxh64::xxh64;
    format!("{:x}", xxh64(content.as_bytes(), 0))
}

/// Compute a fingerprint of <head> content for cache validation
#[pyfunction]
pub fn compute_head_fingerprint(head_html: &str) -> String {
    if head_html.is_empty() {
        return String::new();
    }

    let head_lower = head_html.to_lowercase();
    let mut signals: Vec<String> = Vec::new();

    // Extract <title>
    let title_re = regex::Regex::new(r"<title[^>]*>(.*?)</title>").unwrap();
    if let Some(cap) = title_re.captures(&head_lower) {
        if let Some(m) = cap.get(1) {
            signals.push(m.as_str().trim().to_string());
        }
    }

    // Meta tags to extract
    let meta_tags = [
        ("name", "description"),
        ("name", "last-modified"),
        ("property", "og:title"),
        ("property", "og:description"),
        ("property", "og:image"),
        ("property", "og:updated_time"),
        ("property", "article:modified_time"),
    ];

    for (attr_type, attr_value) in &meta_tags {
        let escaped = regex::escape(attr_value);
        let patterns = [
            format!(
                r#"<meta[^>]*{}=["']{}["'][^>]*content=["']([^"']*)["']"#,
                attr_type, escaped
            ),
            format!(
                r#"<meta[^>]*content=["']([^"']*)["'][^>]*{}=["']{}["']"#,
                attr_type, escaped
            ),
        ];
        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(cap) = re.captures(&head_lower) {
                    if let Some(m) = cap.get(1) {
                        signals.push(m.as_str().trim().to_string());
                        break;
                    }
                }
            }
        }
    }

    if signals.is_empty() {
        return String::new();
    }

    use xxhash_rust::xxh64::xxh64;
    let combined = signals.join("|");
    format!("{:x}", xxh64(combined.as_bytes(), 0))
}
