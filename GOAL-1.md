# GOAL 0.1.0: rust_mutant engine skeleton

> **Status:** draft (proposed 2026-08-08). Locking is a human act. See Human check.
> **Prerequisite:** none.
> **Scope decided in [GOAL milestone boundaries + fixture sizing contract](https://github.com/SulthanZahran1/rust_mutant/issues/8):** M1 is the end-to-end engine skeleton plus the 10 generic table-stakes families. No Rust-idiomatic families, speed features, TCE, reports, or distribution yet.

## Mission

Build a working Rust mutation pipeline. `rust-mutant --path <crate>` parses Rust with tree-sitter-rust, discovers generic mutation points, applies each mutant to a source-only scratch copy, runs the crate tests, and classifies every result as `killed`, `survived`, `not_covered`, `compile_error`, or `timeout`. The source project remains byte-identical before and after the run.

## Verifiable Acceptance Criteria

### 1. Workspace and provisional CLI

A Cargo workspace contains a core library and a `rust-mutant` binary. The binary accepts a project path and exposes the provisional M1 subset of the CLI contract. The full contract is documented by the CLI decision and freezes in M4.

**Test:** `cargo build --workspace`; `cargo run --release -- --help`; `cargo run --release -- --path /does/not/exist`.

**Pass:** the workspace builds; help prints the documented provisional flags; a missing path exits non-zero with an actionable error.

### 2. Generic discovery

Discovery covers the locked 10 generic table-stakes families: AOR, AOD, AOI, ROR, LOR, LCR, COR, SDL, RVR, and loop increment/decrement. Each discovered point carries a stable mutant id, file identity, line, column, family, subtype, original text, and replacement text.

**Test:** `rust-mutant --path tests/fixtures/operator-probes --dry-run --json`, `rust-mutant --path tests/fixtures/small --dry-run --json`, and a parser/operator unit-test suite.

**Pass:** every generic family produces at least one point in the dedicated operator-probes fixture; the mixed-outcome small fixture is not required to cover all ten families; JSON fields are present; source spans are valid UTF-8 boundaries and point at the expected original text.

The operator-probes fixture is a discovery/application probe suite, not a second full mutation campaign. It keeps M1's ten-family coverage explicit without forcing the 12–15 mutant outcome fixture to carry every operator combination.

### 3. Source-only application

Each mutant is applied to a per-worker scratch copy containing source and required manifest/config files, excluding `.git` and `target`. The original project is never patched in place.

**Test:** hash every source and manifest file before and after a full run; inspect `git status --porcelain` in the fixture.

**Pass:** all hashes are identical after the run and the fixture working tree is clean, including after a failed or timed-out mutant.

### 4. Five-way classification

Every mutant receives exactly one initial outcome: `killed`, `survived`, `not_covered`, `compile_error`, or `timeout`. M1 may use a fixed documented timeout; adaptive timeouts are M3.

**Test:** run the M1 fixture with JSON output and manually hand-apply one mutant from each designed outcome.

**Pass:** every mutant is classified exactly once and `killed + survived + not_covered + compile_error + timeout == total`; the hand-checked outcomes match the runner.

### 5. Console and provisional JSON report

The default report prints total mutants, per-outcome counts, MSI, and wall time. JSON contains the same totals plus per-mutant records. MSI is `killed / (killed + survived)` and excludes `not_covered`, `compile_error`, and `timeout`.

**Test:** compare the console totals with a parsed JSON run and a small independent counting script.

**Pass:** totals agree, counts sum to total, MSI is numerically correct, and a repeat run is identical apart from explicitly documented timing fields.

### 6. Small outcome fixture

The M1 fixture uses safe generic constructs and contains 12–15 mutants deliberately spanning all five initial outcome buckets. It is an engine-classification smoke fixture, not a pure 100%-killed correctness fixture and does not require zero compile errors.

**Test:** run the real release binary against `tests/fixtures/small` on the reference two-core Linux box after a baseline `cargo test`; hand-check one representative mutant from each outcome bucket.

**Pass:** 12–15 mutants, `killed`, `survived`, `not_covered`, `compile_error`, and `timeout` are all present and correctly classified, counts sum to total, the baseline is green, and cold wall time is no greater than 20 seconds.

## Implementation Rules

- Use tree-sitter-rust with conservative node guards. Do not add a rust-analyzer dependency.
- Treat compiler rejection as `compile_error`, not as a fabricated behavioral result.
- Use `cargo test` as the baseline runner. Whole-suite execution is acceptable in M1; nextest routing is M3.
- Use a fixed safety timeout in M1 and document its value; M3 replaces it with an adaptive nextest-based timeout.
- Sort discovery and report rows deterministically.
- Every operator has a before/after unit test and an integration assertion against the real binary.
- Run fixture integration tests serially and verify the fixture baseline before mutation.
- Commit each verified piece atomically with tests, formatting, and lint.

## Human check

**Type:** signed-off live demo.

The agent runs live in front of the human:

1. `cargo build --release` and `rust-mutant --help`, showing real exit codes.
2. The small fixture baseline, dry-run JSON, and full JSON run, including counts and the <=20s timing gate.
3. One hand-applied representative mutant from each of the five outcome buckets in a scratch copy, demonstrating that the observed test result maps to the reported classification.
4. A real small Rust crate chosen by the human, with the original tree hash verified before and after.

**Sign-off:** the human confirms the M1 criteria in-session; the document can then be marked signed-off.

**Failure:** small failures are reworked without changing criteria. A core failure, such as source-tree corruption or a classification mismatch on real code, requires explicit renegotiation before another demo.
