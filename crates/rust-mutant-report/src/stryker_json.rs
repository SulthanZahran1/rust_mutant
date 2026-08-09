//! Stryker mutation-testing-elements JSON adapter.

use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;

use crate::{Report, family_summary, source_for, status_reason, stryker_status};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StrykerReport {
    schema_version: String,
    thresholds: Thresholds,
    project_root: String,
    config: serde_json::Value,
    files: BTreeMap<String, StrykerFile>,
}

#[derive(Debug, Serialize)]
struct Thresholds {
    high: u32,
    low: u32,
}

#[derive(Debug, Serialize)]
struct StrykerFile {
    language: &'static str,
    source: String,
    mutants: Vec<StrykerMutant>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StrykerMutant {
    id: String,
    location: Location,
    mutator_name: String,
    status: String,
    replacement: String,
    description: String,
    duration: u128,
    covered_by: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    killed_by: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    status_reason: String,
    tests_completed: usize,
    static_mutant: bool,
}

#[derive(Debug, Serialize)]
struct Location {
    start: Position,
    end: Position,
}

#[derive(Debug, Serialize)]
struct Position {
    line: usize,
    column: usize,
}

pub fn generate(report: &Report) -> Result<String> {
    let mut grouped: BTreeMap<String, Vec<&rust_mutant_core::MutantResult>> = BTreeMap::new();
    for result in &report.mutants {
        grouped
            .entry(result.mutant.file.clone())
            .or_default()
            .push(result);
    }
    let files = grouped
        .into_iter()
        .map(|(file, results)| {
            let mutants = results
                .into_iter()
                .map(|result| {
                    let end_column = result
                        .mutant
                        .column
                        .saturating_add(result.mutant.original.chars().count().max(1));
                    let covered_by = result.tests_run.clone();
                    let killed_by = if result.status == "killed" {
                        covered_by.clone()
                    } else {
                        Vec::new()
                    };
                    StrykerMutant {
                        id: result.mutant.id.clone(),
                        location: Location {
                            start: Position {
                                line: result.mutant.line,
                                column: result.mutant.column,
                            },
                            end: Position {
                                line: result.mutant.line,
                                column: end_column,
                            },
                        },
                        mutator_name: result.mutant.family.clone(),
                        status: stryker_status(&result.status).into(),
                        replacement: result.mutant.replacement.clone(),
                        description: format!(
                            "{} {}: {} -> {}",
                            result.mutant.family,
                            result.mutant.subtype,
                            result.mutant.original,
                            result.mutant.replacement
                        ),
                        duration: result.duration_ms,
                        covered_by,
                        killed_by,
                        status_reason: status_reason(result),
                        tests_completed: result.tests_run.len(),
                        static_mutant: false,
                    }
                })
                .collect();
            (
                file.clone(),
                StrykerFile {
                    language: "rust",
                    source: source_for(report, &file),
                    mutants,
                },
            )
        })
        .collect();
    let threshold = report.summary.threshold.unwrap_or(80.0).clamp(0.0, 100.0) as u32;
    let output = StrykerReport {
        schema_version: "2".into(),
        thresholds: Thresholds {
            high: threshold,
            low: threshold.saturating_sub(20),
        },
        project_root: report.project.path.clone(),
        config: serde_json::json!({
            "rustMutant": {
                "schemaVersion": report.schema_version,
                "msi": report.summary.msi,
                "threshold": report.summary.threshold,
                "thresholdPassed": report.summary.threshold_passed,
                "excludedBuckets": report.summary.excluded_buckets,
                "familySummary": family_summary(report),
            }
        }),
        files,
    };
    Ok(serde_json::to_string_pretty(&output)?)
}

pub fn generate_to_file(report: &Report, path: &std::path::Path) -> Result<()> {
    crate::write_report(path, &generate(report)?)?;
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rust_mutant_core::{
        Mutant, MutantResult, ProjectInfo, Report, Resources, RoutingInfo, Summary, Timing,
        ToolInfo,
    };

    pub(crate) fn report() -> Report {
        let mutant = Mutant {
            id: "m0001-test".into(),
            file: "src/lib.rs".into(),
            line: 4,
            column: 5,
            family: "AOR".into(),
            subtype: "+ -> -".into(),
            original: "+".into(),
            replacement: "-".into(),
            start_byte: 0,
            end_byte: 1,
            source_line: "a + b".into(),
        };
        Report {
            schema_version: 1,
            tool: ToolInfo {
                name: "rust-mutant",
                version: "1.0.0",
            },
            project: ProjectInfo {
                path: ".".into(),
                manifest: "Cargo.toml".into(),
            },
            summary: Summary {
                total: 1,
                killed: 1,
                survived: 0,
                not_covered: 0,
                compile_error: 0,
                timeout: 0,
                equivalent: 0,
                msi: 100.0,
                threshold: Some(80.0),
                threshold_passed: true,
                excluded_buckets: vec![],
            },
            mutants: vec![MutantResult {
                mutant,
                status: "killed".into(),
                tests_run: vec!["test_a".into()],
                duration_ms: 3,
                cache: "miss".into(),
                command: None,
                details: None,
                tce: None,
            }],
            timing: Timing {
                routing_ms: 0,
                execution_ms: 3,
                cache_ms: 0,
                tce_ms: 0,
                total_ms: 3,
            },
            resources: Resources {
                requested_workers: 1,
                effective_workers: 1,
                global_cpu_budget: 1,
                active_sessions: 1,
                memory_budget_mib: None,
                peak_rss_mib: 1,
                wait_ms: 0,
                throttled: false,
            },
            routing: RoutingInfo {
                enabled: false,
                backend: "none".into(),
                tests_discovered: 0,
                mapped_mutants: 0,
                full_suite_comparison: false,
            },
            cache_hits: 0,
        }
    }

    #[test]
    fn official_shape_and_status_are_present() {
        let value: serde_json::Value = serde_json::from_str(&generate(&report()).unwrap()).unwrap();
        assert_eq!(value["schemaVersion"], "2");
        assert!(value["files"].is_object());
        assert_eq!(value["files"]["src/lib.rs"]["language"], "rust");
        assert_eq!(
            value["files"]["src/lib.rs"]["mutants"][0]["status"],
            "Killed"
        );
    }
}
