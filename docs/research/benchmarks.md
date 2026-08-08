# Benchmark targets + cargo-mutants timings

> **Status:** research finding (wayfinder ticket #10, 2026-08-08). Measured on the dev box: 2 cores (srv1173528), 7.8 GiB RAM, rustc/cargo 1.97.1, cargo-mutants 27.1.0. All wall clocks are single-run measurements on the stated box.

## Candidate viability

| Repo | Stars (2026-08-08) | LOC (src) | Packages | `cargo test --no-run` (cold, 2 cores) | Clone size |
|---|---|---|---|---|---|
| **uuid** (uuid-rs/uuid) | ~1.6k | ~2.3k | 1 | **1:58** | small |
| **anyhow** (dtolnay/anyhow) | ~6.5k | ~1.5k | 1 | **1:50** | small |
| **thiserror** (dtolnay/thiserror) | ~5.5k | ~1.2k | 1 | **1:26** | small |
| serde / clap / axum / regex | — | larger | multi | not measured (too big for the 2-core box) | — |

All three small targets are viable: single package, no cgo, cold `--no-run` under 2 minutes. serde/clap/axum/regex are too heavy for this box's benchmark budget — defer to a beefier machine or the large-fixture gate.

## cargo-mutants measured timings (uuid, 2 cores, `-j2`)

Run: `cargo mutants` in a fresh clone of uuid (v1.24.0). **Invocation pitfall: cargo-mutants is a cargo subcommand — the bare binary at `~/.cargo/bin/cargo-mutants` refuses to run directly (prints usage, exit 1). Must be invoked as `cargo mutants`.**

| Metric | Value |
|---|---|
| Mutants generated | 974 |
| Mutants tested (run interrupted at batch end) | 291 |
| Caught (killed) | 158 |
| Missed (survived) | 50 |
| Unviable (compile error) | 83 |
| Timeout | 0 |
| Wall clock for 291 mutants | ~23 min (~4.7s/mutant wall, 2 jobs) |
| Per-mutant build phase | 1–3s (incremental, from debug.log) |
| Per-mutant test phase | 0.15–0.5s (whole suite, from debug.log) |
| Baseline `--no-run` | 1:58 cold |

**The headline number**: ~4.7s wall per mutant on a 2.3k-LOC crate with 2 parallel jobs — the full 974-mutant run would take ~75 min on this box. cargo-mutants' whole-suite-per-mutant approach is the incumbent's cost; per-test routing (see `coverage-routing.md`) is the gap rust_mutant sells against.

## The sambungapi dogfood corpus (primary benchmark target)

MetatechID/sambungapi (private) — Rust axum + rusqlite wire-compatible Composio impostor for Bella. Repo-level facts only (private repo; no contents in this public doc):

| Metric | Value |
|---|---|
| src/ LOC | 10,756 (google/executor.rs 2,558; catalog.rs 2,063; routes.rs 1,473; hot/executor.rs 1,388; ms/executor.rs 669; google/provider.rs 648; store.rs 578) |
| tests/ LOC | 2,009 (contract.rs 446, google.rs 479, smoke.rs 98, common/mod.rs 78 + unit scope.rs) |
| Test shape | 3 integration files booting the full axum app in-process (tower::ServiceExt, in-memory SQLite) + 1 unit file; no real sockets |
| Status | GOAL-0.1 + GOAL-1.0 signed-off; GOAL-2.0 WIP (ms/hot modules uncommitted, currently does not compile — E0308 in routes.rs:514) |

**Implication for the mutation gate (sambungapi map #3)**: the in-process test harness means every mutant runs the full app-boot suite — routing matters even more here, because the app-boot tests are the expensive ones. The gate's scope decision (core logic: store/ids/errors/catalog/scope/routes vs provider executors) directly controls mutant count: catalog.rs alone (2,063 LOC) will generate hundreds of mutants.

## Recommended benchmark target set (README claims)

1. **sambungapi** — the dogfood corpus, primary claim ("runs on a real 10k-LOC service").
2. **uuid** — the small-crate claim, with cargo-mutants comparison (291+ mutants, ~4.7s/mutant vs rust_mutant's routed cost).
3. **anyhow or thiserror** — second small-crate data point.
4. Defer serde/clap/axum to the large-fixture gate (M2/M3 demo on a beefier box).
