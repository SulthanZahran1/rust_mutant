use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use rust_mutant_core::{RunOptions, operator_families, run};
use rust_mutant_report::{console, html, junit_xml, stryker_json};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "rust-mutant",
    version,
    about = "AST-based mutation testing for Rust"
)]
struct Cli {
    /// Cargo project directory to mutate. Defaults to the current directory.
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(long)]
    manifest_path: Option<PathBuf>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    no_config: bool,
    #[arg(long, value_enum)]
    format: Option<Format>,
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
    /// File containing one mutant ID per line. Runs all listed mutants in one process.
    #[arg(long)]
    mutants_file: Option<PathBuf>,
    #[arg(long, value_parser = parse_duration)]
    timeout: Option<Duration>,
    #[arg(long)]
    threshold: Option<f64>,
    #[arg(long)]
    parallel: Option<usize>,
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

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    path: Option<PathBuf>,
    manifest_path: Option<PathBuf>,
    format: Option<String>,
    output: Option<PathBuf>,
    threshold: Option<f64>,
    parallel: Option<usize>,
    timeout: Option<String>,
    operators: Option<Vec<String>>,
    no_tce: Option<bool>,
    no_routing: Option<bool>,
    no_cache: Option<bool>,
    incremental: Option<bool>,
    base_ref: Option<String>,
    max_memory: Option<u64>,
}

struct LoadedConfig {
    value: Config,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("rust-mutant: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<u8> {
    let cli = Cli::parse();
    let loaded = load_config(&cli)?;
    if cli.list_operators {
        if cli.json || matches!(cli.format, Some(Format::Json)) {
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

    let config = loaded.value;
    let project = cli
        .path
        .or(config.path)
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = cli.manifest_path.or(config.manifest_path);
    let format = if cli.json {
        Format::Json
    } else if let Some(format) = cli.format {
        format
    } else if let Some(format) = config.format.as_deref() {
        parse_format(format)?
    } else {
        Format::Console
    };
    let output = cli.output.or(config.output);
    let operators = cli
        .operators
        .or_else(|| config.operators.map(|values| values.join(",")))
        .as_deref()
        .map(parse_operators)
        .transpose()?;
    let timeout = match cli.timeout {
        Some(value) => value,
        None => match config.timeout.as_deref() {
            Some(value) => parse_duration(value).map_err(|error| anyhow::anyhow!(error))?,
            None => Duration::from_secs(2),
        },
    };
    let threshold = cli.threshold.or(config.threshold).unwrap_or(80.0);
    if !(0.0..=100.0).contains(&threshold) {
        bail!("--threshold must be between 0 and 100");
    }
    let incremental = cli.incremental || config.incremental.unwrap_or(false);
    let base_ref = cli.base_ref.or(config.base_ref);
    if incremental && base_ref.is_none() {
        bail!("--incremental requires --base-ref <REF>");
    }
    if cli.mutant.is_some() && cli.mutants_file.is_some() {
        bail!("--mutant and --mutants-file are mutually exclusive");
    }
    let mutant_ids = cli
        .mutants_file
        .as_deref()
        .map(read_mutant_ids)
        .transpose()?;
    let options = RunOptions {
        project: project.clone(),
        manifest,
        timeout,
        threshold: Some(threshold),
        operators,
        mutant: cli.mutant,
        mutant_ids,
        dry_run: cli.dry_run,
        requested_workers: cli.parallel.or(config.parallel).unwrap_or(1).max(1),
        no_cache: cli.no_cache || config.no_cache.unwrap_or(false),
        routing: !(cli.no_routing || config.no_routing.unwrap_or(false)),
        incremental,
        base_ref,
        max_memory_mib: cli.max_memory.or(config.max_memory),
        tce: !(cli.no_tce || config.no_tce.unwrap_or(false)),
    };
    let report = run(&options)?;
    render(&report, format, output.as_deref(), &project, cli.quiet)?;
    if report.summary.total == 0 {
        Ok(3)
    } else if report.summary.threshold_passed {
        Ok(0)
    } else {
        Ok(1)
    }
}

fn render(
    report: &rust_mutant_core::Report,
    format: Format,
    output: Option<&Path>,
    project: &Path,
    quiet: bool,
) -> Result<()> {
    match format {
        Format::Console => {
            if !quiet {
                print!("{}", console::generate(report));
            }
        }
        Format::Json => println!("{}", serde_json::to_string_pretty(report)?),
        Format::StrykerJson => {
            let path = report_path(output, project, "mutation-report.json");
            stryker_json::generate_to_file(report, &path)?;
            eprintln!("stryker report: {}", path.display());
        }
        Format::Junit => {
            let path = report_path(output, project, "mutation-results.xml");
            junit_xml::generate_to_file(report, &path)?;
            eprintln!("junit report: {}", path.display());
        }
        Format::Html => {
            let path = report_path(output, project, "mutation-report.html");
            html::generate_to_file(report, &path)?;
            eprintln!("html report: {}", path.display());
        }
    }
    Ok(())
}

fn report_path(output: Option<&Path>, project: &Path, filename: &str) -> PathBuf {
    output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| project.join("mutation-reports"))
        .join(filename)
}

fn load_config(cli: &Cli) -> Result<LoadedConfig> {
    if cli.no_config {
        return Ok(LoadedConfig {
            value: Config::default(),
        });
    }
    let auto_path = cli
        .path
        .clone()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rust-mutant.toml");
    let path = cli.config.clone().unwrap_or(auto_path);
    if !path.is_file() {
        if cli.config.is_some() {
            bail!("configuration file does not exist: {}", path.display());
        }
        return Ok(LoadedConfig {
            value: Config::default(),
        });
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("read configuration {}", path.display()))?;
    let value = toml::from_str(&text)
        .with_context(|| format!("parse TOML configuration {}", path.display()))?;
    Ok(LoadedConfig { value })
}

fn parse_format(value: &str) -> Result<Format> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "console" => Ok(Format::Console),
        "json" => Ok(Format::Json),
        "stryker-json" | "stryker" => Ok(Format::StrykerJson),
        "junit" | "junit-xml" => Ok(Format::Junit),
        "html" => Ok(Format::Html),
        other => bail!(
            "unknown report format `{other}`; expected console, json, stryker-json, junit, or html"
        ),
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

fn read_mutant_ids(path: &Path) -> Result<BTreeSet<String>> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read mutant manifest {}", path.display()))?;
    let ids = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();
    if ids.is_empty() {
        bail!("mutant manifest {} contains no IDs", path.display());
    }
    Ok(ids)
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
