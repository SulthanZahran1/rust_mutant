use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn binary() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_rust-mutant"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name)
}

fn run(args: &[&str]) -> Output {
    Command::new(binary())
        .args(args)
        .output()
        .expect("rust-mutant binary should run")
}

fn report_dir(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("rust-mutant-goal4-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("report directory should be creatable");
    path
}

#[test]
fn help_lists_frozen_contract_surface() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let help = String::from_utf8_lossy(&output.stdout);
    for item in [
        "stryker-json",
        "junit",
        "html",
        "--no-tce",
        "--threshold",
        "--config",
        "--mutants-file",
    ] {
        assert!(help.contains(item), "help omitted {item}: {help}");
    }
}

#[test]
fn invalid_project_has_contract_exit_two() {
    let output = run(&[
        "--path",
        "/definitely/not/a/rust-mutant-project",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not exist"));
}

#[test]
fn empty_project_has_valid_json_and_exit_three() {
    let output = run(&[
        "--path",
        fixture("empty").to_str().unwrap(),
        "--format",
        "json",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert_eq!(report["summary"]["total"], 0);
    assert_eq!(report["summary"]["thresholdPassed"], true);
}

#[test]
fn threshold_failure_has_contract_exit_one() {
    let output = run(&[
        "--path",
        fixture("tce").to_str().unwrap(),
        "--operators",
        "AOR",
        "--format",
        "json",
        "--threshold",
        "100",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["thresholdPassed"], false);
}

#[test]
fn repeated_json_runs_match_outside_timing() {
    let fixture_path = fixture("tce");
    let args = [
        "--path",
        fixture_path.to_str().unwrap(),
        "--operators",
        "AOR",
        "--format",
        "json",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ];
    let first: serde_json::Value = serde_json::from_slice(&run(&args).stdout).unwrap();
    let second: serde_json::Value = serde_json::from_slice(&run(&args).stdout).unwrap();
    let mut first = first;
    let mut second = second;
    for report in [&mut first, &mut second] {
        report["timing"] = serde_json::Value::Null;
        report["resources"] = serde_json::Value::Null;
        if let Some(mutants) = report["mutants"].as_array_mut() {
            for mutant in mutants {
                mutant["durationMs"] = serde_json::Value::Null;
            }
        }
    }
    assert_eq!(first, second);
}

#[test]
fn ror_ignores_generic_type_delimiters() {
    let output = run(&[
        "--path",
        fixture("ror-generics-mre").to_str().unwrap(),
        "--operators",
        "ROR",
        "--format",
        "json",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["total"], 1);
    assert_eq!(report["summary"]["killed"], 1);
    assert_eq!(report["summary"]["compileError"], 0);
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(mutants.len(), 1, "unexpected ROR mutants: {mutants:?}");
    assert_eq!(mutants[0]["family"], "ROR");
    assert_eq!(mutants[0]["original"], "<");
    assert_eq!(mutants[0]["replacement"], ">");
    assert_eq!(mutants[0]["file"], "src/lib.rs");
    assert_eq!(mutants[0]["line"], 10);
    assert_eq!(mutants[0]["column"], 10);
}

#[test]
fn default_tce_has_six_equivalents_and_no_false_equivalents() {
    let output = run(&[
        "--path",
        fixture("tce").to_str().unwrap(),
        "--operators",
        "AOR",
        "--format",
        "json",
        "--threshold",
        "0",
        "--no-cache",
    ]);
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["summary"]["total"], 12);
    assert_eq!(report["summary"]["equivalent"], 6);
    assert_eq!(report["summary"]["killed"], 6);
    assert_eq!(report["summary"]["notCovered"], 0);
    assert!(report["timing"]["tceMs"].as_u64().unwrap() > 0);
    let mutants = report["mutants"].as_array().unwrap();
    assert_eq!(
        mutants
            .iter()
            .filter(|mutant| mutant["subtype"] == "commutative-swap")
            .count(),
        1
    );
    assert!(mutants.iter().all(|mutant| {
        mutant["sourceLine"]
            .as_str()
            .is_none_or(|line| !line.contains("x + 1") || mutant["status"] == "killed")
    }));
}

#[test]
fn invalid_operator_has_contract_exit_two() {
    let output = run(&[
        "--path",
        fixture("tce").to_str().unwrap(),
        "--operators",
        "not-a-family",
        "--format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown operator family"));
}

#[test]
fn all_report_adapters_preserve_mutant_total() {
    let directory = report_dir("formats");
    for format in ["stryker-json", "junit", "html"] {
        let output = run(&[
            "--path",
            fixture("tce").to_str().unwrap(),
            "--operators",
            "AOR",
            "--format",
            format,
            "--threshold",
            "0",
            "--no-tce",
            "--no-routing",
            "--no-cache",
            "--output",
            directory.to_str().unwrap(),
        ]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{format}: {:?}",
            output.stderr
        );
    }
    let stryker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(directory.join("mutation-report.json")).unwrap())
            .unwrap();
    let mutant_total: usize = stryker["files"]
        .as_object()
        .unwrap()
        .values()
        .map(|file| file["mutants"].as_array().unwrap().len())
        .sum();
    assert_eq!(mutant_total, 12);
    assert_eq!(stryker["config"]["rustMutant"]["msi"], 50.0);
    assert_eq!(
        stryker["config"]["rustMutant"]["familySummary"]["AOR"]["total"],
        12
    );
    let junit = fs::read_to_string(directory.join("mutation-results.xml")).unwrap();
    assert!(junit.contains("tests=\"12\""));
    assert!(junit.contains("rust-mutant.msi"));
    assert!(junit.contains("rust-mutant.family.AOR.total"));
    let html = fs::read_to_string(directory.join("mutation-report.html")).unwrap();
    assert!(html.contains("equivalent"));
    assert!(html.contains("<h2>Families</h2>"));
    assert!(!html.contains("http://") && !html.contains("https://"));
    let _ = fs::remove_dir_all(directory);
}

#[test]
fn console_is_stdout_only_and_json_alias_is_valid() {
    let console = run(&[
        "--path",
        fixture("empty").to_str().unwrap(),
        "--format",
        "console",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ]);
    assert_eq!(console.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&console.stdout).contains("total mutants: 0"));
    assert!(console.stderr.is_empty());

    let alias = run(&[
        "--path",
        fixture("empty").to_str().unwrap(),
        "--json",
        "--no-tce",
        "--no-routing",
        "--no-cache",
    ]);
    assert_eq!(alias.status.code(), Some(3));
    let _: serde_json::Value = serde_json::from_slice(&alias.stdout).unwrap();
}
