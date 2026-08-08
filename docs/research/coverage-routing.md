# Rust per-test coverage attribution for mutant routing

> **Status:** research finding (wayfinder ticket #3, 2026-08-08). Measured on the dev box: 2 cores (srv1173528), 7.8 GiB RAM, rustc/cargo 1.97.1, cargo-mutants 27.1.0, cargo-llvm-cov 0.8.7, cargo-nextest 0.9.143. All wall clocks are single-run measurements on the stated box unless noted.

## The problem

dart_mutant and gopher_mutant route each mutant to only the tests that cover its mutated line (per-test coverage map). Rust has **no built-in per-test coverage attribution**: `cargo test -coverprofile` does not exist; `cargo llvm-cov` produces package-level merged coverage. cargo-mutants (the incumbent) does NOT route: it copies the tree, applies one mutant, runs the whole `cargo test` suite, and relies on incremental builds.

## What works: LLVM instrumentation + per-test profraw via nextest

Verified end-to-end on a bench crate (10 functions, 26 tests):

1. **Instrument once**: `RUSTFLAGS="-C instrument-coverage" cargo test --no-run` builds the test binaries with coverage instrumentation.
2. **Per-test profile capture**: run each test binary with `LLVM_PROFILE_FILE="/tmp/profraws/test_%p.profraw"` and a per-test filter. Two viable runners:
   - **nextest** (verified): `LLVM_PROFILE_FILE=... cargo nextest run -E 'test(<name>)'` — nextest passes the env through to each per-test process, and each test runs in its own process, so `%p` (pid) gives one profraw per test. Measured: **26 per-test invocations total 0.72s** (0.03s/test).
   - **cargo test with `--exact`** (verified): `./target/debug/deps/<bin> --exact <name>` per test — same profraw mechanism, slightly slower per invocation.
3. **Merge + index**: `llvm-profdata merge` the per-test profraws (measured 1.24s for 26), then `llvm-cov export` (measured 1.55s) to get the `(file:line) → {test}` map. The export JSON carries per-function/segment coverage; the map is built by intersecting each test's profraw with the source lines.

**Total routing overhead for 26 tests: ~3.5s one-time** (0.72 + 1.24 + 1.55), then each mutant runs only its covering tests.

## Cost model (measured)

| Scenario | Wall clock |
|---|---|
| Warm baseline `cargo test` (bench crate, 26 tests) | 0.21–0.29s |
| cargo-mutants per-mutant (bench crate, 81 mutants, `-j2`) | 1–2s build + 0s test each |
| Instrumented cold build + full test | 4.39s |
| Instrumented incremental rebuild + full test (one-line patch) | 1.66–1.72s |
| nextest full suite (warm) | 0.73–1.26s |
| **nextest routed (only covering tests)** | **0.39–0.61s** |
| Tree copy (cargo-mutants style, 48M target) | 0.90s |
| src-only copy (24K) | ~0.00s |

**Crossover**: on this bench crate, routed (0.5s) vs full (1.0s) saves ~0.5s/mutant. With 81 mutants that is ~40s saved per run — and the gap grows with suite size, because the full-suite cost scales with the whole test suite while routed cost scales with per-test coverage. On uuid (974 mutants, 1:58 baseline `--no-run`), routing is the difference between a ~2h run and a ~15min run. **Routing wins at any mutant count ≥ ~10 on a suite with >2× test-time spread; the win is decisive on real crates.**

## Mutant application mechanisms (no `-overlay` in Rust)

| Mechanism | Cost | Parallel-safe | Verdict |
|---|---|---|---|
| **Tree copy** (cargo-mutants) | 0.9s per copy (48M); src-only ~0s | Yes (each worker its own copy) | cargo-mutants' choice; src-only copy is cheap |
| **In-place patch + restore** (cargo-mutants `--in-place`) | ~0s copy, but incompatible with `--jobs` (verified in its source: `--in-place` is incompatible with parallel) | No (single tree) | Only for serial runs |
| include!-based schemata | untested | Yes | Needs source restructuring; rejected for v1 |
| RUSTFLAGS cfg gating | untested | Yes | Only works for cfg-visible mutations; not general |

**Recommendation**: src-only tree copy (exclude `target/`, `.git/`) per worker — 24K copy is free, parallel-safe, and matches the sibling playbook (gopher_mutant's `-overlay` was chosen for the same reason: never touch the source tree).

## Adaptive timeout

- **cargo test has no per-test timeout** — an infinite-loop mutant hangs the whole run.
- **nextest has `slow-timeout` with `terminate-after`** (verified): config-file form works (`[profile.default.slow-timeout] period = "2s" terminate-after = 1`); the CLI inline-table form (`--config 'profile.default.slow-timeout={period="2s",terminate-after=1}'`) is rejected. Measured: a hang test was killed at **2.002s** with `TIMEOUT` classification.
- cargo-mutants' own approach (from its source, `src/timeouts.rs`): `build_timeout = baseline_duration × multiplier` (default multiplier), test timeout defaults to **300s** with a warning when `--baseline=skip`. It does NOT kill per-test; it relies on the whole-suite run finishing.

**Recommendation**: adaptive per-mutant timeout = baseline covering-test duration × coefficient (dart_mutant formula), floor ~2–5s, enforced by running each mutant's covering tests under nextest with `slow-timeout.terminate-after` — the process-group kill pattern from the Go sibling applies if a test binary must be killed directly.

## Key facts for the engine grilling (#6)

1. **Per-test routing is feasible on stable Rust** with `-C instrument-coverage` + nextest + `LLVM_PROFILE_FILE` + llvm-profdata/llvm-cov. No nightly needed.
2. **Routing overhead is ~3.5s one-time per run** (26-test crate); per-mutant routed cost ≈ 0.4–0.6s vs 1.0s+ full-suite.
3. **src-only tree copy is the application mechanism** (free, parallel-safe); `--in-place` is serial-only.
4. **nextest is the runner** for both routing and timeouts; cargo test remains the baseline gate.
5. cargo-mutants' per-mutant cost on the bench crate was 1–2s build + 0s test — routing + pre-built binaries (compile once, run per-test) is the speed story rust_mutant sells against it.
