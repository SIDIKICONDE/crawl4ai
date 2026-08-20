pub mod chunking;
pub mod fs;
pub mod hash;
pub mod math;
pub mod memory;
pub mod sanitize;
pub mod token;
pub mod url;

use pyo3::prelude::*;

/// Python module definition
#[pymodule]
fn crawl4ai_utils(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Chunking
    m.add_function(wrap_pyfunction!(chunking::chunk_documents, m)?)?;
    m.add_function(wrap_pyfunction!(chunking::merge_chunks, m)?)?;
    m.add_function(wrap_pyfunction!(chunking::merge_chunks_based_on_token_threshold, m)?)?;
    m.add_function(wrap_pyfunction!(chunking::advanced_split, m)?)?;
    // Sanitize / JSON
    m.add_function(wrap_pyfunction!(sanitize::sanitize_html, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize::sanitize_input_encode, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize::escape_json_string, m)?)?;
    m.add_function(wrap_pyfunction!(sanitize::split_and_parse_json_objects, m)?)?;
    // URL
    m.add_function(wrap_pyfunction!(url::get_base_domain, m)?)?;
    m.add_function(wrap_pyfunction!(url::is_external_url, m)?)?;
    m.add_function(wrap_pyfunction!(url::normalize_url, m)?)?;
    m.add_function(wrap_pyfunction!(url::normalize_url_for_deep_crawl, m)?)?;
    m.add_function(wrap_pyfunction!(url::efficient_normalize_url_for_deep_crawl, m)?)?;
    m.add_function(wrap_pyfunction!(url::quick_extract_links, m)?)?;
    // Token
    m.add_function(wrap_pyfunction!(token::clean_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(token::truncate, m)?)?;
    // FS
    m.add_function(wrap_pyfunction!(fs::ensure_content_dirs, m)?)?;
    m.add_function(wrap_pyfunction!(fs::get_home_folder, m)?)?;
    // Hash
    m.add_function(wrap_pyfunction!(hash::generate_content_hash, m)?)?;
    m.add_function(wrap_pyfunction!(hash::compute_head_fingerprint, m)?)?;
    // Memory / system
    m.add_function(wrap_pyfunction!(memory::calculate_semaphore_count, m)?)?;
    m.add_function(wrap_pyfunction!(memory::get_system_memory, m)?)?;
    m.add_function(wrap_pyfunction!(memory::get_true_available_memory_gb, m)?)?;
    m.add_function(wrap_pyfunction!(memory::get_true_memory_usage_percent, m)?)?;
    m.add_function(wrap_pyfunction!(memory::get_memory_stats, m)?)?;
    // Math
    m.add_function(wrap_pyfunction!(math::cosine_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(math::cosine_distance, m)?)?;
    Ok(())
}
