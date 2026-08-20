use pyo3::prelude::*;

/// Calculate optimal semaphore count based on system resources
#[pyfunction]
pub fn calculate_semaphore_count() -> PyResult<usize> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let available = sys.available_memory();
    let total = sys.total_memory();

    let count = if total == 0 {
        10
    } else {
        let ratio = available as f64 / total as f64;
        let suggested = (num_cpus::get() as f64 * ratio).ceil() as usize;
        suggested.clamp(2, 50)
    };
    Ok(count)
}

/// Get system memory in bytes
#[pyfunction]
pub fn get_system_memory() -> PyResult<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    Ok(sys.available_memory())
}

/// Get truly available memory in GB (cross-platform)
#[pyfunction]
pub fn get_true_available_memory_gb() -> f64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let available = sys.available_memory();
    available as f64 / (1024.0 * 1024.0 * 1024.0)
}

/// Get memory usage percentage
#[pyfunction]
pub fn get_true_memory_usage_percent() -> f64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = sys.total_memory();
    let available = sys.available_memory();
    if total == 0 {
        return 0.0;
    }
    let used_percent = 100.0 * (total - available) as f64 / total as f64;
    used_percent.clamp(0.0, 100.0)
}

/// Get comprehensive memory stats: (used_percent, available_gb, total_gb)
#[pyfunction]
pub fn get_memory_stats() -> (f64, f64, f64) {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = sys.total_memory();
    let available = sys.available_memory();
    let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
    let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_percent = if total == 0 {
        0.0
    } else {
        (100.0 * (total - available) as f64 / total as f64).clamp(0.0, 100.0)
    };
    (used_percent, available_gb, total_gb)
}
