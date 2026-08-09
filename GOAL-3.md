# GOAL 0.3.0: routing, timeouts, and speed

> **Status:** draft (proposed 2026-08-08). Locking is a human act. See Human check.
> **Prerequisite:** [GOAL-2.md](GOAL-2.md) signed-off.
> **Scope decided in [GOAL milestone boundaries + fixture sizing contract](https://github.com/SulthanZahran1/rust_mutant/issues/8) and [CLI surface contract](https://github.com/SulthanZahran1/rust_mutant/issues/9):** per-test coverage routing, adaptive timeouts, bounded parallel scheduling, host-wide CPU/RAM resource coordination, content-addressed cache, incremental mode, and single-mutant debugging. TCE and distribution remain M4.

## Mission

Make mutation runs fast and diagnosable on real Rust projects without weakening classification. Build the per-test coverage map once with stable LLVM instrumentation and cargo-nextest, route each mutant only to tests covering its source line, enforce adaptive per-mutant timeouts, execute scratch copies in parallel, cache exact results, support changed-file runs, and expose a reproducible single-mutant mode.

The measured reference is two cores, 7.8 GiB RAM, rustc/cargo 1.97.1, cargo-nextest 0.9.143, and cargo-llvm-cov 0.8.7. The routing research measured about 3.5 seconds of one-time setup and 0.39–0.61 seconds per routed mutant. These are calibration facts, not promises for every crate.

## Verifiable Acceptance Criteria

### 1. Per-test coverage routing

Build a coverage map with `-C instrument-coverage`, per-test `LLVM_PROFILE_FILE` outputs, `llvm-profdata merge`, and `llvm-cov export`. Store file identity as project-root-relative forward-slash paths so Windows and Linux share the same lookup key. Use cargo-nextest for per-test process isolation.

**Test:** run the medium fixture with routing enabled and with an explicit full-suite comparison mode; inspect each mutant's `testsRun` and hand-check at least one source line against the coverage map.

**Pass:** every routed mutant runs all and only the tests mapped to its line; a mutant killed by the full-suite comparison is also killed by the routed run; the medium run reduces test executions against the full-suite mode; no source path is stored as a platform-specific absolute path.

### 2. Adaptive timeouts

Derive each mutant's timeout from the baseline duration of its covering tests with a documented coefficient and a safety floor. Enforce the timeout through nextest's process termination configuration. A runaway mutant is `timeout`, never `survived`.

**Test:** medium fixture contains 2–3 deliberate loop-bound timeout mutants; run with the default adaptive timeout and with a deliberately low timeout override.

**Pass:** only the designed runaway cases time out under the default; the low override terminates without hanging the runner; timeout counts are stable across two runs.

### 3. Bounded parallel scheduler

Run independent scratch copies through a bounded Rayon scheduler controlled by `--parallel N` and the host-wide resource governor. `--parallel N` is only a per-session upper bound. Report rows and aggregate counts are deterministic regardless of scheduling.

**Test:** run the large fixture with `--parallel 1` and `--parallel 8` on the reference two-core box, launch two mutation sessions concurrently, and inspect resource receipts while excluding timing fields from the JSON comparison.

**Pass:** JSON mutant records and aggregate counts are byte-identical after timing fields are removed; on a two-effective-CPU host the default and `--parallel 8` both select one global mutant worker; a second session waits rather than increasing aggregate workers; nested Cargo jobs do not multiply outer parallelism.

The global CPU budget is `max(1, floor(0.75 * effective_cpu_budget))`, where effective capacity comes from process affinity or cgroup quota. All sessions claim slots from one crash-released semaphore. A global memory guard reserves at least 25% for the rest of the system, tracks registered session/child RSS, pauses new workers under pressure, and reports throttling without changing mutant classification. `--max-memory <MiB>` can lower the budget but cannot raise the safety ceiling.

### 4. Content-addressed cache

Cache results by source hash, test-binary/toolchain identity, operator selection, runner flags, and engine/schema version. A cache hit must be distinguishable from a fresh execution. Any change to a relevant input invalidates the affected result.

**Test:** run the large fixture cold, rerun without changes, then change one source file and rerun; parse cache-hit fields and compare JSON results.

**Pass:** the warm run is under 30 seconds, has non-zero cache hits, and matches the cold result apart from timing fields; the changed-file run re-executes affected mutants and does not reuse stale results.

### 5. Incremental mode

`--incremental --base-ref <git-ref>` restricts discovery to changed source files while retaining compatible cached results for the complete report.

**Test:** commit the large fixture, change one source file, and run with `--incremental --base-ref HEAD~1`.

**Pass:** only changed-file mutants execute fresh; unchanged results are explicitly marked cached; the final totals and MSI cover cached plus fresh records.

### 6. Single-mutant debugging

A user can select one mutant by stable id or discovery index and see its source span, family, subtype, patch, selected tests, command, outcome, and failure/compile details.

**Test:** select a medium-fixture mutant from the full JSON listing, run it alone twice, and compare the records.

**Pass:** exactly one mutant runs; its identity matches the listing; the classification and patch are reproducible; the original source tree remains pristine.

### 7. Performance receipt and scope discipline

The tool records routing setup time, per-mutant execution time, cache hits, requested and effective parallelism, global CPU budget, active sessions, memory budget, peak RSS, resource throttling, and total wall time. External benchmark targets remain provisional until the map specifies them.

**Test:** parse the JSON receipt from small, medium, and large runs and compare it with the measured cost model in `docs/research/coverage-routing.md`.

**Pass:** all timing and cache fields are internally consistent; the receipt distinguishes routing, execution, TCE-free mutation time, and cache time; no unsupported benchmark superiority claim is made.

## Implementation Rules

- Routing correctness is more important than routing speed. The "routing never kills" invariant is a hard regression test.
- Use source-only scratch copies per worker. In-place patch and restore is serial-only and is not the parallel implementation.
- `cargo test` remains the baseline gate; cargo-nextest is required for routing and timeout execution.
- Cache keys and coverage-map formats are internal in M3 but carry a version field from day one. M4 freezes externally visible schema fields.
- Sort all output independent of scheduling.
- `--parallel` never bypasses the host-wide CPU/RAM governor. Nested Cargo execution consumes the same budget through one build job per outer worker or a shared jobserver.
- Resource receipts identify requested workers, effective workers, global budget, active sessions, memory ceiling, peak RSS, and any wait/throttle interval.
- Fixture-mutating integration tests acquire a shared serial lock and run with one test thread.
- Commit each speed feature atomically with fixture-level regression tests and measured receipts.

## Human check

**Type:** signed-off live demo.

The agent runs live in front of the human:

1. Routing on versus full-suite comparison on the medium fixture, including a hand-checked line and the routing-never-kills result.
2. Adaptive timeout cases and a deliberately low timeout override that terminates safely.
3. Serial versus parallel large-fixture output and timings.
4. Two concurrent sessions on the two-effective-CPU reference host, demonstrating one aggregate worker and memory-safe waiting.
5. Cold run, warm cache run, changed-file incremental run, and one `--mutant` debug run.
6. A real Rust project chosen by the human, while treating the external benchmark set as pending map scope.

**Sign-off:** the human confirms the M3 criteria in-session; the document can then be marked signed-off.

**Failure:** incorrect routing, stale cache results, or nondeterministic output is reworked without weakening criteria. Borderline timing is reworked within the stated hardware assumptions; a core premise failure requires explicit renegotiation.
