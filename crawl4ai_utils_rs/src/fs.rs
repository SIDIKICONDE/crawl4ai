use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Create content directories
#[pyfunction]
pub fn ensure_content_dirs(base_path: &str) -> PyResult<PyObject> {
    let py = unsafe { Python::assume_gil_acquired() };
    let dirs = [
        ("html", "html_content"),
        ("cleaned", "cleaned_html"),
        ("markdown", "markdown_content"),
        ("extracted", "extracted_content"),
        ("screenshots", "screenshots"),
        ("screenshot", "screenshots"),
    ];

    let result = PyDict::new(py);
    for (key, dirname) in &dirs {
        let path = std::path::Path::new(base_path).join(dirname);
        std::fs::create_dir_all(&path).map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyOSError, _>(format!(
                "Cannot create directory {}: {}",
                path.display(),
                e
            ))
        })?;
        result.set_item(*key, path.to_string_lossy().to_string()).ok();
    }
    Ok(result.into())
}

/// Get the home folder path
#[pyfunction]
pub fn get_home_folder() -> PyResult<String> {
    // Respect CRAWL4_AI_BASE_DIRECTORY like the Python implementation
    let base = if let Ok(dir) = std::env::var("CRAWL4_AI_BASE_DIRECTORY") {
        std::path::PathBuf::from(dir)
    } else {
        dirs::home_dir().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyOSError, _>("Cannot find home directory")
        })?
    };
    let crawl4ai_dir = base.join(".crawl4ai");
    std::fs::create_dir_all(&crawl4ai_dir)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    std::fs::create_dir_all(crawl4ai_dir.join("cache"))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    std::fs::create_dir_all(crawl4ai_dir.join("models"))
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyOSError, _>(e.to_string()))?;
    Ok(crawl4ai_dir.to_string_lossy().to_string())
}
