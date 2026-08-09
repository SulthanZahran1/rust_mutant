//! Human-readable console report.

use crate::{Report, count_by_status};

pub fn generate(report: &Report) -> String {
    let counts = count_by_status(report);
    let threshold = report
        .summary
        .threshold
        .map(|value| format!("{value:.2}%"))
        .unwrap_or_else(|| "disabled".into());
    let threshold_state = if report.summary.threshold_passed {
        "PASS"
    } else {
        "FAIL"
    };
    format!(
        "rust-mutant {}\n\
         project: {}\n\
         total mutants: {}\n\
         killed: {}\n\
         survived: {}\n\
         equivalent: {}\n\
         not covered: {}\n\
         compile error: {}\n\
         timeout: {}\n\
         MSI: {:.2}%\n\
         threshold: {} ({})\n\
         timing: routing={}ms execution={}ms cache={}ms tce={}ms total={}ms\n\
         resources: requestedWorkers={} effectiveWorkers={} globalCpuBudget={} peakRssMib={} throttled={}\n",
        report.tool.version,
        report.project.path,
        counts.total,
        counts.killed,
        counts.survived,
        counts.equivalent,
        counts.not_covered,
        counts.compile_error,
        counts.timeout,
        report.summary.msi,
        threshold,
        threshold_state,
        report.timing.routing_ms,
        report.timing.execution_ms,
        report.timing.cache_ms,
        report.timing.tce_ms,
        report.timing.total_ms,
        report.resources.requested_workers,
        report.resources.effective_workers,
        report.resources.global_cpu_budget,
        report.resources.peak_rss_mib,
        report.resources.throttled,
    )
}
