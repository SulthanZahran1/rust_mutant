//! Self-contained HTML mutation report.

use crate::{Report, escape_html, family_summary, status_css_class, status_display, status_reason};

pub fn generate(report: &Report) -> String {
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>rust-mutant report</title><style>\
        :root{color-scheme:dark;--bg:#07110e;--panel:#10221b;--line:#234438;--text:#d9eee4;--muted:#91b2a3;--accent:#55e6a5}*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:14px ui-monospace,SFMono-Regular,Consolas,monospace}main{max-width:1400px;margin:0 auto;padding:28px}h1{color:var(--accent);margin:0 0 6px}h2{margin-top:28px}.muted{color:var(--muted)}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(130px,1fr));gap:10px;margin:24px 0}.card{background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:14px}.card b{display:block;font-size:22px;margin-top:6px}.killed{color:#62e6a5}.survived{color:#ffb86b}.equivalent{color:#8ac7ff}.not-covered{color:#b6a8ff}.compile-error,.timeout{color:#ff7272}table{border-collapse:collapse;width:100%;background:var(--panel)}th,td{border-bottom:1px solid var(--line);padding:9px;text-align:left;vertical-align:top}th{color:var(--accent)}code{white-space:pre-wrap;word-break:break-word}.badge{font-weight:700}</style></head><body><main>",
    );
    html.push_str(&format!(
        "<h1>rust-mutant mutation report</h1><div class=\"muted\">{}</div>",
        escape_html(&report.project.path)
    ));
    let cards = [
        ("Total", report.summary.total, ""),
        ("Killed", report.summary.killed, "killed"),
        ("Survived", report.summary.survived, "survived"),
        ("Equivalent", report.summary.equivalent, "equivalent"),
        ("Not covered", report.summary.not_covered, "not-covered"),
        (
            "Compile error",
            report.summary.compile_error,
            "compile-error",
        ),
        ("Timeout", report.summary.timeout, "timeout"),
    ];
    html.push_str("<section class=\"cards\">");
    for (label, value, class) in cards {
        html.push_str(&format!(
            "<div class=\"card {}\">{}<b>{}</b></div>",
            class, label, value
        ));
    }
    html.push_str("</section>");
    html.push_str(&format!(
        "<p>MSI: <strong>{:.2}%</strong> | threshold: <strong>{}</strong> | result: <strong>{}</strong> | TCE: {} ms | wall: {} ms</p>",
        report.summary.msi,
        report
            .summary
            .threshold
            .map(|value| format!("{value:.2}%"))
            .unwrap_or_else(|| "disabled".into()),
        if report.summary.threshold_passed { "PASS" } else { "FAIL" },
        report.timing.tce_ms,
        report.timing.total_ms
    ));
    html.push_str("<h2>Families</h2><table><thead><tr><th>Family</th><th>Total</th><th>Killed</th><th>Survived</th><th>Equivalent</th><th>Not covered</th><th>Compile error</th><th>Timeout</th></tr></thead><tbody>");
    for (family, counts) in family_summary(report) {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&family),
            counts.total,
            counts.killed,
            counts.survived,
            counts.equivalent,
            counts.not_covered,
            counts.compile_error,
            counts.timeout
        ));
    }
    html.push_str("</tbody></table>");
    html.push_str("<h2>Mutants</h2><table><thead><tr><th>ID</th><th>Location</th><th>Family</th><th>Mutation</th><th>Status</th><th>Tests</th><th>Duration</th><th>Details</th></tr></thead><tbody>");
    for result in &report.mutants {
        let class = status_css_class(&result.status);
        let reason = status_reason(result);
        html.push_str(&format!(
            "<tr><td><code>{}</code></td><td>{}:{}:{}</td><td>{}<br><span class=\"muted\">{}</span></td><td><code>{} → {}</code></td><td class=\"badge {}\">{}</td><td>{}</td><td>{} ms</td><td><code>{}</code></td></tr>",
            escape_html(&result.mutant.id),
            escape_html(&result.mutant.file),
            result.mutant.line,
            result.mutant.column,
            escape_html(&result.mutant.family),
            escape_html(&result.mutant.subtype),
            escape_html(&result.mutant.original),
            escape_html(&result.mutant.replacement),
            class,
            status_display(&result.status),
            escape_html(&result.tests_run.join(", ")),
            result.duration_ms,
            escape_html(&reason)
        ));
    }
    html.push_str("</tbody></table></main></body></html>\n");
    html
}

pub fn generate_to_file(report: &Report, path: &std::path::Path) -> anyhow::Result<()> {
    crate::write_report(path, &generate(report))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stryker_json::tests::report;

    #[test]
    fn report_is_self_contained_and_escaped() {
        let html = generate(&report());
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("Killed"));
        assert!(!html.contains("<script src="));
        assert!(!html.contains("https://"));
    }
}
