# AGENTS.md

# rust_mutant — Mutation Testing for Rust

AST-based mutation testing for Rust — the third sibling of dart_mutant and gopher_mutant. Same Rust + tree-sitter architecture, same feature bar (per-test coverage routing, TCE equivalent detection, adaptive timeouts, content-addressed cache, Stryker JSON + JUnit + HTML reports, agent-friendly JSON), differentiated by Rust-idiomatic mutation operators.

**The goal is in [GOAL.md](GOAL.md).** Read it before touching anything. Every acceptance criterion is measurable.

## Status

- **Planning** — wayfinder map charted 2026-08-08 (this repo's issues). Research tickets in flight. No code yet.

## Agent skills

### Issue tracker

Work is tracked as GitHub issues. Skills that read/write the tracker use the `gh` CLI conventions. See `docs/agents/issue-tracker.md`.

### Domain docs

`docs/agents/domain.md` — glossary, verified landscape facts, privacy line.

## Why this exists

Rust has cargo-mutants (the incumbent, active, ~1.2k stars) but it is function-level + generic-operator level. Nobody does Rust-idiomatic semantic mutations (`?` removal, unwrap/expect, await removal, `move` closures, `&mut`→`&`, clone removal, Arc/Rc/Box swaps, iterator chains) with the full dart_mutant feature bar (per-test coverage routing, TCE, Stryker/HTML reports). Same build-for-gap bet as gopher_mutant.

## Dogfood corpus

sambungapi (MetatechID/sambungapi, private) — a Rust wire-compatible Composio impostor for Bella. Its testing-tier map is blocked by this map's completion; the mutation gate on sambungapi's core logic runs rust_mutant.

## License

MIT (sibling parity).
