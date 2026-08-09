use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use rust_mutant_core::{RunOptions, operator_families, run};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "rust-mutant",
    version,
    about = "AST-based mutation testing for Rust"
)]
struct Cli {
    /// Cargo project directory to mutate.
    #[arg(long, default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    manifest_path: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    no_config: bool,
    #[arg(long, value_enum, default_value_t = Format::Console)]
    format: Format,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    list_operators: bool,
    #[arg(long)]
    operators: Option<String>,
    #[arg(long)]
    mutant: Option<String>,
    #[arg(long, default_value = "2s", value_parser = parse_duration)]
    timeout: Duration,
    #[arg(long, default_value_t = 80.0)]
    threshold: f64,
    #[arg(long, default_value_t = 1)]
    parallel: usize,
    #[arg(long)]
    incremental: bool,
    #[arg(long)]
    base_ref: Option<String>,
    #[arg(long)]
    no_tce: bool,
    #[arg(long)]
    no_routing: bool,
    #[arg(long)]
    no_cache: bool,
    #[arg(long)]
    max_memory: Option<u64>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Console,
    Json,
    StrykerJson,
    Junit,
    Html,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("rust-mutant: {error:#}");
            ExitCode::from(if error.to_string().contains("no mutants found") {
                3
            } else {
                2
            })
        }
    }
}

fn real_main() -> Result<u8> {
    let cli = Cli::parse();
    if cli.incremental && cli.base_ref.is_none() {
        bail!("--incremental requires --base-ref <REF>");
    }
    if cli.list_operators {
        if cli.json || matches!(cli.format, Format::Json) {
            let values = operator_families()
                .iter()
                .map(|family| serde_json::json!({"family": family, "subtypes": []}))
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"schemaVersion": 1, "operators": values})
                )?
            );
        } else {
            for family in operator_families() {
                println!("{family}");
            }
        }
        return Ok(0);
    }
    let operators = cli.operators.as_deref().map(parse_operators).transpose()?;
    let options = RunOptions {
        project: cli.path,
        manifest: cli.manifest_path,
        timeout: cli.timeout,
        threshold: Some(cli.threshold),
        operators,
        mutant: cli.mutant,
        dry_run: cli.dry_run,
        requested_workers: cli.parallel.max(1),
        no_cache: cli.no_cache,
        routing: !cli.no_routing,
        incremental: cli.incremental,
        base_ref: cli.base_ref,
        max_memory_mib: cli.max_memory,
    };
    let report = run(&options)?;
    let wants_json = cli.json || matches!(cli.format, Format::Json);
    match cli.format {
        Format::Console if !wants_json => print_console(&report),
        Format::Console | Format::Json => print_json(&report, cli.quiet)?,
        Format::StrykerJson | Format::Junit | Format::Html => {
            bail!(
                "--format {:?} is reserved for the M4 report adapters",
                cli.format
            )
        }
    }
    if report.summary.threshold_passed {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn parse_operators(value: &str) -> Result<BTreeSet<String>> {
    let mut result = BTreeSet::new();
    for value in value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        result.insert(value.to_string());
    }
    if result.is_empty() {
        bail!("--operators requires at least one family");
    }
    Ok(result)
}

fn parse_duration(value: &str) -> Result<Duration, String> {
    let value = value.trim();
    if let Some(number) = value.strip_suffix("ms") {
        return number
            .parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|_| "timeout must be a duration such as 2s or 500ms".to_string());
    }
    if let Some(number) = value.strip_suffix('s') {
        return number
            .parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| "timeout must be a duration such as 2s or 500ms".to_string());
    }
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .map_err(|_| "timeout must be a duration such as 2s or 500ms".to_string())
}

fn print_json(report: &rust_mutant_core::Report, _quiet: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}

fn print_console(report: &rust_mutant_core::Report) {
    println!("rust-mutant {}", report.tool.version);
    println!("project: {}", report.project.path);
    println!("total mutants: {}", report.summary.total);
    println!("killed: {}", report.summary.killed);
    println!("survived: {}", report.summary.survived);
    println!("not covered: {}", report.summary.not_covered);
    println!("compile error: {}", report.summary.compile_error);
    println!("timeout: {}", report.summary.timeout);
    println!("MSI: {:.2}%", report.summary.msi);
    println!("wall time: {} ms", report.timing.total_ms);
}
