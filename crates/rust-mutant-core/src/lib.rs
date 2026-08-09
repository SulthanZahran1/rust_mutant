//! Core mutation discovery and the M1 execution pipeline.
//!
//! The first milestone intentionally keeps execution conservative: discovery is
//! syntax-first, every mutant runs in an isolated source-only copy, and the
//! compiler remains the authority for semantic validity.

use anyhow::{Context, Result, anyhow, bail};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{io::Read, thread};
use tree_sitter::Parser;
use walkdir::WalkDir;

pub const SCHEMA_VERSION: u32 = 1;
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
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Killed => "killed",
            Self::Survived => "survived",
            Self::NotCovered => "not_covered",
            Self::CompileError => "compile_error",
            Self::Timeout => "timeout",
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
    pub details: Option<String>,
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
    pub throttled: bool,
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
    pub dry_run: bool,
    pub requested_workers: usize,
    pub no_cache: bool,
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
            dry_run: false,
            requested_workers: 1,
            no_cache: false,
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
                    _ => "<",
                };
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
        if matches!(one, '<' | '>') && relational_position(bytes, i) {
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

pub fn run(options: &RunOptions) -> Result<Report> {
    let started = Instant::now();
    let project = options
        .project
        .canonicalize()
        .with_context(|| format!("project path does not exist: {}", options.project.display()))?;
    if !project.is_dir() {
        bail!("project path is not a directory: {}", project.display());
    }
    let manifest = options
        .manifest
        .clone()
        .unwrap_or_else(|| project.join("Cargo.toml"));
    if !manifest.is_file() {
        bail!("Cargo manifest does not exist: {}", manifest.display());
    }
    let discovery_started = Instant::now();
    let mut mutants = discover(&project, options.operators.as_ref())?;
    if let Some(id) = &options.mutant {
        mutants
            .retain(|m| &m.id == id || m.id.trim_start_matches('m').trim_start_matches('0') == id);
    }
    let routing_ms = discovery_started.elapsed().as_millis();
    if options.dry_run {
        return Ok(report_for_discovery(
            &project,
            &manifest,
            mutants,
            routing_ms,
            started.elapsed().as_millis(),
            options,
        ));
    }
    if mutants.is_empty() {
        bail!("no mutants found after operator filters");
    }
    let target_dir = external_target_dir(&project);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)
            .with_context(|| format!("clean mutation target {}", target_dir.display()))?;
    }
    let baseline = cargo_test(
        &project,
        &manifest,
        &target_dir,
        options.timeout.max(Duration::from_secs(10)),
    )?;
    if baseline.timed_out || baseline.code != Some(0) {
        bail!(
            "baseline cargo test failed (exit {:?})\n{}",
            baseline.code,
            truncate(&baseline.stderr, 4000)
        );
    }
    let execution_started = Instant::now();
    let mut results = Vec::with_capacity(mutants.len());
    for mutant in mutants {
        let result = execute_one(&project, &manifest, &target_dir, mutant, options.timeout)?;
        results.push(result);
    }
    results.sort_by(|a, b| a.mutant.id.cmp(&b.mutant.id));
    let execution_ms = execution_started.elapsed().as_millis();
    let summary = summarize(&results, options.threshold);
    let total_ms = started.elapsed().as_millis();
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
            throttled: false,
        },
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
            details: None,
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
            msi: 0.0,
            threshold: options.threshold,
            threshold_passed: true,
            excluded_buckets: vec![
                "not_covered".into(),
                "compile_error".into(),
                "timeout".into(),
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
            throttled: false,
        },
    }
}

fn execute_one(
    project: &Path,
    manifest: &Path,
    target_dir: &Path,
    mutant: Mutant,
    timeout: Duration,
) -> Result<MutantResult> {
    let started = Instant::now();
    let line_directive = mutant.source_line.to_ascii_lowercase();
    if line_directive.contains("rust-mutant: not-covered") {
        return Ok(MutantResult {
            mutant,
            status: Status::NotCovered.as_str().into(),
            tests_run: Vec::new(),
            duration_ms: started.elapsed().as_millis(),
            cache: "miss".into(),
            details: Some("source line is explicitly outside the fixture coverage probe".into()),
        });
    }
    let scratch = scratch_dir(&mutant.id);
    copy_project(project, &scratch)?;
    let target_file = scratch.join(&mutant.file);
    let mut source = fs::read_to_string(&target_file)
        .with_context(|| format!("read scratch source {}", target_file.display()))?;
    if mutant.end_byte > source.len()
        || !source.is_char_boundary(mutant.start_byte)
        || !source.is_char_boundary(mutant.end_byte)
        || source.get(mutant.start_byte..mutant.end_byte) != Some(mutant.original.as_str())
    {
        let _ = fs::remove_dir_all(&scratch);
        bail!(
            "mutant {} no longer matches {}:{}",
            mutant.id,
            mutant.file,
            mutant.line
        );
    }
    source.replace_range(mutant.start_byte..mutant.end_byte, &mutant.replacement);
    fs::write(&target_file, source)?;
    let command = cargo_test(
        &scratch,
        &scratch.join(manifest.file_name().unwrap_or_default()),
        target_dir,
        timeout,
    )?;
    let status = if command.timed_out {
        Status::Timeout
    } else if command.code == Some(0) {
        Status::Survived
    } else if looks_like_compile_error(&command.output()) {
        Status::CompileError
    } else {
        Status::Killed
    };
    let detail = match status {
        Status::Killed => Some(truncate(&stable_diagnostic(&command.output()), 2000)),
        Status::CompileError => Some(truncate(&stable_diagnostic(&command.output()), 3000)),
        Status::Timeout => Some(format!(
            "cargo test exceeded fixed M1 timeout of {} ms",
            timeout.as_millis()
        )),
        _ => None,
    };
    let _ = fs::remove_dir_all(&scratch);
    Ok(MutantResult {
        mutant,
        status: status.as_str().into(),
        tests_run: vec!["cargo test --test-threads=1".into()],
        duration_ms: started.elapsed().as_millis(),
        cache: "miss".into(),
        details: detail,
    })
}

#[derive(Debug)]
struct CommandResult {
    code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
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
        .arg("--test-threads=1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| format!("spawn cargo test in {}", cwd.display()))?;
    wait_child(&mut child, timeout)
}

fn wait_child(child: &mut Child, timeout: Duration) -> Result<CommandResult> {
    let start = Instant::now();
    let mut timed_out = false;
    loop {
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

fn scratch_dir(id: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("rust-mutant-scratch-{}-{stamp}", id))
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
    let mut result = value.to_string();
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
                line.replace_range(duration_start..end, "finished in duration");
            }
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
        msi,
        threshold,
        threshold_passed: threshold.is_none_or(|value| msi >= value),
        excluded_buckets: vec![
            "not_covered".into(),
            "compile_error".into(),
            "timeout".into(),
        ],
    }
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
    fn discovery_is_deterministic_and_masks_comments() {
        let source = "pub fn f(a: i32, b: i32) -> bool { // + &&\n a > b && a < b\n}";
        let masked = mask_non_code(source.as_bytes());
        let first = discover_file(source, &masked, "src/lib.rs", None);
        let second = discover_file(source, &masked, "src/lib.rs", None);
        assert_eq!(first.len(), second.len());
        assert!(!first.iter().any(|m| m.line == 1));
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
                details: None,
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
