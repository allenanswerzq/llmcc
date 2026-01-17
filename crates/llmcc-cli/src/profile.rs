//! Profiling utilities.

#[cfg(feature = "profile")]
use std::fs::File;

#[cfg(feature = "profile")]
use tracing::info;

/// Profile a phase and generate a flamegraph (when profiling is enabled).
#[cfg(feature = "profile")]
pub fn profile_phase<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    use pprof::ProfilerGuard;

    let guard = ProfilerGuard::new(1000).expect("Failed to start profiler");
    let result = f();

    if let Ok(report) = guard.report().build() {
        let filename = format!("{}.svg", name);
        let file = File::create(&filename).expect("Failed to create flamegraph file");
        report.flamegraph(file).expect("Failed to write flamegraph");
        info!("Flamegraph saved to {}", filename);
    }

    result
}

/// No-op profiling when feature is disabled.
#[cfg(not(feature = "profile"))]
pub fn profile_phase<F, R>(_name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}
