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
| Mutants tested | 974 (complete run) |
| Caught (killed) | 291 |
| Missed (survived) | 471 |
| Unviable (compile error) | 198 |
| Timeout | 14 |
| Wall clock for full run | **4:02:44** (2 jobs; `/usr/bin/time`) |
| Process CPU | 2,821.84s user + 3,617.94s system; 44% CPU |
| Maximum RSS | 342,844 kB |
| Baseline `--no-run` | 1:58 cold |

**The headline number**: the complete run took **4:02:44 wall** for 974 mutants, or **14.95s per generated mutant** with 2 parallel jobs. The earlier ~23-minute / 291-mutant snapshot was partial and is superseded by this full result. cargo-mutants' whole-suite-per-mutant approach is the incumbent's cost; per-test routing (see `coverage-routing.md`) is the gap rust_mutant sells against.

## The sambungapi dogfood corpus (primary benchmark target)

MetatechID/sambungapi (private) — Rust axum + rusqlite wire-compatible Composio impostor for Bella. Repo-level facts only (private repo; no contents in this public doc):

| Metric | Value |
|---|---|
| src/ LOC | 10,756 (google/executor.rs 2,558; catalog.rs 2,063; routes.rs 1,473; hot/executor.rs 1,388; ms/executor.rs 669; google/provider.rs 648; store.rs 578) |
| tests/ LOC | 2,009 (contract.rs 446, google.rs 479, smoke.rs 98, common/mod.rs 78 + unit scope.rs) |
| Test shape | 3 integration files booting the full axum app in-process (tower::ServiceExt, in-memory SQLite) + 1 unit file; no real sockets |
| Status | GOAL-0.1 + GOAL-1.0 signed-off; GOAL-2.0 WIP (ms/hot modules uncommitted, currently does not compile — E0308 in routes.rs:514) |

**Implication for the mutation gate (sambungapi map #3)**: the in-process test harness means every mutant runs the full app-boot suite — routing matters even more here, because the app-boot tests are the expensive ones. The gate's scope decision (core logic: store/ids/errors/catalog/scope/routes vs provider executors) directly controls mutant count: catalog.rs alone (2,063 LOC) will generate hundreds of mutants.

## rust-mutant measured timings (uuid, same box, routed defaults)

> **Status:** first real-crate receipt (ticket #31, 2026-09-25). Same box and same pinned commit as the cargo-mutants numbers above, so the two are directly comparable.

Run: `rust-mutant --path . --format json --quiet` in `/home/dev/bench/uuid-v1.24.0` (uuid v1.24.0 @ `6a8aeab3d02838f6fef71e69cdfda963e8c4158b`, tree clean before and after). Harness `bench-uuid.sh`; artifacts in `/home/dev/bench/receipt-fixed/`. Binary `7ad67c066bcbb898c94a14860fbcca51519a8d37d696c716ae43e8d7359f5fec` (rust-mutant 1.0.1, `main` @ `e2b21faa`), rustc 1.97.1, cargo-nextest 0.9.143, llvm-tools from the stable sysroot, 2 cores / 7.9 GiB.

| Metric | rust-mutant | cargo-mutants | Notes |
|---|---|---|---|
| Mutants generated | 820 | 974 | different operator sets — see the caveat below |
| Killed | 122 | 291 (caught) | |
| Survived | 372 | 471 (missed) | |
| Compile error | 258 | 198 (unviable) | |
| Equivalent | 68 (TCE) | — | cargo-mutants has no TCE |
| Timeout | 0 | 14 | |
| MSI | **24.7%** (122/494) | **38.2%** (291/762) | not comparable as-is — see below |
| Wall clock | **40:34** | **4:02:44** | |
| Wall per mutant | **2.96s** | 14.95s | **5.0x faster** |
| Workers | 1 (`effectiveWorkers`) | 2 (`-j2`) | |
| CPU-normalised per mutant | **3.0 cpu-s** | 29.9 cpu-s | **10.1x** |
| Peak RSS | 459 MiB | 343 MiB | |
| Cache hits | 0 (cold, fresh clone) | — | |
| Routing | llvm-cov, 63 tests discovered, 4768 mapped lines | no routing (whole suite per mutant) | |

Phase split: routing 181.4s (7.5%), execution 2249.7s (92.5%), TCE 414.1s (17.0%), cache 0.1s. TCE is a large slice of execution and is pure MSI-hygiene (68 provably-equivalent mutants excluded); `--no-tce` is the lever if a faster wall clock matters more than a clean denominator.

**The headline**: routed execution does what it was built to do — **5.0x faster wall-per-mutant and 10.1x on CPU-normalised terms** than the incumbent on the same crate and box, while classifying 68 equivalent mutants the incumbent cannot. Per-mutant cost is roughly constant instead of scaling with suite size.

### This receipt is NOT an apples-to-apples MSI comparison

Three differences make the MSI column misleading if read naively. They are recorded here rather than smoothed over:

1. **rust-mutant mutates test code; cargo-mutants does not.** 445 of the 820 mutants (54%) sit inside `#[cfg(test)]` modules. Verified against the incumbent's own list: `cargo mutants --list` on uuid emits 974 mutations and **zero** fall inside a `#[cfg(test)]` module (highest `src/fmt.rs` line listed is 1139; that file's test module starts at 1238). Restricted to production code, rust-mutant generates 375 mutants and scores **49.2% (95/193)**. Whether mutating test bodies is desirable is an open product decision — it is currently a consequence of discovery, not a stated choice.
2. **cargo-mutants runs doctests; nextest does not.** uuid has ~70 doctests against 63 nextest tests, and the sets are disjoint. A mutant whose only kill is a doctest survives a routed run by construction (tracked separately). This biases the comparison against rust-mutant.
3. **Different operator sets.** 820 vs 974 generated mutants — rust-mutant applies Rust-idiomatic operators (`?` removal, unwrap/expect, `&mut`→`&`, clone removal, iterator chains) that cargo-mutants does not implement, so the denominators are not the same population.

**Reading the numbers honestly**: use the wall-clock/CPU ratios as the routing claim; treat MSI as "24.7% over all mutants, 49.2% over production mutants" and the incumbent's 38.2% as a different population, not a ranking.

### First-run artefacts that were fixed on the way to this number

The first attempt at this receipt (2026-09-11, `/home/dev/bench/receipt/`) recorded `killed: 0 / survived: 595 / msi: 0.0` — an artefact of discovery only finding integration suites (1 test discovered). That is fixed (see #41, PR #42) and this receipt supersedes it. Two known limitations remain and are called out above and in the tracker: doctest execution (issue #43) and truncated `testsRun` labels for tests 1–9 in groups of 10+ (issue #44) — the latter affects only the label arrays, not status, buckets, MSI or wall clock.

## Recommended benchmark target set (README claims)

1. **sambungapi** — the dogfood corpus, primary claim ("runs on a real 10k-LOC service").
2. **uuid** — the small-crate claim, with the complete cargo-mutants comparison (974 generated: 291 caught / 471 missed / 198 unviable / 14 timeouts; 4:02:44 wall on 2 cores).
3. **anyhow or thiserror** — second small-crate data point.
4. Defer serde/clap/axum to the large-fixture gate (M2/M3 demo on a beefier box).
