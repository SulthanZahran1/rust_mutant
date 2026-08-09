# rust_mutant release roadmap

> **Status:** planning. Milestone documents are drafts until the human locks each one.
> **Destination:** rust_mutant 1.0.0, a stable Rust mutation-testing tool with the locked 18-family operator contract, measured fixture gates, per-test routing, TCE, machine-readable reports, and a release/distribution path.

## Milestones

| Document | Version | Focus | State | Human check |
|---|---:|---|---|---|
| [GOAL-1.md](GOAL-1.md) | `0.1.0` | Engine skeleton and generic operators | ✅ signed-off | End-to-end mutation run on a small Rust fixture |
| [GOAL-2.md](GOAL-2.md) | `0.2.0` | Rust-idiomatic operators and fixture contract | ✅ signed-off | All 18 families and all fixture buckets live |
| [GOAL-3.md](GOAL-3.md) | `0.3.0` | Routing, timeouts, parallelism, cache, incremental mode | ✅ signed-off | Routing correctness and warm-cache demo |
|| [GOAL-4.md](GOAL-4.md) | `1.0.0` | TCE, reports, frozen contract, distribution | ✅ signed-off | Full release demo on Linux and Windows |

## Locked planning decisions

- [Decide: Rust-idiomatic operator set](https://github.com/SulthanZahran1/rust_mutant/issues/7) — 18 public families: 10 generic table-stakes plus 8 Rust-idiomatic families; syntax-first generation; compiler-classified `compile_error`; Arc/Rc only for smart pointers; map/filter/collect for iterator chains.
- [Decide: GOAL milestone boundaries + fixture sizing contract](https://github.com/SulthanZahran1/rust_mutant/issues/8) — four milestones; Rust-calibrated 12–15 / 60–80 / 300+ fixture gates; M2 unlocks the sambungapi gate; Windows, crates.io, Homebrew, installer, releases, and `schemaVersion` are hard 1.0.0 gates.
- [Decide: rust_mutant CLI surface (agent-friendly contract)](https://github.com/SulthanZahran1/rust_mutant/issues/9) — `rust-mutant --path .`, TOML configuration, pure agent JSON with `schemaVersion: 1`, explicit report formats, exit codes 0–3, and a host-wide CPU/RAM resource governor.

## Research basis

- [Rust landscape](docs/research/landscape.md)
- [Tree-sitter grammar coverage](docs/research/grammar.md)
- [Per-test coverage routing](docs/research/coverage-routing.md)
- [TCE feasibility](docs/research/tce.md)
- [Benchmark measurements](docs/research/benchmarks.md)

## Pending draft amendments

- **GOAL-4 TCE mode:** During milestone discussion, the human chose automatic post-survival LLVM-IR TCE by default, with `--no-tce` as the escape hatch. This supersedes the earlier opt-in wording in the provisional CLI/architecture notes and must be frozen with the final contract in M4.

## Canonical fixture roles

These roles are authoritative across the milestone documents:

- **Small outcome fixture** — `tests/fixtures/small`; a 12–15 mutant M1/M2 classification smoke fixture with all five initial outcome buckets, a green baseline, and a cold run no greater than 20 seconds. It is not required to cover every operator family.
- **Generic operator probes** — `tests/fixtures/operator-probes`; a focused M1 discovery/application suite that exercises every generic family. It is not a second full mutation campaign and does not replace the small outcome fixture.
- **Medium integrated fixture** — `tests/fixtures/medium`; a 60–80 mutant M2 fixture covering all 18 public families and all five initial outcome buckets, with a cold run no greater than 90 seconds.

## Lifecycle

Each milestone follows:

```text
draft -> locked -> signed-off
```

Locking is a human act. A draft becomes locked only when the human confirms that its acceptance criteria are frozen. A signed-off milestone has passed its live demo and its receipts are recorded. Criteria are hard gates: small failures are reworked without weakening the criterion; a failure of the milestone's core premise requires explicit human renegotiation before a new demo.

## How to complete a milestone

1. Read the prerequisite milestone and all linked research.
2. Keep the milestone document in draft until the human locks it.
3. Implement on a branch with atomic commits, tests, lint, and clean source-tree verification.
4. Run every `Test:` command and capture the real output.
5. Run the signed-off live demo in front of the human.
6. Record the sign-off and update this index only after the human confirms it.

The CLI and JSON surface is decided by the CLI contract ticket and remains implementation-provisional through M3. M4 freezes the contract after the implementation, documentation, and integration tests agree.

## Non-negotiables

- The original project source tree is never mutated in place.
- 1.0.0 has no rust-analyzer dependency. Tree-sitter guards are conservative; the compiler classifies semantically invalid mutants.
- The locked 18-family enumeration is authoritative. Subtypes and replacement variants get stable IDs without inflating the family count.
- `compile_error`, `timeout`, and `not_covered` remain visible outcome buckets. MSI is `killed / (killed + survived)`; non-behavioral buckets are excluded from its denominator.
- Fixture-mutating integration tests run serially and verify a green baseline before mutation.
- Research branches are cited from the milestone docs; external benchmark targets remain a separate decision until specified by the map.
- Follow [AGENTS.md](AGENTS.md): atomic commits, branch to PR to merge, and no direct pushes to `main`.
