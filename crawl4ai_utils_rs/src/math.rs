use pyo3::prelude::*;

/// Calculate cosine similarity between two vectors
#[pyfunction]
pub fn cosine_similarity(vec1: Vec<f64>, vec2: Vec<f64>) -> f64 {
    let len = vec1.len().min(vec2.len());
    if len == 0 {
        return 0.0;
    }
    let dot_product: f64 = vec1[..len].iter().zip(&vec2[..len]).map(|(a, b)| a * b).sum();
    let norm1: f64 = vec1[..len].iter().map(|a| a * a).sum::<f64>().sqrt();
    let norm2: f64 = vec2[..len].iter().map(|a| a * a).sum::<f64>().sqrt();
    let norm_product = norm1 * norm2;
    if norm_product == 0.0 {
        0.0
    } else {
        dot_product / norm_product
    }
}

/// Calculate cosine distance (1 - similarity) between two vectors
#[pyfunction]
pub fn cosine_distance(vec1: Vec<f64>, vec2: Vec<f64>) -> f64 {
    1.0 - cosine_similarity(vec1, vec2)
}
