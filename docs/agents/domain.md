# Domain — rust_mutant

## Working glossary

- **rust_mutant** — the tool this repo builds: AST-based mutation testing for Rust, third sibling of dart_mutant (Dart) and gopher_mutant (Go). Same Rust + tree-sitter architecture, same feature bar.
- **dart_mutant** — the first sibling (SulthanZahran1/dart-mutant, MIT). Validated playbook: Rust + tree-sitter, crate split (CLI/core/runner/report/TCE), mutant schemata, per-test coverage routing, adaptive timeouts, content-addressed cache, TCE, Stryker JSON + JUnit + HTML, agent-friendly JSON.
- **gopher_mutant** — the second sibling (SulthanZahran1/gopher_mutant, MIT). Go sibling; `go test -overlay` byte-patching; 22 operators (10 generic + 12 Go-idiomatic); M1 locked.
- **cargo-mutants** — the incumbent Rust mutation tool (sourcefrog, MIT, ~1.2k stars, active). Function-level + generic operator mutations via syn parse + textual patch. NOT a vacuum — the gap is Rust-idiomatic semantic operators + the full dart_mutant feature bar.
- **mutagen / mutantor / mutest-rs** — other Rust mutation tools: mutagen (proc-macro, nightly, stale 2023), mutantor (proc-macro framework, generic ops, AI-assisted planning), mutest-rs (attribute-based, small).
- **MSI** — mutation score indicator: killed / (killed + survived), excluding not-covered and equivalent.
- **TCE** — trivial compiler equivalence: compile a surviving mutant and compare normalized IR against the original; identical IR ⇒ provably equivalent, excluded from the MSI denominator.
- **Schemata** — compile once with all mutants injected (Dart playbook) vs per-mutant compile (Go playbook via `-overlay`). Rust's equivalent mechanism is an open research question (cargo-mutants copies the tree and patches textually; per-mutant `cargo test` recompiles).
- **sambungapi** — the dogfood corpus #1: MetatechID/sambungapi, a Rust (axum + rusqlite) wire-compatible Composio impostor for Bella. Its testing-tier map (MetatechID/sambungapi issues) is blocked by this map's completion.

## Known constraints / facts from research

- Landscape verified 2026-08-08 (gh api + web): cargo-mutants 1,246 stars, pushed 2026-07-06, MIT, v27.1.0 installed on the dev box; mutagen 642 stars, last pushed 2023-05-29, Apache-2.0, nightly-only proc-macro; mutantor is a proc-macro framework (not a CLI) with generic ops + SDL + IPVR/IPEX and AI-assisted planning, reports/parallel on its roadmap; mutest-rs is attribute-based.
- cargo-mutants mutation genres (verified from mutants.rs): FnValue (replace function body with type-appropriate value), binary operator replacement (==/!=, &&/||, </>, <=/>=, +/-, */, /%, <<>>, &|^, assignment forms), unary operator deletion (!, -), match arm deletion (only with wildcard arm), match arm guard replacement (true/false), struct literal field deletion (only with base expression), skip attributes (#[mutants::skip], #[mutants::exclude_re]).
- cargo-mutants mechanics: syn parse + textual patch (not token-stream), scratch-tree copy, baseline test, per-mutant `cargo test --no-run` viability check then `cargo test`, JSON output, --in-diff, sharding, nextest support.
- Rust has NO built-in per-test coverage attribution (Go has coverprofile, Dart has --coverage). The per-test routing research must find the mechanism (llvm-cov/grcov/nextest-based).
- TCE for Rust: Dart compares kernel bytecode, Go compares normalized assembly. Rust candidates: MIR (rustc -Zdump-mir), LLVM IR (llvm-cov/llvm-dis), or assembly. Open research question.
- Sibling licenses: dart-mutant MIT, gopher_mutant MIT. rust_mutant: MIT (locked).
- The sambungapi map (MetatechID/sambungapi) has a ticket "mutation gate" blocked by this map's completion (cross-repo body convention).

## Privacy

No private/personal data involved in this effort. sambungapi is a private Metatech repo; its contents stay out of this public repo's artifacts except as benchmark/corpus facts (repo name, LOC, test counts).
