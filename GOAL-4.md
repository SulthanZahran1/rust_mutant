# GOAL 1.0.0: TCE, reports, contract, and distribution

> **Status:** ✅ **signed-off** (locked 2026-08-09, signed-off 2026-08-09 — M4 criteria verified in-session by human). See Human check.
> **Prerequisite:** [GOAL-3.md](GOAL-3.md) signed-off, plus [the CLI surface contract](https://github.com/SulthanZahran1/rust_mutant/issues/9) resolved before contract freeze.
> **Scope decided in [GOAL milestone boundaries + fixture sizing contract](https://github.com/SulthanZahran1/rust_mutant/issues/8):** LLVM-IR TCE, Stryker JSON/JUnit/HTML reports, threshold gate, agent-friendly JSON, Windows, crates.io, Homebrew, installer, release artifacts, and Linux/Windows CI.

## Mission

Ship rust_mutant 1.0.0 as a stable, installable, machine-verifiable Rust mutation-testing tool. TCE removes only provably equivalent survivors through normalized stable LLVM IR. A single result model feeds console, Stryker JSON, JUnit XML, and self-contained HTML reports. The final CLI and JSON contract is frozen, threshold failures are CI-safe, and users can install verified Linux and Windows artifacts through the documented release paths.

## Verifiable Acceptance Criteria

### 1. One-sided LLVM-IR TCE

For every surviving mutant, automatically compile the original and scratch-tree mutant through Cargo in debug mode with stable LLVM IR emission, `-C opt-level=2`, `-C debuginfo=0`, and `--remap-path-prefix`. `--no-tce` disables this post-survival analysis for a run. Normalize only known path, codegen-unit, panic-location, allocation-name, and test type-hash noise. An empty or missing expected function definition is an error, never evidence of equivalence.

**Test:** run a 12-case soundness fixture containing six deliberately equivalent and six must-differ mutants, including a commutativity case and a multi-file case, once with default settings and once with `--no-tce`.

**Pass:** default mode detects all six equivalent cases; zero must-differ cases classify as `equivalent`; `--no-tce` bypasses equivalence classification without changing ordinary mutant outcomes; a killable mutant is never marked equivalent; the result records TCE cost when enabled and excludes `equivalent` from MSI. TCE runs through Cargo debug mode, never direct release rustc.

### 2. Reports from one result model

The same in-memory result model produces console, Stryker mutation-testing-elements JSON, JUnit XML, and a self-contained HTML report. Every record includes stable mutant id, file, line, column, family, subtype, original/replacement text, status, tests run, duration, compile details, and equivalence details where applicable.

**Test:** run the medium fixture with all report formats; validate JSON with the repository schema/checker, parse JUnit with a standard XML parser, and inspect the HTML for external asset references.

**Pass:** Stryker JSON validates; JUnit totals equal the mutant totals; HTML is a single self-contained file with no external HTTP or HTTPS assets; all formats agree on counts, statuses, MSI, and per-family summaries.

### 3. Frozen CLI and JSON contract

The resolved CLI contract in [`docs/cli-contract.md`](docs/cli-contract.md) defines the `rust-mutant --path .` invocation, final flags, configuration precedence, error behavior, report formats, resource governor, exit codes, and JSON schema. `schemaVersion` is present and breaking changes require a major version bump.

**Test:** run the contract integration suite against help, valid runs, invalid paths, empty projects, threshold pass/fail, invalid operator selections, concurrent-session resource limits, and each report format.

**Pass:** the implementation, contract document, and integration assertions agree; two identical runs diff cleanly after stripping explicitly variable timing and generated-at fields; the schema version is present and correct.

### 4. Threshold and CI gate

A threshold option evaluates MSI after TCE exclusion and exits according to the frozen contract. The report explains the denominator and all excluded buckets instead of hiding them.

**Test:** run the medium fixture below and above its threshold, including a no-mutants case and an invalid-project case.

**Pass:** exit codes exactly match `docs/cli-contract.md`; threshold behavior is tested in integration and CI; `compile_error`, `timeout`, `not_covered`, and `equivalent` remain visible and are not silently treated as kills or survived mutants.

### 5. Linux and Windows support

Build and test the same commit on Linux and Windows MSVC. Coverage and report file identities use project-root-relative forward slashes. Windows path, process termination, scratch-tree cleanup, and report parsing are tested rather than assumed from Linux.

**Test:** GitHub Actions runs the fixture suite and CLI contract on both operating systems; a Windows fixture run is inspected for non-empty coverage attribution and clean restoration.

**Pass:** Linux and Windows CI are green; Windows does not collapse into all-`not_covered`; scratch trees and reports are cleaned up; JSON is structurally identical apart from environment and timing fields.

### 6. Distribution floor

The release process publishes:

- Linux release binaries and Windows x86_64 MSVC artifacts.
- Checksums for every release artifact.
- A crates.io package published from a tag.
- A Homebrew formula or tap entry.
- An installer that downloads the platform artifact, verifies its checksum, and falls back to a user-local bin directory when needed.
- GitHub release notes with version and supported platforms.

**Test:** run the release workflow on a draft tag; install from the release path in a clean environment; run `rust-mutant --version` and a small fixture.

**Pass:** artifacts, checksums, crates.io, Homebrew, and installer paths all resolve to the tagged version; the clean-environment binary runs the small fixture; Linux and Windows release jobs are green.

### 7. README and real-project proof

README documents the differentiation, all 18 families, fixture gates, installation paths, CLI contract, outcome semantics, and links to the research basis. The final demo includes a real Rust project selected by the human. External benchmark target claims remain subject to the separate map decision.

**Test:** run a documentation consistency check for all 18 family ids, the `schemaVersion`, installation commands, and report names; execute the selected real-project run.

**Pass:** documentation matches the frozen contract and the real-project run produces a valid report without source-tree mutation; no unresolved benchmark claim is presented as a locked guarantee.

## Implementation Rules

- Contract freeze is a hard gate. After M4 locks, CLI flags, exit codes, JSON fields, and status meanings require explicit human renegotiation for breaking changes.
- TCE is automatic after survivors unless `--no-tce` is supplied. It is one-sided: identical normalized IR may be marked `equivalent`; different IR remains a live/survived result; never guess equivalence from a failed or empty build.
- Keep one result model for every report format.
- Preserve M1–M3 fixture and routing regression tests on Linux and Windows.
- Release artifacts are reproducible from a pinned Rust toolchain and carry checksums.
- Do not add rust-analyzer as a 1.0.0 requirement.
- Commit each release feature atomically with tests, formatting, lint, and CI evidence.

## Human check

**Type:** full signed-off release demo.

The agent runs live in front of the human:

1. TCE soundness fixture with equivalent and must-differ cases, including the empty-IR guard.
2. All report formats, schema validation, JUnit parsing, and HTML inspection.
3. CLI contract and threshold exit-code matrix.
4. Linux and Windows CI receipts, including Windows coverage attribution.
5. Clean-environment installer, crates.io, Homebrew, and release-artifact checksums.
6. A real-project run selected by the human, with source-tree hashes and the final reports reviewed.

**Sign-off:** the human confirms the M4 criteria in-session; the document can then be marked signed-off, the map can close, and the project can tag 1.0.0.

**Failure:** release or report defects are reworked without weakening the frozen contract. A false-equivalence result, broken Windows coverage, or unusable installation path is a core-premise failure and requires explicit renegotiation before re-demo.
