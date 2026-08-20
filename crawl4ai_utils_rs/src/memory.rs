use pyo3::prelude::*;

/// Calculate optimal semaphore count based on system resources.
///
/// Faithful port of `utils.calculate_semaphore_count`:
/// `min(max(1, cpu_count // 2), int(memory_gb / 2))`.
#[pyfunction]
pub fn calculate_semaphore_count() -> PyResult<usize> {
    let cpu_count = num_cpus::get();
    let memory_gb = get_system_memory()? as f64 / (1024.0 * 1024.0 * 1024.0);
    let base_count = (cpu_count / 2).max(1);
    let memory_based_cap = (memory_gb / 2.0) as usize; // Assume 2GB per instance
    Ok(base_count.min(memory_based_cap))
}

/// Get total system memory in bytes.
///
/// Faithful port of `utils.get_system_memory` (returns total memory,
/// not available memory).
#[pyfunction]
pub fn get_system_memory() -> PyResult<u64> {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    Ok(sys.total_memory())
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
