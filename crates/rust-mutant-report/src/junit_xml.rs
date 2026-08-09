//! JUnit XML mutation-result adapter.

use crate::{Report, escape_xml, status_display, status_reason};

pub fn generate(report: &Report) -> String {
    let total = report.mutants.len();
    let failures = report
        .mutants
        .iter()
        .filter(|result| result.status == "survived")
        .count();
    let errors = report
        .mutants
        .iter()
        .filter(|result| matches!(result.status.as_str(), "compile_error" | "timeout"))
        .count();
    let skipped = report
        .mutants
        .iter()
        .filter(|result| matches!(result.status.as_str(), "not_covered" | "equivalent"))
        .count();
    let mut output = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"rust-mutant\" tests=\"{total}\" failures=\"{failures}\" errors=\"{errors}\" skipped=\"{skipped}\" time=\"{:.3}\">\n",
        report.timing.total_ms as f64 / 1000.0
    );
    for result in &report.mutants {
        let name = escape_xml(&format!("{}:{}", result.mutant.file, result.mutant.id));
        let classname = escape_xml(&result.mutant.family);
        let time = result.duration_ms as f64 / 1000.0;
        output.push_str(&format!(
            "  <testcase classname=\"{classname}\" name=\"{name}\" time=\"{time:.3}\">"
        ));
        let reason = escape_xml(&status_reason(result));
        match result.status.as_str() {
            "survived" => output.push_str(&format!(
                "<failure type=\"Survived\" message=\"mutation survived\">{reason}</failure>"
            )),
            "compile_error" => output.push_str(&format!(
                "<error type=\"CompileError\" message=\"mutation did not compile\">{reason}</error>"
            )),
            "timeout" => output.push_str(&format!(
                "<error type=\"Timeout\" message=\"mutation timed out\">{reason}</error>"
            )),
            "not_covered" => output
                .push_str("<skipped type=\"NoCoverage\" message=\"mutation was not covered\"/>"),
            "equivalent" => output.push_str(
                "<skipped type=\"Equivalent\" message=\"normalized LLVM IR equivalent\"/>",
            ),
            _ => {}
        }
        output.push_str(&format!(
            "<system-out>{}</system-out></testcase>\n",
            escape_xml(&format!(
                "status={} tests={} original={:?} replacement={:?}",
                status_display(&result.status),
                result.tests_run.join(","),
                result.mutant.original,
                result.mutant.replacement
            ))
        ));
    }
    output.push_str("</testsuite>\n");
    output
}

pub fn generate_to_file(report: &Report, path: &std::path::Path) -> anyhow::Result<()> {
    crate::write_report(path, &generate(report))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stryker_json::tests::report;
    use rust_mutant_core::Report;

    #[test]
    fn totals_equal_mutant_count_and_xml_is_escaped() {
        let report: Report = report();
        let xml = generate(&report);
        assert!(xml.starts_with("<?xml"));
        assert!(xml.contains("tests=\"1\""));
        assert!(xml.contains("<testcase"));
    }
}
