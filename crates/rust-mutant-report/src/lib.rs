//! Report adapters for the single rust-mutant result model.
//!
//! The adapters intentionally accept [`rust_mutant_core::Report`] rather than
//! their own execution model. This keeps counts, statuses, TCE metadata, and
//! timing consistent across console, Stryker, JUnit, and HTML output.

pub mod console;
pub mod html;
pub mod junit_xml;
pub mod stryker_json;

pub use rust_mutant_core::{MutantResult, Report, Status, Summary};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub killed: usize,
    pub survived: usize,
    pub not_covered: usize,
    pub compile_error: usize,
    pub timeout: usize,
    pub equivalent: usize,
    pub total: usize,
}

pub fn count_by_status(report: &Report) -> StatusCounts {
    let mut counts = StatusCounts {
        total: report.mutants.len(),
        ..StatusCounts::default()
    };
    for result in &report.mutants {
        match result.status.as_str() {
            "killed" => counts.killed += 1,
            "survived" => counts.survived += 1,
            "not_covered" => counts.not_covered += 1,
            "compile_error" => counts.compile_error += 1,
            "timeout" => counts.timeout += 1,
            "equivalent" => counts.equivalent += 1,
            _ => {}
        }
    }
    counts
}

pub fn stryker_status(status: &str) -> &'static str {
    match status {
        "killed" => "Killed",
        "survived" => "Survived",
        "not_covered" => "NoCoverage",
        "compile_error" => "CompileError",
        "timeout" => "Timeout",
        // The Stryker schema has no equivalent status. Keep the detail in
        // statusReason while mapping it to the semantically closest status.
        "equivalent" => "Survived",
        _ => "Pending",
    }
}

pub fn status_display(status: &str) -> &'static str {
    match status {
        "killed" => "Killed",
        "survived" => "Survived",
        "not_covered" => "Not covered",
        "compile_error" => "Compile error",
        "timeout" => "Timeout",
        "equivalent" => "Equivalent",
        _ => "Pending",
    }
}

pub fn status_css_class(status: &str) -> &'static str {
    match status {
        "not_covered" => "not-covered",
        "compile_error" => "compile-error",
        "killed" => "killed",
        "survived" => "survived",
        "equivalent" => "equivalent",
        "timeout" => "timeout",
        _ => "pending",
    }
}

pub fn write_report(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub(crate) fn source_for(report: &Report, file: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(&report.project.path).join(file))
        .unwrap_or_default()
}

pub(crate) fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn escape_html(value: &str) -> String {
    escape_xml(value)
}

pub(crate) fn status_reason(result: &MutantResult) -> String {
    if result.status == "equivalent" {
        return result
            .tce
            .as_ref()
            .and_then(|tce| {
                tce.original_ir_hash
                    .as_ref()
                    .zip(tce.mutant_ir_hash.as_ref())
            })
            .map(|(original, mutant)| {
                format!("normalized LLVM IR equivalent ({original} == {mutant})")
            })
            .unwrap_or_else(|| "normalized LLVM IR equivalent".into());
    }
    result.details.clone().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_preserves_equivalent_detail_semantics() {
        assert_eq!(stryker_status("equivalent"), "Survived");
        assert_eq!(status_display("equivalent"), "Equivalent");
        assert_eq!(status_css_class("not_covered"), "not-covered");
    }

    #[test]
    fn xml_and_html_escaping_is_safe() {
        let raw = "<&\"'>";
        assert_eq!(escape_xml(raw), "&lt;&amp;&quot;&apos;&gt;");
        assert_eq!(escape_html(raw), "&lt;&amp;&quot;&apos;&gt;");
    }
}
