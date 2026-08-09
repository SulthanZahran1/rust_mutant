//! Sound, one-sided LLVM-IR trivial compiler equivalence analysis.
//!
//! TCE deliberately runs through `cargo build` in debug mode. Direct `rustc`
//! and release builds can dead-code-eliminate public functions at `-O2`, which
//! would make an empty IR file look equivalent. Empty or missing IR is always
//! an error.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TceResult {
    pub enabled: bool,
    pub status: String,
    pub equivalent: bool,
    pub duration_ms: u128,
    pub original_ir_hash: Option<String>,
    pub mutant_ir_hash: Option<String>,
    pub error: Option<String>,
}

impl TceResult {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            status: "disabled".into(),
            equivalent: false,
            duration_ms: 0,
            original_ir_hash: None,
            mutant_ir_hash: None,
            error: None,
        }
    }

    pub fn error(error: impl Into<String>, duration_ms: u128) -> Self {
        Self {
            enabled: true,
            status: "error".into(),
            equivalent: false,
            duration_ms,
            original_ir_hash: None,
            mutant_ir_hash: None,
            error: Some(error.into()),
        }
    }
}

/// Compile the pristine project and a mutated scratch project through Cargo,
/// then compare their normalized library LLVM IR. Equality is sufficient for
/// an equivalence classification; inequality remains a normal survivor.
pub fn compare(
    original_project: &Path,
    original_manifest: &Path,
    mutant_project: &Path,
    mutant_manifest: &Path,
    target_root: &Path,
    expected_function: Option<&str>,
) -> Result<TceResult> {
    let started = Instant::now();
    let original_target = target_root.join("original");
    let mutant_target = target_root.join("mutant");
    if target_root.exists() {
        fs::remove_dir_all(target_root)
            .with_context(|| format!("clean TCE target {}", target_root.display()))?;
    }
    fs::create_dir_all(target_root)?;

    let original_ir = compile_and_collect(
        original_project,
        original_manifest,
        &original_target,
        expected_function,
    )?;
    let mutant_ir = compile_and_collect(
        mutant_project,
        mutant_manifest,
        &mutant_target,
        expected_function,
    )?;
    let original_hash = stable_hash(&original_ir);
    let mutant_hash = stable_hash(&mutant_ir);
    if std::env::var_os("RUST_MUTANT_TCE_DUMP").is_some() {
        fs::write(target_root.join("original.normalized.ll"), &original_ir)?;
        fs::write(target_root.join("mutant.normalized.ll"), &mutant_ir)?;
    }
    let equivalent = original_ir == mutant_ir;

    Ok(TceResult {
        enabled: true,
        status: if equivalent {
            "equivalent"
        } else {
            "different"
        }
        .into(),
        equivalent,
        duration_ms: started.elapsed().as_millis(),
        original_ir_hash: Some(original_hash),
        mutant_ir_hash: Some(mutant_hash),
        error: None,
    })
}

fn compile_and_collect(
    project: &Path,
    manifest: &Path,
    target: &Path,
    expected_function: Option<&str>,
) -> Result<String> {
    let package = package_name(manifest)?;
    let project_prefix = project.to_string_lossy().replace('\\', "/");
    let remap = format!("--remap-path-prefix={project_prefix}=/SRC");
    let encoded_flags = [
        "-C",
        "opt-level=2",
        "-C",
        "debuginfo=0",
        "--emit=llvm-ir",
        &remap,
    ]
    .join("\u{1f}");
    let output = Command::new("cargo")
        .current_dir(project)
        .arg("build")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("--target-dir")
        .arg(target)
        .arg("--quiet")
        .env_remove("RUSTFLAGS")
        // The parent runner may set CARGO_INCREMENTAL=0 for reproducible
        // tests. Non-incremental debug LLVM emission can contain only module
        // metadata for an otherwise public fixture, so TCE owns this setting.
        .env("CARGO_INCREMENTAL", "1")
        .env("CARGO_ENCODED_RUSTFLAGS", encoded_flags)
        .output()
        .with_context(|| format!("spawn cargo TCE build in {}", project.display()))?;
    if !output.status.success() {
        bail!(
            "TCE cargo build failed for {} (exit {:?})\n{}",
            project.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let crate_prefix = format!("{}-", package.replace('-', "_"));
    let deps = target.join("debug").join("deps");
    let mut files = Vec::new();
    for entry in WalkDir::new(&deps).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|x| x != "ll") {
            continue;
        }
        let Some(name) = entry.path().file_name().and_then(|x| x.to_str()) else {
            continue;
        };
        if name.starts_with(&crate_prefix) {
            files.push(entry.into_path());
        }
    }
    files.sort();
    let mut pieces = Vec::new();
    for file in files {
        pieces.push(
            fs::read_to_string(&file)
                .with_context(|| format!("read TCE IR artifact {}", file.display()))?,
        );
    }
    if pieces.is_empty() {
        bail!(
            "TCE produced no LLVM IR for package `{package}` under {}",
            deps.display()
        );
    }
    let normalized = normalize_ir(&pieces.join("\n"));
    let definitions = normalized
        .lines()
        .filter(|line| line.trim_start().starts_with("define "))
        .count();
    if definitions == 0 {
        bail!("TCE produced empty LLVM IR for package `{package}`; refusing equivalence");
    }
    if let Some(expected_function) = expected_function
        && !has_expected_definition(&normalized, expected_function)
    {
        bail!(
            "TCE expected function `{expected_function}` is absent from LLVM IR for package `{package}`; refusing equivalence"
        );
    }
    Ok(normalized)
}

fn package_name(manifest: &Path) -> Result<String> {
    let text = fs::read_to_string(manifest)
        .with_context(|| format!("read TCE manifest {}", manifest.display()))?;
    let in_package = text
        .lines()
        .scan(false, |seen, line| {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                *seen = trimmed == "[package]";
            }
            Some((*seen, trimmed.to_string()))
        })
        .find_map(|(seen, line)| {
            if !seen || !line.starts_with("name") {
                return None;
            }
            let (_, value) = line.split_once('=')?;
            Some(value.trim().trim_matches('"').to_string())
        });
    in_package.filter(|name| !name.is_empty()).ok_or_else(|| {
        anyhow!(
            "TCE could not determine package name from {}",
            manifest.display()
        )
    })
}

fn normalize_ir(ir: &str) -> String {
    let mut lines = Vec::new();
    for raw in ir.lines() {
        if is_type_hash_metadata(raw) {
            continue;
        }
        let mut line = raw.to_string();
        if line.trim_start().starts_with("source_filename =") {
            line = "source_filename = \"/SRC\"".into();
        } else if line.trim_start().starts_with("ModuleID =") {
            line = "ModuleID = 'MODULE'".into();
        }
        line = normalize_alloc_names(&line);
        line = normalize_crate_disambiguators(&line);
        line = normalize_panic_location(&line);
        line = normalize_llvm_directories(&line);
        line = normalize_commutative_add(&line);
        lines.push(line);
    }
    lines.join("\n")
}

fn is_type_hash_metadata(line: &str) -> bool {
    let trimmed = line.trim();
    let Some((left, right)) = trimmed.split_once(" = !{i64 ") else {
        return false;
    };
    left.strip_prefix('!')
        .is_some_and(|value| value.chars().all(|c| c.is_ascii_digit()))
        && right
            .strip_suffix('}')
            .is_some_and(|value| value.chars().all(|c| c.is_ascii_digit()))
}

fn normalize_alloc_names(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"@alloc_") {
            result.push_str("@alloc_H");
            index += b"@alloc_".len();
            while index < bytes.len() && bytes[index].is_ascii_hexdigit() {
                index += 1;
            }
        } else {
            result.push(bytes[index] as char);
            index += 1;
        }
    }
    result
}

fn normalize_panic_location(line: &str) -> String {
    let marker = "c\\\"\\0A\\00";
    let Some(start) = line.find(marker) else {
        return line.to_string();
    };
    let Some(end_offset) = line[start + 2..].find('"') else {
        return line.to_string();
    };
    let end = start + 2 + end_offset + 1;
    let mut result = line.to_string();
    result.replace_range(start..end, "c\\\"LOC\\\"");
    result
}

fn normalize_crate_disambiguators(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut result = String::with_capacity(line.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"Cs") {
            let hash_start = index + 2;
            let mut hash_end = hash_start;
            while hash_end < bytes.len() && bytes[hash_end].is_ascii_alphanumeric() {
                hash_end += 1;
            }
            if hash_end > hash_start && hash_end < bytes.len() && bytes[hash_end] == b'_' {
                result.push_str("CsHASH");
                index = hash_end;
                continue;
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

fn normalize_llvm_directories(line: &str) -> String {
    let marker = "directory: \"";
    let mut result = line.to_string();
    let mut search_from = 0usize;
    while let Some(relative) = result[search_from..].find(marker) {
        let content_start = search_from + relative + marker.len();
        let Some(end_offset) = result[content_start..].find('"') else {
            break;
        };
        let content_end = content_start + end_offset;
        result.replace_range(content_start..content_end, "/SRC");
        search_from = content_start + "/SRC".len();
    }
    result
}

fn normalize_commutative_add(line: &str) -> String {
    let Some(op) = line.find(" = add ") else {
        return line.to_string();
    };
    let operands_start = op + " = add ".len();
    let rest = &line[operands_start..];
    let Some(comma) = top_level_comma(rest) else {
        return line.to_string();
    };
    let left = &rest[..comma];
    let Some(separator) = left.rfind(char::is_whitespace) else {
        return line.to_string();
    };
    let prefix = &rest[..=separator];
    let first = left[separator + 1..].trim();
    let right_with_suffix = rest[comma + 1..].trim_start();
    let (second, suffix) = right_with_suffix
        .find(", !")
        .map_or((right_with_suffix, ""), |index| {
            (&right_with_suffix[..index], &right_with_suffix[index..])
        });
    let second = second.trim();
    if first <= second {
        return line.to_string();
    }
    format!(
        "{}{}{}, {}{}",
        &line[..operands_start],
        prefix,
        second,
        first,
        suffix
    )
}

fn top_level_comma(value: &str) -> Option<usize> {
    let mut angle = 0usize;
    let mut paren = 0usize;
    for (index, character) in value.char_indices() {
        match character {
            '<' => angle += 1,
            '>' => angle = angle.saturating_sub(1),
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            ',' if angle == 0 && paren == 0 => return Some(index),
            _ => {}
        }
    }
    None
}

fn has_expected_definition(ir: &str, expected_function: &str) -> bool {
    ir.lines().any(|line| {
        line.contains(expected_function)
            && (line.trim_start().starts_with("define ") || line.contains(" = unnamed_addr alias "))
    })
}

fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_removes_known_noise_only() {
        let input = "ModuleID = 'lib.abc-cgu.0'\nsource_filename = \"/tmp/project/src/lib.rs\"\n@alloc_0123456789abcdef = private unnamed_addr constant\n!7 = !{i64 1234}\ndefine void @f()";
        let output = normalize_ir(input);
        assert!(output.contains("ModuleID = 'MODULE'"));
        assert!(output.contains("source_filename = \"/SRC\""));
        assert!(output.contains("@alloc_H"));
        assert!(!output.contains("!7 = !{i64 1234}"));
        assert!(output.contains("define void @f()"));
    }

    #[test]
    fn empty_definition_guard_is_not_equivalence() {
        let normalized = normalize_ir("source_filename = \"x\"\n");
        assert_eq!(
            normalized
                .lines()
                .filter(|line| line.starts_with("define "))
                .count(),
            0
        );
    }

    #[test]
    fn integer_add_commutativity_is_canonicalized() {
        let left = normalize_ir("define i32 @f(i32 %a, i32 %b) {\n%1 = add i32 %a, %b\n}");
        let right = normalize_ir("define i32 @f(i32 %a, i32 %b) {\n%1 = add i32 %b, %a\n}");
        assert_eq!(left, right);
    }

    #[test]
    fn absent_expected_definition_is_rejected() {
        assert!(!has_expected_definition("define i32 @other()", "target"));
        assert!(has_expected_definition(
            "@target = unnamed_addr alias i32, ptr @other",
            "target"
        ));
    }
}
