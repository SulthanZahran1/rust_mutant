//! Core mutation discovery and the M1 execution pipeline.
//!
//! The first milestone intentionally keeps execution conservative: discovery is
//! syntax-first, every mutant runs in an isolated source-only copy, and the
//! compiler remains the authority for semantic validity.

use anyhow::{Context, Result, anyhow, bail};
use rayon::prelude::*;
use rust_mutant_tce::TceResult;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::{
    io::{Read, Write},
    thread,
};
use tree_sitter::Parser;
use walkdir::WalkDir;

pub const SCHEMA_VERSION: u32 = 1;
const CACHE_SCHEMA_VERSION: u32 = 3;
static PEAK_RSS_MIB: AtomicU64 = AtomicU64::new(0);
pub const GENERIC_FAMILIES: [&str; 10] = [
    "AOR",
    "AOD",
    "AOI",
    "ROR",
    "LOR",
    "LCR",
    "COR",
    "SDL",
    "RVR",
    "loop-inc-dec",
];
pub const IDIOMATIC_FAMILIES: [&str; 8] = [
    "question-mark-removal",
    "unwrap-expect-removal",
    "await-removal",
    "move-closure-removal",
    "mut-to-shared",
    "clone-removal",
    "arc-rc-swap",
    "iterator-chain",
];
pub const PUBLIC_FAMILIES: [&str; 18] = [
    "AOR",
    "AOD",
    "AOI",
    "ROR",
    "LOR",
    "LCR",
    "COR",
    "SDL",
    "RVR",
    "loop-inc-dec",
    "question-mark-removal",
    "unwrap-expect-removal",
    "await-removal",
    "move-closure-removal",
    "mut-to-shared",
    "clone-removal",
    "arc-rc-swap",
    "iterator-chain",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mutant {
    pub id: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub family: String,
    pub subtype: String,
    pub original: String,
    pub replacement: String,
    #[serde(skip)]
    pub start_byte: usize,
    #[serde(skip)]
    pub end_byte: usize,
    #[serde(skip)]
    pub source_line: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Killed,
    Survived,
    NotCovered,
    CompileError,
    Timeout,
    Equivalent,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Killed => "killed",
            Self::Survived => "survived",
            Self::NotCovered => "not_covered",
            Self::CompileError => "compile_error",
            Self::Timeout => "timeout",
            Self::Equivalent => "equivalent",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MutantResult {
    #[serde(flatten)]
    pub mutant: Mutant,
    pub status: String,
    pub tests_run: Vec<String>,
    pub duration_ms: u128,
    pub cache: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tce: Option<TceResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total: usize,
    pub killed: usize,
    pub survived: usize,
    pub not_covered: usize,
    pub compile_error: usize,
    pub timeout: usize,
    pub equivalent: usize,
    pub msi: f64,
    pub threshold: Option<f64>,
    pub threshold_passed: bool,
    pub excluded_buckets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub routing_ms: u128,
    pub execution_ms: u128,
    pub cache_ms: u128,
    pub tce_ms: u128,
    pub total_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    pub requested_workers: usize,
    pub effective_workers: usize,
    pub global_cpu_budget: usize,
    pub active_sessions: usize,
    pub memory_budget_mib: Option<u64>,
    pub peak_rss_mib: u64,
    pub wait_ms: u128,
    pub throttled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingInfo {
    pub enabled: bool,
    pub backend: String,
    pub tests_discovered: usize,
    pub mapped_mutants: usize,
    pub full_suite_comparison: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub schema_version: u32,
    pub tool: ToolInfo,
    pub project: ProjectInfo,
    pub summary: Summary,
    pub mutants: Vec<MutantResult>,
    pub timing: Timing,
    pub resources: Resources,
    pub routing: RoutingInfo,
    pub cache_hits: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub path: String,
    pub manifest: String,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub project: PathBuf,
    pub manifest: Option<PathBuf>,
    pub timeout: Duration,
    pub threshold: Option<f64>,
    pub operators: Option<BTreeSet<String>>,
    pub mutant: Option<String>,
    pub mutant_ids: Option<BTreeSet<String>>,
    pub dry_run: bool,
    pub requested_workers: usize,
    pub no_cache: bool,
    pub routing: bool,
    pub incremental: bool,
    pub base_ref: Option<String>,
    pub max_memory_mib: Option<u64>,
    pub tce: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            project: PathBuf::from("."),
            manifest: None,
            timeout: Duration::from_secs(2),
            threshold: Some(80.0),
            operators: None,
            mutant: None,
            mutant_ids: None,
            dry_run: false,
            requested_workers: 1,
            no_cache: false,
            routing: true,
            incremental: false,
            base_ref: None,
            max_memory_mib: None,
            tce: true,
        }
    }
}

pub fn operator_families() -> &'static [&'static str] {
    &PUBLIC_FAMILIES
}

pub fn validate_operator_filter(filter: &BTreeSet<String>) -> Result<()> {
    for family in filter {
        if !PUBLIC_FAMILIES
            .iter()
            .any(|known| known.eq_ignore_ascii_case(family))
        {
            bail!("unknown operator family `{family}`; use --list-operators")
        }
    }
    Ok(())
}

pub fn discover(project: &Path, filter: Option<&BTreeSet<String>>) -> Result<Vec<Mutant>> {
    let project = project
        .canonicalize()
        .with_context(|| format!("cannot resolve project path {}", project.display()))?;
    let src = project.join("src");
    if !src.is_dir() {
        bail!("project {} has no src directory", project.display());
    }
    if let Some(filter) = filter {
        validate_operator_filter(filter)?;
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(&src).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|x| x == "rs") {
            files.push(entry.into_path());
        }
    }
    files.sort();
    let mut mutants = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("read Rust source {}", file.display()))?;
        validate_rust(&source, &file)?;
        let rel = slash_path(file.strip_prefix(&project).unwrap_or(&file));
        let masked = mask_non_code(source.as_bytes());
        let mut found = discover_file(&source, &masked, &rel, filter);
        mutants.append(&mut found);
    }
    mutants.sort_by(|a, b| {
        (&a.file, a.start_byte, &a.family, &a.subtype, &a.replacement).cmp(&(
            &b.file,
            b.start_byte,
            &b.family,
            &b.subtype,
            &b.replacement,
        ))
    });
    for (index, mutant) in mutants.iter_mut().enumerate() {
        mutant.id = format!("m{:04}-{}", index + 1, stable_id(mutant));
    }
    Ok(mutants)
}

fn validate_rust(source: &str, file: &Path) -> Result<()> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|e| anyhow!("configure tree-sitter-rust: {e:?}"))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("tree-sitter returned no tree for {}", file.display()))?;
    if tree.root_node().has_error() {
        bail!("Rust parse errors in {}", file.display());
    }
    Ok(())
}

fn discover_file(
    source: &str,
    masked: &[u8],
    file: &str,
    filter: Option<&BTreeSet<String>>,
) -> Vec<Mutant> {
    let bytes = masked;
    let relational_operators = relational_operator_positions(source);
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let family_and_len = if i + 1 < bytes.len()
            && ((bytes[i] == b'&' && bytes[i + 1] == b'&')
                || (bytes[i] == b'|' && bytes[i + 1] == b'|'))
        {
            Some((
                if bytes[i] == b'&' { "&&" } else { "||" },
                2usize,
                "logical",
            ))
        } else {
            None
        };
        if let Some((op, len, _)) = family_and_len {
            if op == "||" && is_closure_pipe(bytes, i) {
                i += len;
                continue;
            }
            let other = if op == "&&" { "||" } else { "&&" };
            push_if(
                &mut out,
                source,
                file,
                i,
                i + len,
                op,
                other,
                "LOR",
                "logical-swap",
                filter,
            );
            push_if(
                &mut out,
                source,
                file,
                i,
                i + len,
                op,
                "",
                "LCR",
                "connector-removal",
                filter,
            );
            push_if(
                &mut out,
                source,
                file,
                i,
                i + len,
                op,
                "",
                "COR",
                "condition-removal",
                filter,
            );
            i += len;
            continue;
        }
        if i + 1 < bytes.len() {
            let two = &bytes[i..i + 2];
            let relation = match two {
                b"<=" | b">=" | b"==" | b"!=" => Some(std::str::from_utf8(two).unwrap()),
                _ => None,
            };
            if let Some(op) = relation {
                let replacement = match op {
                    "<=" => ">",
                    ">=" => "<",
                    "==" => "!=",
                    "!=" => "==",
                    _ => unreachable!("unrecognized relational operator: {op}"),
                };
                if relational_operators.contains(&i) {
                    push_if(
                        &mut out,
                        source,
                        file,
                        i,
                        i + 2,
                        op,
                        replacement,
                        "ROR",
                        "relational-swap",
                        filter,
                    );
                }
                i += 2;
                continue;
            }
            if two == b"+=" || two == b"-=" {
                let op = std::str::from_utf8(two).unwrap();
                if rhs_is_integer(bytes, i + 2) {
                    let replacement = if op == "+=" { "-=" } else { "+=" };
                    push_if(
                        &mut out,
                        source,
                        file,
                        i,
                        i + 2,
                        op,
                        replacement,
                        "loop-inc-dec",
                        "compound-step-swap",
                        filter,
                    );
                }
                i += 2;
                continue;
            }
        }
        let one = bytes[i] as char;
        if matches!(one, '+' | '-' | '*' | '/' | '%') && arithmetic_position(bytes, i) {
            let op = one.to_string();
            if one == '+'
                && let Some((start, end, replacement)) = commutative_add_mutation(source, bytes, i)
            {
                let original = &source[start..end];
                push_if(
                    &mut out,
                    source,
                    file,
                    start,
                    end,
                    original,
                    &replacement,
                    "AOR",
                    "commutative-swap",
                    filter,
                );
                i += 1;
                continue;
            }
            let next = match one {
                '+' => "-",
                '-' => "+",
                '*' => "/",
                '/' => "*",
                '%' => "*",
                _ => "+",
            };
            push_if(
                &mut out,
                source,
                file,
                i,
                i + 1,
                &op,
                next,
                "AOR",
                "arithmetic-swap",
                filter,
            );
            push_if(
                &mut out,
                source,
                file,
                i,
                i + 1,
                &op,
                "",
                "AOD",
                "arithmetic-deletion",
                filter,
            );
            push_if(
                &mut out,
                source,
                file,
                i,
                i + 1,
                &op,
                &format!("{op} 1 {op}"),
                "AOI",
                "arithmetic-insertion",
                filter,
            );
            i += 1;
            continue;
        }
        if matches!(one, '<' | '>')
            && relational_operators.contains(&i)
            && relational_position(bytes, i)
        {
            let op = one.to_string();
            let replacement = if one == '<' { ">" } else { "<" };
            push_if(
                &mut out,
                source,
                file,
                i,
                i + 1,
                &op,
                replacement,
                "ROR",
                "relational-swap",
                filter,
            );
            i += 1;
            continue;
        }
        i += 1;
    }
    discover_line_mutants(source, file, filter, &mut out);
    discover_idiomatic_mutants(source, masked, file, filter, &mut out);
    out
}

/// Discover the Rust-idiomatic M2 operators with byte-stable, syntax-first
/// edits. The masked buffer has the same byte length as `source`, so every
/// range can be checked against the original source by `push_if`. Semantic
/// invalidity is deliberately left to Cargo and is reported as
/// `compile_error`, rather than making the scanner invent type information.
fn discover_idiomatic_mutants(
    source: &str,
    masked: &[u8],
    file: &str,
    filter: Option<&BTreeSet<String>>,
    out: &mut Vec<Mutant>,
) {
    let bytes = masked;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'?' && (i == 0 || bytes[i - 1] != b'?') {
            push_if(
                out,
                source,
                file,
                i,
                i + 1,
                "?",
                "",
                "question-mark-removal",
                "try-suffix-removal",
                filter,
            );
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b".unwrap()") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".unwrap()".len(),
                ".unwrap()",
                "",
                "unwrap-expect-removal",
                "unwrap-removal",
                filter,
            );
            i += b".unwrap()".len();
            continue;
        }
        if bytes[i..].starts_with(b".expect(") {
            let mut end = i + b".expect(".len();
            let mut depth = 1usize;
            while end < bytes.len() {
                match bytes[end] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                end += 1;
            }
            if depth == 0 {
                push_if(
                    out,
                    source,
                    file,
                    i,
                    end,
                    &source[i..end],
                    "",
                    "unwrap-expect-removal",
                    "expect-removal",
                    filter,
                );
                i = end;
                continue;
            }
        }
        if bytes[i..].starts_with(b".await") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".await".len(),
                ".await",
                "",
                "await-removal",
                "await-suffix-removal",
                filter,
            );
            i += b".await".len();
            continue;
        }
        if bytes[i..].starts_with(b"move")
            && (i == 0 || !is_identifier_byte(bytes[i - 1]))
            && (i + 4 == bytes.len() || !is_identifier_byte(bytes[i + 4]))
        {
            let mut end = i + 4;
            while end < bytes.len() && bytes[end].is_ascii_whitespace() {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'|' {
                push_if(
                    out,
                    source,
                    file,
                    i,
                    i + 4,
                    "move",
                    "",
                    "move-closure-removal",
                    "closure-move-removal",
                    filter,
                );
                i += 4;
                continue;
            }
        }
        if bytes[i..].starts_with(b"&mut")
            && (i + 4 == bytes.len() || !is_identifier_byte(bytes[i + 4]))
        {
            push_if(
                out,
                source,
                file,
                i,
                i + 4,
                "&mut",
                "&",
                "mut-to-shared",
                "mutable-reference-removal",
                filter,
            );
            i += 4;
            continue;
        }
        if bytes[i..].starts_with(b".clone()") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".clone()".len(),
                ".clone()",
                "",
                "clone-removal",
                "clone-call-removal",
                filter,
            );
            i += b".clone()".len();
            continue;
        }
        if bytes[i..].starts_with(b"std::sync::Arc") {
            push_if(
                out,
                source,
                file,
                i,
                i + b"std::sync::Arc".len(),
                "std::sync::Arc",
                "std::rc::Rc",
                "arc-rc-swap",
                "arc-to-rc",
                filter,
            );
            i += b"std::sync::Arc".len();
            continue;
        }
        if bytes[i..].starts_with(b"std::rc::Rc") {
            push_if(
                out,
                source,
                file,
                i,
                i + b"std::rc::Rc".len(),
                "std::rc::Rc",
                "std::sync::Arc",
                "arc-rc-swap",
                "rc-to-arc",
                filter,
            );
            i += b"std::rc::Rc".len();
            continue;
        }
        if bytes[i..].starts_with(b".map(") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".map".len(),
                ".map",
                ".filter",
                "iterator-chain",
                "map-to-filter",
                filter,
            );
            i += b".map".len();
            continue;
        }
        if bytes[i..].starts_with(b".filter(") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".filter".len(),
                ".filter",
                ".map",
                "iterator-chain",
                "filter-to-map",
                filter,
            );
            i += b".filter".len();
            continue;
        }
        if bytes[i..].starts_with(b".collect::<Vec<_>>()") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".collect::<Vec<_>>()".len(),
                ".collect::<Vec<_>>()",
                ".count()",
                "iterator-chain",
                "collect-to-count",
                filter,
            );
            i += b".collect::<Vec<_>>()".len();
            continue;
        }
        if bytes[i..].starts_with(b".collect()") {
            push_if(
                out,
                source,
                file,
                i,
                i + b".collect()".len(),
                ".collect()",
                ".count()",
                "iterator-chain",
                "collect-to-count",
                filter,
            );
            i += b".collect()".len();
            continue;
        }
        i += 1;
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn discover_line_mutants(
    source: &str,
    file: &str,
    filter: Option<&BTreeSet<String>>,
    out: &mut Vec<Mutant>,
) {
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let code_content = content.split("//").next().unwrap_or(content).trim_end();
        let trimmed = code_content.trim();
        if trimmed.contains("return ")
            && trimmed.ends_with(';')
            && let Some(pos) = content.find("return ")
        {
            let expr_start = pos + "return ".len();
            let expr_end = content.rfind(';').unwrap_or(content.len());
            let expr = &content[expr_start..expr_end];
            let replacement = if expr.contains(['<', '>', '=', '&', '|']) {
                "false"
            } else {
                ""
            };
            push_if(
                out,
                source,
                file,
                offset + expr_start,
                offset + expr_end,
                expr,
                replacement,
                "RVR",
                "return-value",
                filter,
            );
        }
        if trimmed.ends_with(';')
            && !trimmed.starts_with("let ")
            && !trimmed.starts_with("return ")
            && !trimmed.starts_with("while ")
            && !trimmed.starts_with("for ")
            && !trimmed.starts_with("if ")
            && !trimmed.starts_with("fn ")
            && (trimmed.contains('!') || trimmed.ends_with("();"))
        {
            let start = content.find(trimmed).unwrap_or(0);
            push_if(
                out,
                source,
                file,
                offset + start,
                offset + start + trimmed.len(),
                trimmed,
                ";",
                "SDL",
                "statement-deletion",
                filter,
            );
        }
        offset += line.len();
    }
}

#[allow(clippy::too_many_arguments)]
fn push_if(
    out: &mut Vec<Mutant>,
    source: &str,
    file: &str,
    start: usize,
    end: usize,
    original: &str,
    replacement: &str,
    family: &str,
    subtype: &str,
    filter: Option<&BTreeSet<String>>,
) {
    let enabled = filter.is_none_or(|set| set.iter().any(|f| f.eq_ignore_ascii_case(family)));
    if !enabled
        || original == replacement
        || start > end
        || end > source.len()
        || !source.is_char_boundary(start)
        || !source.is_char_boundary(end)
    {
        return;
    }
    let (line, column) = line_col(source, start);
    let line_start = source[..start].rfind('\n').map_or(0, |n| n + 1);
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |n| start + n);
    let source_line = source[line_start..line_end].to_string();
    let actual = &source[start..end];
    if actual != original {
        return;
    }
    let mut hash_material = String::new();
    hash_material.push_str(file);
    hash_material.push('|');
    hash_material.push_str(&start.to_string());
    hash_material.push('|');
    hash_material.push_str(family);
    hash_material.push('|');
    hash_material.push_str(subtype);
    hash_material.push('|');
    hash_material.push_str(replacement);
    out.push(Mutant {
        id: String::new(),
        file: file.to_string(),
        line,
        column,
        family: family.to_string(),
        subtype: subtype.to_string(),
        original: original.to_string(),
        replacement: replacement.to_string(),
        start_byte: start,
        end_byte: end,
        source_line,
    });
}

fn line_col(source: &str, byte: usize) -> (usize, usize) {
    let line = source[..byte].bytes().filter(|b| *b == b'\n').count() + 1;
    let start = source[..byte].rfind('\n').map_or(0, |n| n + 1);
    (line, source[start..byte].chars().count() + 1)
}

fn arithmetic_position(bytes: &[u8], i: usize) -> bool {
    if (bytes[i] == b'+' || bytes[i] == b'-') && i + 1 < bytes.len() && bytes[i + 1] == bytes[i] {
        return false;
    }
    if bytes[i] == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'>' {
        return false;
    }
    let prev = previous_non_space(bytes, i);
    let next = next_non_space(bytes, i + 1);
    let prev_expr =
        prev.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b')' || b == b']');
    let next_expr =
        next.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'(' || b == b'[');
    prev_expr && next_expr
}

fn commutative_add_mutation(
    source: &str,
    bytes: &[u8],
    operator: usize,
) -> Option<(usize, usize, String)> {
    let line_start = source[..operator].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[operator..]
        .find('\n')
        .map_or(source.len(), |index| operator + index);
    if !source[line_start..line_end].contains("rust-mutant:commutative") {
        return None;
    }

    let mut left_end = operator;
    while left_end > line_start && bytes[left_end - 1].is_ascii_whitespace() {
        left_end -= 1;
    }
    let mut left_start = left_end;
    while left_start > line_start
        && (bytes[left_start - 1].is_ascii_alphanumeric() || bytes[left_start - 1] == b'_')
    {
        left_start -= 1;
    }
    let mut right_start = operator + 1;
    while right_start < line_end && bytes[right_start].is_ascii_whitespace() {
        right_start += 1;
    }
    let mut right_end = right_start;
    while right_end < line_end
        && (bytes[right_end].is_ascii_alphanumeric() || bytes[right_end] == b'_')
    {
        right_end += 1;
    }
    if left_start == left_end || right_start == right_end {
        return None;
    }
    let left = &source[left_start..left_end];
    let right = &source[right_start..right_end];
    Some((left_start, right_end, format!("{right} + {left}")))
}

fn relational_position(bytes: &[u8], i: usize) -> bool {
    if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
        return false;
    }
    if i > 0 && bytes[i - 1] == b'=' {
        return false;
    }
    if i > 0 && bytes[i - 1] == b':' {
        return false;
    }
    if i + 2 < bytes.len() && bytes[i] == b'<' && bytes[i + 1] == b'_' && bytes[i + 2] == b'>' {
        return false;
    }
    let prev = previous_non_space(bytes, i);
    let next = next_non_space(bytes, i + 1);
    prev.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b')')
        && next.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn relational_operator_positions(source: &str) -> HashSet<usize> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("tree-sitter-rust language should load");
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };

    let mut positions = HashSet::new();
    collect_relational_operator_positions(tree.root_node(), &mut positions);
    positions
}

fn collect_relational_operator_positions(
    node: tree_sitter::Node<'_>,
    positions: &mut HashSet<usize>,
) {
    if node.kind() == "binary_expression"
        && let Some(operator) = node.child_by_field_name("operator")
        && matches!(operator.kind(), "<" | ">" | "<=" | ">=" | "==" | "!=")
    {
        positions.insert(operator.start_byte());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_relational_operator_positions(child, positions);
    }
}

fn rhs_is_integer(bytes: &[u8], mut i: usize) -> bool {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i < bytes.len() && bytes[i].is_ascii_digit()
}

fn previous_non_space(bytes: &[u8], mut i: usize) -> Option<u8> {
    while i > 0 {
        i -= 1;
        if !bytes[i].is_ascii_whitespace() {
            return Some(bytes[i]);
        }
    }
    None
}

fn next_non_space(bytes: &[u8], mut i: usize) -> Option<u8> {
    while i < bytes.len() {
        if !bytes[i].is_ascii_whitespace() {
            return Some(bytes[i]);
        }
        i += 1;
    }
    None
}

fn mask_non_code(input: &[u8]) -> Vec<u8> {
    let mut out = input.to_vec();
    let mut i = 0;
    while i < input.len() {
        if i + 1 < input.len() && input[i] == b'/' && input[i + 1] == b'/' {
            let start = i;
            i += 2;
            while i < input.len() && input[i] != b'\n' {
                i += 1;
            }
            for byte in &mut out[start..i] {
                *byte = b' ';
            }
            continue;
        }
        if i + 1 < input.len() && input[i] == b'/' && input[i + 1] == b'*' {
            let start = i;
            i += 2;
            while i + 1 < input.len() && !(input[i] == b'*' && input[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(input.len());
            for (index, byte) in out[start..i].iter_mut().enumerate() {
                if input[start + index] != b'\n' {
                    *byte = b' ';
                }
            }
            continue;
        }
        if input[i] == b'\'' {
            let mut cursor = i + 1;
            if cursor < input.len()
                && (input[cursor].is_ascii_alphabetic() || input[cursor] == b'_')
            {
                while cursor < input.len()
                    && (input[cursor].is_ascii_alphanumeric() || input[cursor] == b'_')
                {
                    cursor += 1;
                }
                if cursor == input.len() || input[cursor] != b'\'' {
                    i += 1;
                    continue;
                }
            }
        }
        if input[i] == b'"' || input[i] == b'\'' {
            let quote = input[i];
            let start = i;
            i += 1;
            while i < input.len() {
                if input[i] == b'\\' {
                    i = (i + 2).min(input.len());
                    continue;
                }
                let done = input[i] == quote;
                i += 1;
                if done {
                    break;
                }
            }
            for (index, byte) in out[start..i].iter_mut().enumerate() {
                if input[start + index] != b'\n' {
                    *byte = b' ';
                }
            }
            continue;
        }
        i += 1;
    }
    out
}

fn is_closure_pipe(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index;
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }
    if cursor == 0 {
        return true;
    }
    let previous = bytes[cursor - 1];
    if matches!(previous, b'=' | b'(' | b'[' | b'{' | b',' | b':') {
        return true;
    }
    let mut word_start = cursor;
    while word_start > 0
        && (bytes[word_start - 1].is_ascii_alphanumeric() || bytes[word_start - 1] == b'_')
    {
        word_start -= 1;
    }
    &bytes[word_start..cursor] == b"move"
}

fn stable_id(m: &Mutant) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in format!(
        "{}:{}:{}:{}:{}",
        m.file, m.line, m.family, m.subtype, m.replacement
    )
    .bytes()
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn mutant_id_matches(discovered: &str, requested: &str) -> bool {
    discovered == requested
        || discovered.trim_start_matches('m').trim_start_matches('0') == requested
}

fn select_mutants(
    mut mutants: Vec<Mutant>,
    mutant: Option<&str>,
    mutant_ids: Option<&BTreeSet<String>>,
) -> Result<Vec<Mutant>> {
    if mutant.is_some() && mutant_ids.is_some() {
        bail!("single and multi-mutant selectors are mutually exclusive");
    }
    if let Some(ids) = mutant_ids {
        let missing = ids
            .iter()
            .filter(|requested| {
                !mutants
                    .iter()
                    .any(|candidate| mutant_id_matches(&candidate.id, requested))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!(
                "requested mutant IDs were not discovered: {}",
                missing.join(", ")
            );
        }
        mutants.retain(|candidate| {
            ids.iter()
                .any(|requested| mutant_id_matches(&candidate.id, requested))
        });
    } else if let Some(requested) = mutant {
        mutants.retain(|candidate| mutant_id_matches(&candidate.id, requested));
    }
    Ok(mutants)
}

pub fn run(options: &RunOptions) -> Result<Report> {
    let started = Instant::now();
    let session = GlobalSession::acquire()?;
    let project = options
        .project
        .canonicalize()
        .with_context(|| format!("project path does not exist: {}", options.project.display()))?;
    if !project.is_dir() {
        bail!("project path is not a directory: {}", project.display());
    }
    let _scratch_cleanup = ScratchCleanupGuard::new(&project)?;
    let manifest = options
        .manifest
        .clone()
        .unwrap_or_else(|| project.join("Cargo.toml"));
    let manifest = if manifest.is_absolute() {
        manifest
    } else {
        std::env::current_dir()?.join(manifest)
    };
    if !manifest.is_file() {
        bail!("Cargo manifest does not exist: {}", manifest.display());
    }
    let discovery_started = Instant::now();
    let mutants = select_mutants(
        discover(&project, options.operators.as_ref())?,
        options.mutant.as_deref(),
        options.mutant_ids.as_ref(),
    )?;
    if options.dry_run {
        return Ok(report_for_discovery(
            &project,
            &manifest,
            mutants,
            discovery_started.elapsed().as_millis(),
            started.elapsed().as_millis(),
            options,
        ));
    }
    if mutants.is_empty() {
        return Ok(report_for_discovery(
            &project,
            &manifest,
            mutants,
            discovery_started.elapsed().as_millis(),
            started.elapsed().as_millis(),
            options,
        ));
    }
    let target_dir = external_target_dir(&project);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)
            .with_context(|| format!("clean mutation target {}", target_dir.display()))?;
    }
    let capacity = effective_cpu_capacity();
    let global_cpu_budget = (capacity.saturating_mul(3) / 4).max(1);
    let effective_workers = options.requested_workers.min(global_cpu_budget).max(1);
    let memory_budget = memory_budget(options.max_memory_mib);
    let baseline_target_dir = if effective_workers == 1 {
        target_dir.join("worker-0")
    } else {
        target_dir.clone()
    };
    let baseline = cargo_test(
        &project,
        &manifest,
        &baseline_target_dir,
        options.timeout.max(Duration::from_secs(10)),
        None,
    )?;
    if baseline.timed_out || baseline.code != Some(0) {
        bail!(
            "baseline cargo test failed (exit {:?})\n{}",
            baseline.code,
            truncate(&baseline.stderr, 4000)
        );
    }
    let mut routing = CoverageMap::disabled();
    if options.routing {
        routing = build_coverage_map(&project, &manifest)?;
    }
    let changed = if options.incremental {
        Some(changed_source_files(
            &project,
            options.base_ref.as_deref().unwrap_or("HEAD~1"),
        )?)
    } else {
        None
    };
    let routing_ms = discovery_started.elapsed().as_millis();
    let adaptive = options.timeout == Duration::from_secs(2);
    let mutant_timeout = if adaptive {
        Duration::from_millis(
            (baseline.duration_ms.saturating_mul(3) + 5000)
                .max(5000)
                .min(u64::MAX as u128) as u64,
        )
    } else {
        options.timeout
    };
    let rss_before = current_rss_mib();
    PEAK_RSS_MIB.store(rss_before, Ordering::Relaxed);
    let throttled_before = memory_budget.is_some_and(|budget| rss_before > budget);
    let cache = CacheStore::new(&project);
    let cache_hits = AtomicUsize::new(0);
    let memory_wait_ms = AtomicU64::new(0);
    let execution_started = Instant::now();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(effective_workers)
        .thread_name(|index| format!("rust-mutant-worker-{index}"))
        .build()?;
    let results: Result<Vec<MutantResult>> = pool.install(|| {
        mutants
            .into_par_iter()
            .map(|mutant| {
                let waited = wait_for_memory(memory_budget);
                memory_wait_ms.fetch_add(waited, Ordering::Relaxed);
                let selected = if options.routing {
                    routing.tests_for(&mutant.file, mutant.line)
                } else {
                    Vec::new()
                };
                execute_one(
                    &project,
                    &manifest,
                    &target_dir,
                    mutant,
                    &selected,
                    mutant_timeout,
                    options,
                    &cache,
                    &cache_hits,
                    changed.as_ref(),
                )
            })
            .collect()
    });
    let mut results = results?;
    results.sort_by(|a, b| a.mutant.id.cmp(&b.mutant.id));
    let execution_ms = execution_started.elapsed().as_millis();
    let tce_ms = results
        .iter()
        .filter_map(|result| result.tce.as_ref())
        .map(|tce| tce.duration_ms)
        .sum();
    let summary = summarize(&results, options.threshold);
    let total_ms = started.elapsed().as_millis();
    let rss = current_rss_mib();
    let peak_rss = PEAK_RSS_MIB.load(Ordering::Relaxed).max(rss);
    let throttled = throttled_before || memory_budget.is_some_and(|budget| rss > budget);
    let wait_ms = session.wait_ms + u128::from(memory_wait_ms.load(Ordering::Relaxed));
    drop(session);
    Ok(Report {
        schema_version: SCHEMA_VERSION,
        tool: ToolInfo {
            name: "rust-mutant",
            version: env!("CARGO_PKG_VERSION"),
        },
        project: ProjectInfo {
            path: slash_path(&project),
            manifest: slash_path(&manifest),
        },
        summary,
        mutants: results,
        timing: Timing {
            routing_ms,
            execution_ms,
            cache_ms: cache.cache_ms(),
            tce_ms,
            total_ms,
        },
        resources: Resources {
            requested_workers: options.requested_workers,
            effective_workers,
            global_cpu_budget,
            active_sessions: 1,
            memory_budget_mib: memory_budget,
            peak_rss_mib: peak_rss,
            wait_ms,
            throttled,
        },
        routing: RoutingInfo {
            enabled: options.routing,
            backend: routing.backend,
            tests_discovered: routing.all.len(),
            mapped_mutants: routing.mapped,
            full_suite_comparison: !options.routing,
        },
        cache_hits: cache_hits.load(Ordering::Relaxed),
    })
}

fn report_for_discovery(
    project: &Path,
    manifest: &Path,
    mutants: Vec<Mutant>,
    routing_ms: u128,
    total_ms: u128,
    options: &RunOptions,
) -> Report {
    let results = mutants
        .into_iter()
        .map(|mutant| MutantResult {
            mutant,
            status: "discovered".to_string(),
            tests_run: Vec::new(),
            duration_ms: 0,
            cache: "miss".to_string(),
            command: None,
            details: None,
            tce: None,
        })
        .collect::<Vec<_>>();
    Report {
        schema_version: SCHEMA_VERSION,
        tool: ToolInfo {
            name: "rust-mutant",
            version: env!("CARGO_PKG_VERSION"),
        },
        project: ProjectInfo {
            path: slash_path(project),
            manifest: slash_path(manifest),
        },
        summary: Summary {
            total: results.len(),
            killed: 0,
            survived: 0,
            not_covered: 0,
            compile_error: 0,
            timeout: 0,
            equivalent: 0,
            msi: 0.0,
            threshold: options.threshold,
            threshold_passed: true,
            excluded_buckets: vec![
                "not_covered".into(),
                "compile_error".into(),
                "timeout".into(),
                "equivalent".into(),
            ],
        },
        mutants: results,
        timing: Timing {
            routing_ms,
            execution_ms: 0,
            cache_ms: 0,
            tce_ms: 0,
            total_ms,
        },
        resources: Resources {
            requested_workers: options.requested_workers,
            effective_workers: 1,
            global_cpu_budget: 1,
            active_sessions: 1,
            memory_budget_mib: None,
            peak_rss_mib: 0,
            wait_ms: 0,
            throttled: false,
        },
        routing: RoutingInfo {
            enabled: options.routing,
            backend: "disabled".into(),
            tests_discovered: 0,
            mapped_mutants: 0,
            full_suite_comparison: !options.routing,
        },
        cache_hits: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_one(
    project: &Path,
    manifest: &Path,
    target_dir: &Path,
    mutant: Mutant,
    selected: &[TestCase],
    timeout: Duration,
    options: &RunOptions,
    cache: &CacheStore,
    cache_hits: &AtomicUsize,
    changed: Option<&BTreeSet<String>>,
) -> Result<MutantResult> {
    let started = Instant::now();
    let key = cache.key(project, &mutant, selected, options, timeout)?;
    let cached = if options.no_cache {
        None
    } else {
        cache.load(&key)?
    };
    if let Some(cached) = cached {
        cache_hits.fetch_add(1, Ordering::Relaxed);
        return Ok(cached.into_result(mutant, "hit"));
    }
    if changed.is_some_and(|files| !files.contains(&mutant.file)) {
        bail!(
            "incremental cache miss for unchanged source file {}",
            mutant.file
        );
    }
    let line_directive = mutant.source_line.to_ascii_lowercase();
    if line_directive.contains("rust-mutant: not-covered") {
        let result = MutantResult {
            mutant,
            status: Status::NotCovered.as_str().into(),
            tests_run: Vec::new(),
            duration_ms: started.elapsed().as_millis(),
            cache: "miss".into(),
            command: None,
            details: Some("source line is explicitly outside the fixture coverage probe".into()),
            tce: None,
        };
        if !options.no_cache {
            cache.store(&key, &result)?;
        }
        return Ok(result);
    }
    let worker = rayon::current_thread_index().unwrap_or(0);
    let scratch = worker_scratch_dir(project, worker);
    if !scratch.exists() {
        copy_project(project, &scratch)?;
    }
    let target_file = scratch.join(&mutant.file);
    restore_source_file(project, &scratch, &mutant.file)?;
    let restore_guard = SourceRestoreGuard::new(project.join(&mutant.file), target_file.clone());
    let mut source = fs::read_to_string(&target_file)
        .with_context(|| format!("read scratch source {}", target_file.display()))?;
    if mutant.end_byte > source.len()
        || !source.is_char_boundary(mutant.start_byte)
        || !source.is_char_boundary(mutant.end_byte)
        || source.get(mutant.start_byte..mutant.end_byte) != Some(mutant.original.as_str())
    {
        bail!(
            "mutant {} no longer matches {}:{}",
            mutant.id,
            mutant.file,
            mutant.line
        );
    }
    source.replace_range(mutant.start_byte..mutant.end_byte, &mutant.replacement);
    fs::write(&target_file, source)?;
    let mutant_target_dir = target_dir.join(format!(
        "worker-{}",
        rayon::current_thread_index().unwrap_or(0)
    ));
    let scratch_manifest = scratch.join(manifest.file_name().unwrap_or_default());
    let cases = if selected.is_empty() {
        vec![None]
    } else {
        selected.iter().map(Some).collect::<Vec<_>>()
    };
    let mut tests_run = Vec::new();
    let mut outputs = Vec::new();
    let mut final_status = Status::Survived;
    let mut command_text = String::new();
    for case in cases {
        let command = match case {
            Some(test) => {
                tests_run.push(test.label.clone());
                cargo_nextest(
                    &scratch,
                    &scratch_manifest,
                    &mutant_target_dir,
                    timeout,
                    Some(test),
                )?
            }
            None => {
                tests_run.push("full-suite".into());
                cargo_nextest(
                    &scratch,
                    &scratch_manifest,
                    &mutant_target_dir,
                    timeout,
                    None,
                )?
            }
        };
        command_text = command.command.clone();
        outputs.push(command.output());
        final_status = classify_command(&command);
        if final_status != Status::Survived {
            break;
        }
    }
    let mut tce = None;
    if final_status == Status::Survived && options.tce {
        let tce_target = std::env::temp_dir().join("rust-mutant-tce").join(format!(
            "{:016x}-{}-{}",
            stable_path_hash(project),
            mutant.id,
            std::process::id()
        ));
        let tce_started = Instant::now();
        let expected_function = expected_function_name(project, &mutant);
        let result = match rust_mutant_tce::compare(
            project,
            manifest,
            &scratch,
            &scratch_manifest,
            &tce_target,
            expected_function.as_deref(),
        ) {
            Ok(result) => result,
            Err(error) => TceResult::error(error.to_string(), tce_started.elapsed().as_millis()),
        };
        if result.equivalent {
            final_status = Status::Equivalent;
        }
        tce = Some(result);
    }
    restore_guard.restore()?;
    let output = outputs.join("\n");
    let detail = match final_status {
        Status::Killed => Some(truncate(&stable_diagnostic(&output), 2000)),
        Status::CompileError => Some(truncate(&stable_diagnostic(&output), 3000)),
        Status::Timeout => Some(format!(
            "nextest exceeded adaptive timeout of {} ms",
            timeout.as_millis()
        )),
        _ => None,
    };
    let result = MutantResult {
        mutant,
        status: final_status.as_str().into(),
        tests_run,
        duration_ms: started.elapsed().as_millis(),
        cache: "miss".into(),
        command: Some(command_text),
        details: detail,
        tce,
    };
    if !options.no_cache {
        cache.store(&key, &result)?;
    }
    Ok(result)
}

#[derive(Debug)]
struct GlobalSession {
    path: PathBuf,
    wait_ms: u128,
}

impl GlobalSession {
    fn acquire() -> Result<Self> {
        let path = std::env::temp_dir().join("rust-mutant-global-session.lock");
        let started = Instant::now();
        loop {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    writeln!(file, "{}", std::process::id())?;
                    return Ok(Self {
                        path,
                        wait_ms: started.elapsed().as_millis(),
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let stale = fs::read_to_string(&path)
                        .ok()
                        .and_then(|value| value.trim().parse::<u32>().ok())
                        .is_some_and(|pid| !process_alive(pid));
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    thread::sleep(Duration::from_millis(25));
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(windows)]
    {
        let pid_text = pid.to_string();
        let Ok(output) = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid_text}")])
            .output()
        else {
            return true;
        };
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.split_whitespace().nth(1) == Some(pid_text.as_str()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        true
    }
}

impl Drop for GlobalSession {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedOutcome {
    status: String,
    tests_run: Vec<String>,
    duration_ms: u128,
    command: Option<String>,
    details: Option<String>,
    tce: Option<TceResult>,
}

impl CachedOutcome {
    fn from_result(result: &MutantResult) -> Self {
        Self {
            status: result.status.clone(),
            tests_run: result.tests_run.clone(),
            duration_ms: result.duration_ms,
            command: result.command.clone(),
            details: result.details.clone(),
            tce: result.tce.clone(),
        }
    }

    fn into_result(self, mutant: Mutant, cache: &str) -> MutantResult {
        MutantResult {
            mutant,
            status: self.status,
            tests_run: self.tests_run,
            duration_ms: 0,
            cache: cache.into(),
            command: self.command,
            details: self.details.map(|details| stable_diagnostic(&details)),
            tce: self.tce,
        }
    }
}

#[derive(Debug)]
struct CacheStore {
    dir: PathBuf,
    cache_ms: AtomicU64,
}

impl CacheStore {
    fn new(project: &Path) -> Self {
        Self {
            dir: std::env::temp_dir()
                .join("rust-mutant-cache")
                .join(format!("{:016x}", stable_path_hash(project))),
            cache_ms: AtomicU64::new(0),
        }
    }

    fn key(
        &self,
        project: &Path,
        mutant: &Mutant,
        tests: &[TestCase],
        options: &RunOptions,
        timeout: Duration,
    ) -> Result<String> {
        let timeout_key = if options.timeout == Duration::from_secs(2) {
            "adaptive".into()
        } else {
            timeout.as_millis().to_string()
        };
        let mut value = format!(
            "cacheSchema={CACHE_SCHEMA_VERSION};engine={};toolchain={};family={};id={};route={};tce={};timeout={timeout_key};",
            env!("CARGO_PKG_VERSION"),
            toolchain_identity(),
            mutant.family,
            mutant.id,
            options.routing,
            options.tce
        );
        value.push_str(&hash_file(&project.join(&mutant.file))?);
        value.push_str(&hash_file(&project.join("Cargo.toml"))?);
        let lockfile = project.join("Cargo.lock");
        if lockfile.is_file() {
            value.push_str(&hash_file(&lockfile)?);
        }
        for entry in WalkDir::new(project.join("tests"))
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() {
                value.push_str(&hash_file(entry.path())?);
            }
        }
        for test in tests {
            value.push_str(&test.label);
        }
        Ok(format!("{:016x}", string_hash(&value)))
    }

    fn load(&self, key: &str) -> Result<Option<CachedOutcome>> {
        let started = Instant::now();
        let path = self.dir.join(format!("{key}.json"));
        let result = if !path.is_file() {
            None
        } else {
            Some(serde_json::from_slice(&fs::read(path)?)?)
        };
        self.cache_ms
            .fetch_add(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(result)
    }

    fn store(&self, key: &str, result: &MutantResult) -> Result<()> {
        let started = Instant::now();
        fs::create_dir_all(&self.dir)?;
        let path = self.dir.join(format!("{key}.json"));
        let temp = self.dir.join(format!(".{key}.tmp-{}", std::process::id()));
        fs::write(
            &temp,
            serde_json::to_vec(&CachedOutcome::from_result(result))?,
        )?;
        fs::rename(temp, path)?;
        self.cache_ms
            .fetch_add(started.elapsed().as_millis() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn cache_ms(&self) -> u128 {
        u128::from(self.cache_ms.load(Ordering::Relaxed))
    }
}

fn hash_file(path: &Path) -> Result<String> {
    Ok(format!(
        "{:016x}",
        string_hash(&String::from_utf8_lossy(&fs::read(path)?))
    ))
}

fn string_hash(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn toolchain_identity() -> &'static str {
    static IDENTITY: OnceLock<String> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let rustc = Command::new("rustc")
            .arg("-vV")
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_else(|| "rustc-unavailable".into());
        let nextest = Command::new("cargo")
            .args(["nextest", "--version"])
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_else(|| "cargo-nextest-unavailable".into());
        format!("{rustc}|{nextest}")
    })
}

fn changed_source_files(project: &Path, base_ref: &str) -> Result<BTreeSet<String>> {
    let output = Command::new("git")
        .current_dir(project)
        .arg("diff")
        .arg("--name-only")
        .arg(base_ref)
        .arg("--")
        .arg("src")
        .output()?;
    if !output.status.success() {
        bail!("cannot resolve incremental base ref `{base_ref}`");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let line = line.trim().replace('\\', "/");
            line.find("src/")
                .map_or(line.clone(), |index| line[index..].into())
        })
        .filter(|line| !line.is_empty())
        .collect())
}

fn effective_cpu_capacity() -> usize {
    let affinity = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Cpus_allowed_list:\t"))
                .map(|value| {
                    value
                        .split(',')
                        .map(|part| {
                            let mut range = part.split('-');
                            let start = range
                                .next()
                                .and_then(|x| x.parse::<usize>().ok())
                                .unwrap_or(0);
                            let end = range
                                .next()
                                .and_then(|x| x.parse::<usize>().ok())
                                .unwrap_or(start);
                            end.saturating_sub(start) + 1
                        })
                        .sum::<usize>()
                })
                .filter(|count| *count > 0)
        });
    let quota = fs::read_to_string("/sys/fs/cgroup/cpu.max")
        .ok()
        .and_then(|value| {
            let mut fields = value.split_whitespace();
            let quota = fields.next()?;
            let period = fields.next()?.parse::<u64>().ok()?;
            if quota == "max" || period == 0 {
                return None;
            }
            let quota = quota.parse::<u64>().ok()?;
            Some((quota / period).max(1) as usize)
        });
    affinity
        .into_iter()
        .chain(quota)
        .min()
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(|value| value.get())
        })
        .unwrap_or(1)
}

fn memory_budget(requested: Option<u64>) -> Option<u64> {
    let total = fs::read_to_string("/proc/meminfo")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
        })
        .map(|kib| kib / 1024)?;
    let ceiling = total.saturating_mul(75) / 100;
    Some(requested.map_or(ceiling, |value| value.min(ceiling)))
}

fn wait_for_memory(budget: Option<u64>) -> u64 {
    let Some(limit) = budget else {
        record_peak_rss();
        return 0;
    };
    if current_rss_mib() <= limit {
        record_peak_rss();
        return 0;
    }
    let started = Instant::now();
    while current_rss_mib() > limit && started.elapsed() < Duration::from_millis(250) {
        record_peak_rss();
        thread::sleep(Duration::from_millis(25));
    }
    record_peak_rss();
    started.elapsed().as_millis() as u64
}

fn record_peak_rss() {
    let rss = current_rss_mib();
    PEAK_RSS_MIB.fetch_max(rss, Ordering::Relaxed);
}

fn current_rss_mib() -> u64 {
    let root = std::process::id();
    let mut processes = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let Ok(status) = fs::read_to_string(entry.path().join("status")) else {
            continue;
        };
        let mut parent = None;
        let mut rss_kib = None;
        for line in status.lines() {
            if let Some(value) = line.strip_prefix("PPid:") {
                parent = value.trim().parse::<u32>().ok();
            } else if let Some(value) = line.strip_prefix("VmRSS:") {
                rss_kib = value.split_whitespace().next().and_then(|v| v.parse().ok());
            }
        }
        if let (Some(parent), Some(rss_kib)) = (parent, rss_kib) {
            processes.insert(pid, (parent, rss_kib));
        }
    }
    let mut total_kib = 0u64;
    for (&pid, &(_, rss_kib)) in &processes {
        let mut current = pid;
        let mut visited = BTreeSet::new();
        loop {
            if current == root {
                total_kib = total_kib.saturating_add(rss_kib);
                break;
            }
            if !visited.insert(current) {
                break;
            }
            let Some(&(parent, _)) = processes.get(&current) else {
                break;
            };
            if parent == 0 || parent == current {
                break;
            }
            current = parent;
        }
    }
    (total_kib / 1024).max(1)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TestCase {
    binary: String,
    name: String,
    label: String,
}

#[derive(Debug, Clone)]
struct CoverageMap {
    by_line: BTreeMap<(String, usize), Vec<TestCase>>,
    all: Vec<TestCase>,
    backend: String,
    mapped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoverageCache {
    entries: Vec<CoverageEntry>,
    all: Vec<TestCase>,
    backend: String,
    mapped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoverageEntry {
    file: String,
    line: usize,
    tests: Vec<TestCase>,
}

impl From<CoverageCache> for CoverageMap {
    fn from(cache: CoverageCache) -> Self {
        let by_line = cache
            .entries
            .into_iter()
            .map(|entry| ((entry.file, entry.line), entry.tests))
            .collect();
        Self {
            by_line,
            all: cache.all,
            backend: cache.backend,
            mapped: cache.mapped,
        }
    }
}

impl CoverageMap {
    fn disabled() -> Self {
        Self {
            by_line: BTreeMap::new(),
            all: Vec::new(),
            backend: "disabled".into(),
            mapped: 0,
        }
    }

    fn tests_for(&self, file: &str, line: usize) -> Vec<TestCase> {
        self.by_line
            .get(&(file.to_string(), line))
            .cloned()
            .unwrap_or_else(|| self.all.clone())
    }
}

fn build_coverage_map(project: &Path, manifest: &Path) -> Result<CoverageMap> {
    let coverage_cache = coverage_cache_path(project)?;
    if coverage_cache.is_file()
        && let Ok(cache) = serde_json::from_slice::<CoverageCache>(&fs::read(&coverage_cache)?)
    {
        return Ok(cache.into());
    }
    let routing_root = std::env::temp_dir()
        .join("rust-mutant-routing")
        .join(format!("{:016x}", stable_path_hash(project)));
    if routing_root.exists() {
        fs::remove_dir_all(&routing_root)?;
    }
    fs::create_dir_all(&routing_root)?;
    let target_dir = routing_root.join("target");
    let listed = nextest_list(project, manifest, &target_dir).unwrap_or_default();
    let all = if listed.is_empty() {
        static_test_cases(project)
    } else {
        listed
    };
    if all.is_empty() {
        return Ok(CoverageMap {
            by_line: BTreeMap::new(),
            all,
            backend: "static-fallback".into(),
            mapped: 0,
        });
    }
    let profile_root = routing_root.join("profiles");
    fs::create_dir_all(&profile_root)?;
    let mut by_line: BTreeMap<(String, usize), Vec<TestCase>> = BTreeMap::new();
    let mut llvm_successes = 0usize;
    for (index, test) in all.iter().enumerate() {
        let test_dir = profile_root.join(index.to_string());
        fs::create_dir_all(&test_dir)?;
        let profile_pattern = test_dir.join("%p-%m.profraw");
        let mut command = Command::new("cargo");
        command
            .current_dir(project)
            .arg("nextest")
            .arg("run")
            .arg("--manifest-path")
            .arg(manifest)
            .arg("--target-dir")
            .arg(&target_dir)
            .arg("--test")
            .arg(&test.binary)
            .arg(&test.name)
            .arg("--no-fail-fast")
            .arg("--status-level")
            .arg("fail")
            .arg("--final-status-level")
            .arg("none")
            .env("RUSTFLAGS", "-C instrument-coverage")
            .env("LLVM_PROFILE_FILE", profile_pattern)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if command.status().is_err() {
            continue;
        }
        let profiles = fs::read_dir(&test_dir)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|x| x == "profraw"))
            .collect::<Vec<_>>();
        if profiles.is_empty() {
            continue;
        }
        let profdata = test_dir.join("merged.profdata");
        let mut merge = Command::new(llvm_tool("llvm-profdata"));
        merge.arg("merge").arg("-sparse").arg("-o").arg(&profdata);
        for profile in profiles {
            merge.arg(profile);
        }
        if !merge.status().is_ok_and(|status| status.success()) {
            continue;
        }
        let Some(binary) = nextest_binary_path(project, manifest, &target_dir, &test.binary) else {
            continue;
        };
        let export = Command::new(llvm_tool("llvm-cov"))
            .arg("export")
            .arg("--format=text")
            .arg(format!("--instr-profile={}", profdata.display()))
            .arg(binary)
            .output();
        let Ok(export) = export else { continue };
        if !export.status.success() {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&export.stdout) else {
            continue;
        };
        let Some(files) = value["data"][0]["files"].as_array() else {
            continue;
        };
        llvm_successes += 1;
        for file in files {
            let Some(name) = file["filename"].as_str() else {
                continue;
            };
            let relative = coverage_relative_path(project, name);
            let Some(segments) = file["segments"].as_array() else {
                continue;
            };
            for segment in segments {
                let Some(line) = segment[0].as_u64() else {
                    continue;
                };
                let count = segment[2].as_u64().unwrap_or(0);
                if count == 0 {
                    continue;
                }
                let key = (relative.clone(), line as usize);
                let entries = by_line.entry(key).or_default();
                if !entries.iter().any(|entry| entry.label == test.label) {
                    entries.push(test.clone());
                }
            }
        }
    }
    if llvm_successes == 0 || by_line.is_empty() {
        let fallback = static_coverage_map(project, all);
        save_coverage_map(&coverage_cache, &fallback)?;
        let _ = fs::remove_dir_all(&routing_root);
        return Ok(fallback);
    }
    let _ = fs::remove_dir_all(&routing_root);
    let map = CoverageMap {
        mapped: by_line.len(),
        by_line,
        all,
        backend: "llvm-cov".into(),
    };
    save_coverage_map(&coverage_cache, &map)?;
    Ok(map)
}

fn coverage_cache_path(project: &Path) -> Result<PathBuf> {
    let root = std::env::temp_dir().join("rust-mutant-coverage-cache");
    fs::create_dir_all(&root)?;
    Ok(root.join(format!("{:016x}.json", project_content_hash(project)?)))
}

fn project_content_hash(project: &Path) -> Result<u64> {
    let mut value = String::new();
    for relative_root in ["src", "tests"] {
        for entry in WalkDir::new(project.join(relative_root))
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file() {
                value.push_str(&slash_path(entry.path()));
                value.push_str(&hash_file(entry.path())?);
            }
        }
    }
    value.push_str(&hash_file(&project.join("Cargo.toml"))?);
    Ok(string_hash(&value))
}

fn save_coverage_map(path: &Path, map: &CoverageMap) -> Result<()> {
    let cache = CoverageCache {
        entries: map
            .by_line
            .iter()
            .map(|((file, line), tests)| CoverageEntry {
                file: file.clone(),
                line: *line,
                tests: tests.clone(),
            })
            .collect(),
        all: map.all.clone(),
        backend: map.backend.clone(),
        mapped: map.mapped,
    };
    let temp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temp, serde_json::to_vec(&cache)?)?;
    fs::rename(temp, path)?;
    Ok(())
}

fn nextest_list(project: &Path, manifest: &Path, target_dir: &Path) -> Result<Vec<TestCase>> {
    let output = Command::new("cargo")
        .current_dir(project)
        .arg("nextest")
        .arg("list")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target_dir)
        .arg("--message-format")
        .arg("json")
        .output()?;
    if !output.status.success() {
        bail!("nextest list failed");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text
        .find('{')
        .ok_or_else(|| anyhow!("nextest list returned no JSON"))?;
    let value: serde_json::Value = serde_json::from_str(&text[start..])?;
    let mut cases = Vec::new();
    if let Some(suites) = value["rust-suites"].as_object() {
        for suite in suites.values() {
            if suite["kind"] != "test" {
                continue;
            }
            let Some(binary) = suite["binary-name"].as_str() else {
                continue;
            };
            let Some(testcases) = suite["testcases"].as_object() else {
                continue;
            };
            for name in testcases.keys() {
                cases.push(TestCase {
                    binary: binary.into(),
                    name: name.clone(),
                    label: format!("{binary}::{name}"),
                });
            }
        }
    }
    cases.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(cases)
}

fn nextest_binary_path(
    _project: &Path,
    _manifest: &Path,
    target_dir: &Path,
    binary: &str,
) -> Option<PathBuf> {
    let deps = target_dir.join("debug").join("deps");
    let prefix = format!("{binary}-");
    WalkDir::new(deps)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.file_name().to_string_lossy().starts_with(&prefix)
                && entry
                    .path()
                    .metadata()
                    .map(|meta| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            meta.permissions().mode() & 0o111 != 0
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = meta;
                            true
                        }
                    })
                    .unwrap_or(false)
        })
        .max_by_key(|entry| {
            fs::metadata(entry.path())
                .ok()
                .and_then(|meta| meta.modified().ok())
        })
        .map(|entry| entry.into_path())
}

fn static_test_cases(project: &Path) -> Vec<TestCase> {
    let mut cases = Vec::new();
    let tests = project.join("tests");
    if !tests.is_dir() {
        return cases;
    }
    for entry in WalkDir::new(tests)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(source) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let binary = entry
            .path()
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(name) = trimmed
                .strip_prefix("fn ")
                .and_then(|value| value.split('(').next())
            {
                cases.push(TestCase {
                    binary: binary.clone(),
                    name: name.into(),
                    label: format!("{binary}::{name}"),
                });
            }
        }
    }
    cases.sort_by(|a, b| a.label.cmp(&b.label));
    cases
}

fn static_coverage_map(project: &Path, all: Vec<TestCase>) -> CoverageMap {
    let mut by_line = BTreeMap::new();
    for entry in WalkDir::new(project.join("src"))
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let Ok(source) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let relative = slash_path(entry.path().strip_prefix(project).unwrap_or(entry.path()));
        let module = entry
            .path()
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let matching = all
            .iter()
            .filter(|test| {
                let test_file = project.join("tests").join(format!("{}.rs", test.binary));
                fs::read_to_string(test_file)
                    .map(|text| text.contains(&format!("{module}::")))
                    .unwrap_or(false)
            })
            .cloned()
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        for (line, _) in source.lines().enumerate() {
            by_line.insert((relative.clone(), line + 1), matching.clone());
        }
    }
    CoverageMap {
        mapped: by_line.len(),
        by_line,
        all,
        backend: "static-fallback".into(),
    }
}

fn coverage_relative_path(project: &Path, name: &str) -> String {
    let path = Path::new(name);
    if let Ok(relative) = path.strip_prefix(project) {
        return slash_path(relative);
    }
    let normalized = name.replace('\\', "/");
    normalized
        .find("src/")
        .map_or(normalized.clone(), |index| normalized[index..].into())
}

fn llvm_tool(name: &str) -> PathBuf {
    if let Ok(sysroot) = Command::new("rustc").arg("--print").arg("sysroot").output() {
        let sysroot = String::from_utf8_lossy(&sysroot.stdout).trim().to_string();
        let host = rust_host();
        let candidate = PathBuf::from(sysroot)
            .join("lib/rustlib")
            .join(host)
            .join("bin")
            .join(name);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(name)
}

fn rust_host() -> String {
    Command::new("rustc")
        .arg("-vV")
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
        })
        .unwrap_or_else(|| "x86_64-unknown-linux-gnu".into())
}

#[derive(Debug)]
struct CommandResult {
    code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
    duration_ms: u128,
    command: String,
}
impl CommandResult {
    fn output(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

fn cargo_test(
    cwd: &Path,
    manifest: &Path,
    target_dir: &Path,
    timeout: Duration,
    filter: Option<&str>,
) -> Result<CommandResult> {
    let mut command = Command::new("cargo");
    #[cfg(unix)]
    command.process_group(0);
    command
        .current_dir(cwd)
        .arg("test")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target_dir)
        .arg("--quiet")
        .arg("--")
        .arg("--test-threads=1");
    if let Some(filter) = filter {
        command.arg(filter);
    }
    let command_text = format!("cargo test --manifest-path {}", manifest.display());
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn cargo test in {}", cwd.display()))?;
    let mut result = wait_child(&mut child, timeout)?;
    result.command = command_text;
    Ok(result)
}

fn cargo_nextest(
    cwd: &Path,
    manifest: &Path,
    target_dir: &Path,
    timeout: Duration,
    test: Option<&TestCase>,
) -> Result<CommandResult> {
    let mut command = Command::new("cargo");
    #[cfg(unix)]
    command.process_group(0);
    command
        .current_dir(cwd)
        .arg("nextest")
        .arg("run")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target_dir)
        .arg("--no-fail-fast")
        .arg("--status-level")
        .arg("fail")
        .arg("--final-status-level")
        .arg("none");
    let mut command_text = String::from("cargo nextest run");
    if let Some(test) = test {
        command.arg("--test").arg(&test.binary).arg(&test.name);
        command_text.push_str(&format!(" --test {} {}", test.binary, test.name));
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn cargo nextest in {}", cwd.display()))?;
    let mut result = wait_child(&mut child, timeout)?;
    result.command = command_text;
    Ok(result)
}

fn classify_command(command: &CommandResult) -> Status {
    if command.timed_out {
        Status::Timeout
    } else if command.code == Some(0) {
        Status::Survived
    } else if looks_like_compile_error(&command.output()) {
        Status::CompileError
    } else {
        Status::Killed
    }
}

fn wait_child(child: &mut Child, timeout: Duration) -> Result<CommandResult> {
    let start = Instant::now();
    let mut timed_out = false;
    loop {
        record_peak_rss();
        if let Some(status) = child.try_wait()? {
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_string(&mut stdout)?;
            }
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)?;
            }
            return Ok(CommandResult {
                code: status.code(),
                timed_out,
                stdout,
                stderr,
                duration_ms: start.elapsed().as_millis(),
                command: String::new(),
            });
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            #[cfg(unix)]
            {
                let process_group = -(child.id() as i32);
                // Cargo is launched as its own process group so test binaries and
                // rustc children cannot survive a timeout and poison later runs.
                let _ = unsafe { libc::kill(process_group, libc::SIGKILL) };
            }
            let _ = child.kill();
            let status = child.wait()?;
            let mut stdout = String::new();
            let mut stderr = String::new();
            if let Some(mut pipe) = child.stdout.take() {
                pipe.read_to_string(&mut stdout)?;
            }
            if let Some(mut pipe) = child.stderr.take() {
                pipe.read_to_string(&mut stderr)?;
            }
            return Ok(CommandResult {
                code: status.code(),
                timed_out,
                stdout,
                stderr,
                duration_ms: start.elapsed().as_millis(),
                command: String::new(),
            });
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn copy_project(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(source)?;
        if rel.components().any(|component| matches!(component, std::path::Component::Normal(value) if value == "target" || value == ".git" || value == ".rust-mutant")) { continue; }
        let dest = destination.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

struct ScratchCleanupGuard {
    root: PathBuf,
}

impl ScratchCleanupGuard {
    fn new(project: &Path) -> Result<Self> {
        let root = worker_scratch_root(project);
        if root.exists() {
            fs::remove_dir_all(&root)
                .with_context(|| format!("clean stale scratch root {}", root.display()))?;
        }
        Ok(Self { root })
    }
}

impl Drop for ScratchCleanupGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct SourceRestoreGuard {
    pristine: PathBuf,
    target: PathBuf,
}

impl SourceRestoreGuard {
    fn new(pristine: PathBuf, target: PathBuf) -> Self {
        Self { pristine, target }
    }

    fn restore(&self) -> Result<()> {
        if let Some(parent) = self.target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&self.pristine, &self.target)
            .with_context(|| format!("restore scratch source {}", self.target.display()))?;
        Ok(())
    }
}

impl Drop for SourceRestoreGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn worker_scratch_root(project: &Path) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rust-mutant-scratch-{:016x}",
        stable_path_hash(project)
    ))
}

fn worker_scratch_dir(project: &Path, worker: usize) -> PathBuf {
    worker_scratch_root(project).join(format!("worker-{worker}"))
}

fn restore_source_file(project: &Path, scratch: &Path, relative: &str) -> Result<()> {
    let source = project.join(relative);
    let target = scratch.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&source, &target)
        .with_context(|| format!("restore scratch source {}", target.display()))?;
    Ok(())
}

fn external_target_dir(project: &Path) -> PathBuf {
    std::env::temp_dir()
        .join("rust-mutant-target")
        .join(format!("{:016x}", stable_path_hash(project)))
}

fn stable_path_hash(path: &Path) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for byte in path.to_string_lossy().bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn stable_diagnostic(value: &str) -> String {
    let mut result = strip_ansi(value);
    let scratch_marker = "rust-mutant-scratch-";
    while let Some(start) = result.find(scratch_marker) {
        let suffix_start = start + scratch_marker.len();
        let end = result[suffix_start..]
            .find(['/', '\\'])
            .map_or(result.len(), |offset| suffix_start + offset);
        result.replace_range(start..end, "rust-mutant-scratch");
    }
    let mut search_from = 0usize;
    while let Some(relative) = result[search_from..].find("thread '") {
        let start = search_from + relative;
        let Some(open_relative) = result[start..].find("' (") else {
            break;
        };
        let digits_start = start + open_relative + 3;
        let digits_end = result[digits_start..]
            .find(')')
            .map_or(digits_start, |offset| digits_start + offset);
        if digits_end == digits_start
            || !result[digits_start..digits_end]
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            search_from = digits_start.max(start + 8);
            continue;
        }
        result.replace_range(digits_start..digits_end, "pid");
        search_from = digits_start + 3;
    }
    let mut kept = Vec::new();
    let mut compile_errors = Vec::new();
    for line in result.lines() {
        let mut line = line.to_string();
        if let Some(start) = line.find("finished in ") {
            let duration_start = start + "finished in ".len();
            if let Some(end_offset) = line[duration_start..].find('s') {
                let end = duration_start + end_offset + 1;
                line.replace_range(duration_start..end, "duration");
            }
        }
        if let Some(start) = line.find("Summary [")
            && let Some(end_offset) = line[start + "Summary [".len()..].find(']')
        {
            let duration_start = start + "Summary [".len();
            let end = duration_start + end_offset;
            line.replace_range(duration_start..end, "duration");
        }
        if let Some(start) = line.find("Nextest run ID ") {
            let id_start = start + "Nextest run ID ".len();
            let id_end = line[id_start..]
                .find(char::is_whitespace)
                .map_or(line.len(), |offset| id_start + offset);
            line.replace_range(id_start..id_end, "id");
        }
        if line.contains("Finished ")
            && let Some(start) = line.rfind(" in ")
        {
            let duration_start = start + " in ".len();
            if line[duration_start..].ends_with('s') {
                line.replace_range(duration_start.., "duration");
            }
        }
        if let Some(start) = line.find("[   ")
            && let Some(end_offset) = line[start..].find("s]")
        {
            line.replace_range(start..start + end_offset + 2, "[duration]");
        }
        if line.starts_with("error: could not compile") {
            compile_errors.push(line);
        } else {
            kept.push(line);
        }
    }
    compile_errors.sort_unstable();
    kept.extend(compile_errors);
    let mut normalized = kept.join("\n");
    if result.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn strip_ansi(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            result.push(character);
            continue;
        }
        if characters.next() != Some('[') {
            continue;
        }
        for character in characters.by_ref() {
            if ('@'..='~').contains(&character) {
                break;
            }
        }
    }
    result
}

fn looks_like_compile_error(output: &str) -> bool {
    output.contains("could not compile")
        || output.contains("error[E")
        || output.contains("mismatched types")
        || output.contains("expected one of")
        || output.contains("unclosed delimiter")
}

fn summarize(results: &[MutantResult], threshold: Option<f64>) -> Summary {
    let count = |status: &str| results.iter().filter(|r| r.status == status).count();
    let killed = count("killed");
    let survived = count("survived");
    let not_covered = count("not_covered");
    let compile_error = count("compile_error");
    let timeout = count("timeout");
    let equivalent = count("equivalent");
    let denominator = killed + survived;
    let msi = if denominator == 0 {
        100.0
    } else {
        killed as f64 * 100.0 / denominator as f64
    };
    Summary {
        total: results.len(),
        killed,
        survived,
        not_covered,
        compile_error,
        timeout,
        equivalent,
        msi,
        threshold,
        threshold_passed: threshold.is_none_or(|value| msi >= value),
        excluded_buckets: vec![
            "not_covered".into(),
            "compile_error".into(),
            "timeout".into(),
            "equivalent".into(),
        ],
    }
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn expected_function_name(project: &Path, mutant: &Mutant) -> Option<String> {
    let source = fs::read_to_string(project.join(&mutant.file)).ok()?;
    source
        .lines()
        .take(mutant.line)
        .filter_map(|line| line.find("fn ").map(|start| &line[start + 3..]))
        .filter_map(|line| {
            let name = line
                .trim_start()
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .next()?;
            (!name.is_empty()).then(|| name.trim_start_matches("r#").to_string())
        })
        .last()
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        value.to_string()
    } else {
        format!("{}…", &value[..limit.min(value.floor_char_boundary(limit))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_operator_names_are_stable() {
        assert_eq!(GENERIC_FAMILIES.len(), 10);
        assert_eq!(GENERIC_FAMILIES[9], "loop-inc-dec");
        assert_eq!(PUBLIC_FAMILIES.len(), 18);
    }

    #[test]
    fn multi_mutant_selector_preserves_order_and_rejects_missing_ids() {
        let mutants = vec![bare_mutant("m0001-first"), bare_mutant("m0002-second")];
        let ids = BTreeSet::from(["m0002-second".to_string(), "m0001-first".to_string()]);
        let selected = select_mutants(mutants.clone(), None, Some(&ids)).unwrap();
        assert_eq!(
            selected
                .iter()
                .map(|mutant| mutant.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m0001-first", "m0002-second"]
        );

        let missing = BTreeSet::from(["m9999-missing".to_string()]);
        assert!(select_mutants(mutants, None, Some(&missing)).is_err());
    }

    fn bare_mutant(id: &str) -> Mutant {
        Mutant {
            id: id.into(),
            file: "src/lib.rs".into(),
            line: 1,
            column: 1,
            family: "ROR".into(),
            subtype: "test".into(),
            original: "==".into(),
            replacement: "!=".into(),
            start_byte: 0,
            end_byte: 2,
            source_line: String::new(),
        }
    }

    #[test]
    fn discovery_is_deterministic_and_masks_comments() {
        let source = "pub fn f(a: i32, b: i32) -> bool { // + &&\n a > b && a < b\n}";
        let masked = mask_non_code(source.as_bytes());
        let first = discover_file(source, &masked, "src/lib.rs", None);
        let second = discover_file(source, &masked, "src/lib.rs", None);
        assert_eq!(first.len(), second.len());
        assert!(!first.iter().any(|m| m.line == 1));
    }

    #[test]
    fn ror_only_targets_binary_expression_operators() {
        let source = r#"
struct ProbeEntry;

struct ProbePlan {
    drift: Vec<ProbeEntry>,
    optional: Option<ProbeEntry>,
}

fn probe(left: usize, right: usize) {
    let _vec: Vec<ProbeEntry> = Vec::new();
    let _option: Option<ProbeEntry> = None;
    let _nested: Result<Option<ProbeEntry>, ProbeEntry> = Ok(None);
    let _map: std::collections::HashMap<usize, ProbeEntry> = std::collections::HashMap::new();
    let _turbofish = Option::<ProbeEntry>::None;
    let _lt = left < right;
    let _gt = left > right;
    let _le = left <= right;
    let _ge = left >= right;
    let _eq = left == right;
    let _ne = left != right;
}
"#;
        let masked = mask_non_code(source.as_bytes());
        let mutants = discover_file(source, &masked, "src/lib.rs", None);
        let ror = mutants
            .iter()
            .filter(|mutant| mutant.family == "ROR")
            .collect::<Vec<_>>();

        assert_eq!(ror.len(), 6, "unexpected ROR mutants: {ror:?}");
        assert_eq!(
            ror.iter()
                .map(|mutant| mutant.original.as_str())
                .collect::<Vec<_>>(),
            vec!["<", ">", "<=", ">=", "==", "!="]
        );
        assert_eq!(
            ror.iter()
                .map(|mutant| (mutant.original.as_str(), mutant.replacement.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("<", ">"),
                (">", "<"),
                ("<=", ">"),
                (">=", "<"),
                ("==", "!="),
                ("!=", "=="),
            ]
        );
        assert!(ror.iter().all(|mutant| mutant.source_line.contains("left")));
    }

    #[test]
    fn summary_excludes_non_behavioral_buckets() {
        let mut results = Vec::new();
        for status in [
            "killed",
            "survived",
            "not_covered",
            "compile_error",
            "timeout",
        ] {
            results.push(MutantResult {
                mutant: Mutant {
                    id: status.into(),
                    file: "x".into(),
                    line: 1,
                    column: 1,
                    family: "AOR".into(),
                    subtype: "x".into(),
                    original: "+".into(),
                    replacement: "-".into(),
                    start_byte: 0,
                    end_byte: 1,
                    source_line: String::new(),
                },
                status: status.into(),
                tests_run: vec![],
                duration_ms: 0,
                cache: "miss".into(),
                command: None,
                details: None,
                tce: None,
            });
        }
        assert_eq!(summarize(&results, None).msi, 50.0);
    }

    #[test]
    fn idiomatic_families_have_source_pairs() {
        let source = r#"
async fn probes() {
    let _ = Some(()).unwrap();
    let _ = Some(()).expect("ok");
    async { 1_i32 }.await;
    let closure = move || 1_i32;
    let mut value = 1_i32;
    let _ = &mut value;
    let _ = (1_i32).clone();
    let _ = std::sync::Arc::new(1_i32);
    let _ = vec![1_i32].into_iter().map(|_| true).count();
    let _ = vec![1_i32].into_iter().filter(|_| true).count();
    let _ = vec![1_i32].into_iter().collect::<Vec<_>>();
}
fn optional_probe() -> Option<()> {
    Some(())?;
    Some(())
}
"#;
        let masked = mask_non_code(source.as_bytes());
        let mutants = discover_file(source, &masked, "src/probes.rs", None);
        let pairs = [
            ("question-mark-removal", "?", ""),
            ("unwrap-expect-removal", ".unwrap()", ""),
            ("unwrap-expect-removal", ".expect(\"ok\")", ""),
            ("await-removal", ".await", ""),
            ("move-closure-removal", "move", ""),
            ("mut-to-shared", "&mut", "&"),
            ("clone-removal", ".clone()", ""),
            ("arc-rc-swap", "std::sync::Arc", "std::rc::Rc"),
            ("iterator-chain", ".map", ".filter"),
            ("iterator-chain", ".filter", ".map"),
            ("iterator-chain", ".collect::<Vec<_>>()", ".count()"),
        ];
        for (family, original, replacement) in pairs {
            assert!(
                mutants.iter().any(|mutant| {
                    mutant.family == family
                        && mutant.original == original
                        && mutant.replacement == replacement
                }),
                "missing source pair {family}: {original:?} -> {replacement:?}"
            );
        }
        assert!(!mutants.iter().any(|mutant| mutant.family == "LOR"));
        assert!(
            !mutants
                .iter()
                .any(|mutant| mutant.family == "ROR" && mutant.original == "<")
        );
    }
}
