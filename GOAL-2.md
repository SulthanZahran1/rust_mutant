# GOAL 0.2.0: Rust-idiomatic operators and fixtures

> **Status:** draft (proposed 2026-08-08). Locking is a human act. See Human check.
> **Prerequisite:** [GOAL-1.md](GOAL-1.md) signed-off.
> **Scope decided in [Rust-idiomatic operator set](https://github.com/SulthanZahran1/rust_mutant/issues/7) and [GOAL milestone boundaries + fixture sizing contract](https://github.com/SulthanZahran1/rust_mutant/issues/8):** all 18 public families, three Rust-calibrated fixtures, and deliberate five-bucket classification. No TCE or release distribution yet.

## Mission

Complete the 1.0.0 operator inventory and prove it against real fixture projects. M2 adds the eight Rust-idiomatic families to M1's ten generic families, exposes operator filtering, and establishes the small, medium, and large fixture contract. Syntax-first generation remains the rule: tree-sitter prevents obvious scope errors, while the compiler classifies semantically invalid mutants as `compile_error`.

## Verifiable Acceptance Criteria

### 1. Complete public operator inventory

`rust-mutant --list-operators` reports exactly 18 public families:

- Generic: AOR, AOD, AOI, ROR, LOR, LCR, COR, SDL, RVR, loop increment/decrement.
- Rust-idiomatic: `?` removal, `unwrap()`/`expect()` removal, `.await` removal, `move`-closure removal, `&mut` to `&`, `.clone()` removal, Arc/Rc swap, and iterator-chain mutation.

Replacement directions and method-specific strategies use stable subtype ids and do not increase the family count.

**Test:** run `--list-operators --json` and assert the set of family ids; run one source-pair test per family and subtype strategy.

**Pass:** exactly 18 public family ids are present, every family has a real mutation test, and no deferred candidate such as Box swaps, `if let`, unsafe removal, derive removal, or loop/while changes is presented as a 1.0.0 family.

### 2. Rust-idiomatic discovery and application

The eight idiomatic families operate on the verified tree-sitter node shapes from the grammar research. Smart-pointer mutation is Arc/Rc only. Iterator-chain mutation starts with `.map(...)`, `.filter(...)`, and `.collect(...)` transformations.

**Test:** operator integration tests run the real binary against before/after fixture pairs, then run `cargo check` and the fixture tests for each generated mutant.

**Pass:** every family produces at least one source-changing mutant on a valid construct; no operator mutates comments, string literals, patterns, or unrelated type syntax; compiler-rejected variants are reported as `compile_error`.

### 3. Operator filtering

Users can select a subset of families and subtypes without changing discovery order or ids outside the selection.

**Test:** `rust-mutant --path tests/fixtures/medium --operators ROR,await-removal --json`; run an invalid operator name and an empty selection.

**Pass:** output contains only the requested families/subtypes; invalid names fail clearly; an empty selection does not silently run all operators.

### 4. Small fixture: outcome smoke test

The [canonical small outcome fixture](GOAL.md#canonical-fixture-roles) follows GOAL-1's amended mixed-outcome contract. It remains deliberately small and need not exercise all 18 families; M1's separate [generic operator-probes suite](GOAL.md#canonical-fixture-roles) already covers the ten generic families. The small fixture must expose every initial outcome bucket so the engine's classification surface stays cheap to verify.

**Test:** baseline `cargo test` followed by the real release binary against `tests/fixtures/small`; compare each bucket with the hand-checked M1 probes.

**Pass:** 12–15 mutants, all five initial outcome buckets are present and correctly classified, counts sum to total, and cold wall time is no greater than 20 seconds on the reference two-core Linux box.

### 5. Medium fixture: complete inventory and all buckets

The medium fixture contains 60–80 mutants, all 18 families, and deliberate examples of `killed`, `survived`, `timeout`, `not_covered`, and `compile_error`.

**Test:** `rust-mutant --path tests/fixtures/medium --json`; parse the per-family and per-status distributions; verify the baseline before the run.

**Pass:** 60–80 mutants; every family produces at least one mutant; every five-bucket status is present; counts sum to total; cold wall time is no greater than 90 seconds.

### 6. Large fixture: scale and realistic MSI

The large fixture is a multi-file Rust project with 300 or more mutants and realistic tested behavior.

**Test:** run the real binary twice against `tests/fixtures/large`, first cold and then warm after M3 cache support is available.

**Pass:** 300+ mutants, all five pre-TCE buckets present, MSI between 55% and 75%, and cold wall time under 10 minutes. The fixture is retained for M3's warm-cache gate, which must be under 30 seconds.

### 7. Compile-error budget

Compile errors remain visible and are not silently counted as kills. The fixture contract has an explicitly designed compile-error bucket in the small fixture and a bounded invalid-mutant rate for the complete inventory.

**Test:** sum `compile_error` across medium and large JSON outputs and compute the rate; print the per-family breakdown.

**Pass:** the small fixture's explicitly designed `compile_error` bucket is explicit and visible; medium plus large is no greater than 10% aggregate; every family has at least one compile-valid mutant; MSI excludes `compile_error`, `not_covered`, and `timeout` from its denominator.

### 8. Fixture harness integrity

Fixtures have green baselines, assert original behavior rather than mutant behavior, and run serially because the tool mutates scratch projects on disk.

**Test:** run the baseline suite before mutation, run the integration harness with one test thread, and compare source hashes after an interrupted-mutant recovery test.

**Pass:** baseline is green; no test encodes mutant behavior; no concurrent fixture run corrupts another run; scratch and source trees are pristine after completion.

## Implementation Rules

- Follow the locked 18-family enumeration. Do not copy a sibling's operator list when it differs.
- Keep operator logic syntax-first and tree-sitter based. Type-aware filtering is post-1.0 work unless the compile-error budget becomes unattainable.
- Record family and subtype ids in provisional JSON from the first implementation so M4 can freeze them without renumbering.
- Keep the terminating semicolon when replacing a Rust method-chain segment.
- Every fixture test must assert original behavior and must call deliberately unasserted functions when designing survivors.
- Run mutation-fixture integration tests serially and use the real binary, not only internal operator unit tests.
- Commit each verified operator and fixture increment atomically with `cargo test`, `cargo fmt --check`, and `cargo clippy`.

## Human check

**Type:** signed-off live demo.

The agent runs live in front of the human:

1. `rust-mutant --list-operators` and the family/subtype JSON listing.
2. A before/after demonstration for each of the eight idiomatic families, including the Arc/Rc and iterator-chain scope limits.
3. Small, medium, and large fixture runs with real counts, status distributions, MSI, compile-error calculation, and timings.
4. The operator-filtering and invalid-name behavior.
5. A real Rust project chosen by the human, without claiming the still-unspecified external benchmark set.

**Sign-off:** the human confirms the M2 criteria in-session; the document can then be marked signed-off. M2 sign-off is the first point at which the sambungapi core-logic mutation gate is admissible.

**Failure:** fixture tuning or isolated operator failures are reworked without weakening criteria. If an idiomatic premise produces only compile errors or vacuous survivors, pause and renegotiate the affected criterion before re-demoing.
