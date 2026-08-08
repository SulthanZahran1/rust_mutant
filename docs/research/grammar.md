# Grammar Research — tree-sitter-rust Coverage for the 8 Candidate Rust-Idiomatic Mutation Operators

**Ticket:** wayfinder research #5 (rust_mutant)
**Date:** 2026-08-08
**Status:** live-verified (every mutation re-parsed with zero ERROR / MISSING nodes)

## 1. tree-sitter-rust version + node count

| Item | Value |
|---|---|
| Grammar repo | [tree-sitter/tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust) |
| Latest crate (crates.io) | **0.24.2** (published 2026-03-27) |
| npm package used for verification | `tree-sitter-rust` **0.24.0** (npm lags crates.io by one patch; grammar identical for our purposes) |
| tree-sitter runtime (npm) | 0.22.4 |
| Node types in `src/node-types.json` | **280 total — 169 named, 111 anonymous** |
| Supertypes | none in this version |

The Rust crate `tree-sitter-rust` (used by rust_mutant's Rust core) and the npm package ship the same generated parser (`src/parser.c` + `src/node-types.json`), so node types verified here apply 1:1 to the Rust-side integration.

## 2. Per-operator node-type mapping table

| # | Operator | Primary node type(s) | Target identification | Keyword or identifier? |
|---|---|---|---|---|
| 1 | `?` removal | `try_expression` (named; children: `_expression` + anonymous `?` token) | node type alone; `?` is the last child (anonymous, text `?`) | `?` is an **anonymous token**, not a keyword |
| 2 | unwrap/expect removal | `call_expression` (fields `function`, `arguments`) whose `function` is a `field_expression` (fields `value`, `field`) with `field` text `unwrap` / `expect` | method name is a `field_identifier` child of `field_expression` | `unwrap` / `expect` are **plain identifiers** (no `method_call_expression` node exists in this grammar) |
| 3 | await removal | `await_expression` (named; children: `_expression`, `.` anon, `await` anon) | node type alone; receiver is `child(0)` | `await` is a **real keyword** (in the reserved-word list, line 241 of grammar.js) but appears as an **anonymous token** inside `await_expression` |
| 4 | move-closure removal | `closure_expression` (fields `body`, `parameters`, `return_type`; optional `static`/`async`/`move` prefix tokens) | scan children for anonymous token with text `move` | `move` is a **contextual keyword** — NOT in the reserved list; it is an anonymous token in `closure_expression`, `async_block`, `gen_block` |
| 5 | `&mut` → `&` | `reference_expression` (field `value`; optional child `mutable_specifier`) | `mutable_specifier` child (named node, text `mut`) | `mut` is a **named node** `mutable_specifier` (grammar rule `mutable_specifier: _ => 'mut'`), not a bare token |
| 6 | clone removal | `call_expression` + `field_expression` (same shape as #2) with `field` text `clone` | method name is a `field_identifier` | `clone` is a **plain identifier** |
| 7 | Arc/Rc/Box swap | `generic_type` (fields `type`, `type_arguments`), `generic_type_with_turbofish` (same fields), `scoped_identifier` (fields `path`, `name`), `scoped_type_identifier` (fields `path`, `name`) | `type` field text `Arc`/`Rc`/`Box` in type position; `path` field text in constructor position (`Arc::new`) | `Arc`/`Rc`/`Box` are **plain identifiers** (std prelude types, not keywords) |
| 8 | iterator-chain mutation | `call_expression` + `field_expression` chains (nested: `v.iter().map(...).filter(...).collect()` is a left-nested `call_expression` tree) | method name is a `field_identifier` (`map`/`filter`/`collect`/…) | method names are **plain identifiers** |

### Structural notes that matter for implementation

- **No `method_call_expression` node exists.** Method calls are `call_expression` with `function` = `field_expression`. Operators 2, 6, 8 share one matcher: `call_expression → function: field_expression → field: field_identifier`.
- **`?` chains nest:** `g()?.foo()?` parses as `try_expression(try_expression(call_expression(...)))` — each `?` is its own `try_expression`; removing one `?` leaves the outer one intact and parse-clean.
- **`await` chains nest the same way:** `fut.await.to_string()` is `call_expression(function: field_expression(value: await_expression, field: to_string))`.
- **`&mut` appears in three distinct node types** — the operator must only touch `reference_expression` (expression position) and must NOT touch `reference_pattern` (`let &mut z = y;`), `ref_pattern` (`let ref mut r = w;`), `mut_pattern` (`let mut w`), `pointer_type` (`*mut T`), `reference_type` (`&mut T` in type position), or `self_parameter` (`&mut self`). All of these are separate named node types.
- **`move` also appears in `async_block` and `gen_block`** (`async move { … }`, `gen move { … }`) — the operator must scope to `closure_expression` only.
- **Turbofish:** `Vec::<i32>::new()` is `generic_type_with_turbofish` (not `generic_type`); `Arc::<T>::new(x)` needs both variants handled for operator 7.

## 3. Parse-cleanliness results (live-verified)

All probes and mutations were executed with the tree-sitter Node bindings (tree-sitter 0.22.4 + tree-sitter-rust 0.24.0), counting `ERROR` and `MISSING` nodes by walking the full tree. **Every mutation below re-parses with zero ERROR and zero MISSING nodes.**

| # | Mutation | Original ERROR/MISSING | Mutated ERROR/MISSING | Verdict |
|---|---|---|---|---|
| 1 | `g()?` → `g()` (incl. `?` as whole statement `g()?;` and `?` chains) | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 2 | `opt.unwrap()` → `opt`, `r.expect("boom")` → `r` | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 3 | `fut.await` → `fut` (incl. inside call chain `fut.await.to_string()`) | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 4 | `move \|x: i32\| x + 1` → `\|x: i32\| x + 1` (incl. `move \|\| { 42 }`) | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 5 | `&mut *x` → `&*x` (expression position only) | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 6 | `s.clone()` → `s` | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 7 | `Arc<T>` → `Rc<T>`, `Arc::new(T)` → `Rc::new(T)`, `std::sync::Arc<T>` → `std::sync::Rc<T>`, turbofish `Arc::<T>::new(x)` | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |
| 8 | `v.iter().map(\|x\| x*2).filter(...).collect()` → drop `.map(...)` segment (receiver substitution) | 0 / 0 | 0 / 0 | **PARSE-CLEAN** |

**Edge probes that also parse clean (0/0):** `if let` (`let_condition`), let-else (`let_declaration` with `alternative` field), let-chains (`let_chain`), `match` with guards (`match_pattern` with `condition` field), labeled `loop`/`while let`/`for`, `unsafe_block`, `as` casts (`type_cast_expression`), `dyn Trait + Send` (`dynamic_type` + `bounded_type`), `impl Trait` (`abstract_type`), `#[derive(...)]` (`attribute_item` → `attribute` → `token_tree`), `&mut`/`ref mut` patterns, `async move` blocks, macro invocations (`macro_invocation` → `token_tree`), `where` clauses (`where_clause` → `where_predicate` → `trait_bounds`).

**Conclusion:** the gopher_mutant bar (zero ERROR/MISSING after mutation) is met by all 8 operators with purely syntactic, tree-guided text edits. No operator requires semantic information to stay parse-clean.

## 4. Type-info needs per operator

| # | Operator | Type info needed for parse-cleanliness? | Type info needed for *correctness* (mutant validity)? | Conservative heuristic + compiler rejection acceptable? |
|---|---|---|---|---|
| 1 | `?` removal | No | No (any `?` removal compiles if the enclosing fn returns `Result`/`Option`; otherwise it's a compile error) | **Yes** — compile_error bucket catches `?` in non-Result fns |
| 2 | unwrap/expect removal | No | No (receiver type is irrelevant to syntax; `opt.unwrap()` → `opt` always parses; may fail borrow/type checks) | **Yes** — compiler rejects type mismatches |
| 3 | await removal | No | No (removing `.await` yields the future itself; type errors surface at compile) | **Yes** |
| 4 | move-closure removal | No | No (dropping `move` changes capture semantics, not syntax) | **Yes** — may produce borrow errors, caught by compiler |
| 5 | `&mut` → `&` | No | **Yes, ideally** — the callee must accept `&T` (or `&mut T` via reborrow rules); without type info many mutants are compile errors | **Yes** — conservative heuristic (mutate all `&mut` in expression position) + compiler rejection is acceptable; a type-aware refinement (only when callee signature takes `&`) is a later enhancement |
| 6 | clone removal | No | **Yes, ideally** — `T: Clone` bound needed for the *original* to compile; the mutant `s.clone()` → `s` compiles iff the surrounding code doesn't require an owned `T` where only `&T` is available. Without type info, most mutants are compile errors (which is fine — they land in the compile_error bucket, but they're low-value) | **Yes** — but note: clone removal is the operator where type info (or at least `T: Clone` presence in the same file) most improves mutant yield. Syntax-only still meets the parse bar |
| 7 | Arc/Rc/Box swap | No | **Yes, ideally** — `Arc<T>` → `Rc<T>` compiles only if `T: Send + Sync` (Arc) vs not (Rc); `Box<T>` → `Rc<T>` changes ownership semantics. Smart-pointer knowledge (which is which, `::new` vs `::clone` vs `::strong_count` APIs) is needed to avoid trivially-broken mutants | **Yes** — conservative swap + compiler rejection acceptable; restrict to `Arc`↔`Rc` (same `::new`/`::clone` API surface) as the safe pair, treat `Box` swaps as a separate, lower-priority variant |
| 8 | iterator-chain mutation | No | No (dropping `.map()`/`.filter()` segments always parses; type errors surface at compile) | **Yes** |

**Summary:** all 8 operators are parse-clean with syntax-only information. Operators 5, 6, 7 are the ones where type info (rust-analyzer or `cargo check` feedback) would improve *mutant quality* (fewer compile_error-bucket mutants), but the gopher_mutant bar — parse cleanliness — does not require it. The compile_error bucket is the sanctioned sink for semantically-invalid mutants, matching the gopher_mutant playbook.

## 5. Builtins vs keywords findings

| Token | Status in tree-sitter-rust | Evidence |
|---|---|---|
| `?` | **Anonymous token** inside `try_expression` | grammar.js: `try_expression: $ => prec(PREC.try, seq($._expression, '?'))` |
| `await` | **Real keyword** (reserved list) but **anonymous token** in `await_expression` | grammar.js line 241 reserved list; `await_expression: seq($._expression, '.', 'await')` |
| `move` | **Contextual keyword** — NOT reserved; anonymous token in `closure_expression`, `async_block`, `gen_block` | grammar.js: `optional('move')` in all three rules; absent from the reserved list |
| `mut` | **Named node** `mutable_specifier` (text `mut`) | grammar.js: `mutable_specifier: _ => 'mut'`; appears in `reference_expression`, `reference_type`, `pointer_type`, `reference_pattern`, `mut_pattern`, `ref_pattern`, `self_parameter`, `let_declaration`, `static_item`, `field_pattern` |
| `unwrap`, `expect`, `clone`, `map`, `filter`, `collect` | **Plain identifiers** (`field_identifier` in method position) | verified by live parse: `field_expression` children `[identifier, .(anon), field_identifier]` |
| `Arc`, `Rc`, `Box` | **Plain identifiers** (`type_identifier` in type position, `identifier` in path position) | verified by live parse: `generic_type` children `[type_identifier, type_arguments]`; `scoped_identifier` path is `identifier` |
| `static`, `async` (closure prefixes) | Real keywords, anonymous tokens | grammar.js: `optional('static')`, `optional('async')` in `closure_expression` |

**Implication for the matcher design:** operators 2/6/8 match on `field_identifier` *text* (identifier equality — cheap, no keyword ambiguity). Operator 4 matches an anonymous `move` token scoped to `closure_expression`. Operator 5 matches the named `mutable_specifier` node scoped to `reference_expression`. Operator 7 matches `type_identifier`/`identifier` text `Arc`/`Rc`/`Box` in `generic_type`/`generic_type_with_turbofish`/`scoped_identifier`/`scoped_type_identifier` positions. None of the targets are reserved words, so identifier-text matching cannot collide with keyword parsing.

## 6. Other node types the operator set should know about

Verified present in this grammar (all parse clean in probes):

- **`let_condition`** (`if let` / `while let` conditions) and **`let_chain`** (let-chains, `if let A = a && let B = b`) — relevant if a future operator mutates `if let` → `if` or strips guards; not needed by the 8.
- **`let_declaration` with `alternative` field** — let-else (`let Some(x) = opt else { … }`); the `else` block is a plain `block` child. A future "let-else removal" operator would target the `alternative` field.
- **`match_expression` / `match_block` / `match_arm` / `match_pattern`** (with `condition` field for guards) — match-arm deletion/guard replacement (cargo-mutants genres) would use these.
- **`loop_expression` / `while_expression` / `for_expression`** (all with optional `label` child) — loop-body mutation targets.
- **`unsafe_block`** — candidate for an "unsafe block removal" operator.
- **`type_cast_expression`** (`x as u64`, fields `value`, `type`) — candidate for cast-type replacement.
- **`dynamic_type`** (`dyn Trait`) and **`abstract_type`** (`impl Trait`) — appear as children of `type_arguments`/`bounded_type`; relevant to operator 7 (`Box<dyn Trait>` swaps) and to any `dyn`/`impl` swap operator.
- **`attribute_item` / `attribute` / `token_tree`** — `#[derive(...)]` is `attribute` with `token_tree` argument; a derive-removal operator would parse the `token_tree` textually (tokens are opaque).
- **`where_clause` / `where_predicate` / `trait_bounds` / `removed_trait_bound`** — trait-bound removal targets.
- **`macro_invocation` / `token_tree`** — macro bodies are opaque token trees; mutations inside `vec![…]` etc. must be handled textually or skipped.
- **`reference_pattern` / `ref_pattern` / `mut_pattern` / `pointer_type` / `reference_type` / `self_parameter`** — the `&mut`-adjacent node types operator 5 must explicitly exclude.
- **`async_block` / `gen_block`** — contain `move` tokens; operator 4 must exclude them.
- **`generic_type_with_turbofish`** — operator 7 must handle alongside `generic_type`.
- **`scoped_type_identifier`** (`std::sync::Arc<T>` type position) — operator 7 must handle alongside `scoped_identifier`.

## 7. Reproduction steps

```bash
# 1. Environment: node >= 18 (v22 used here), npm
mkdir -p /tmp/grammar_research && cd /tmp/grammar_research
npm init -y
npm install tree-sitter tree-sitter-rust
# -> tree-sitter 0.22.4, tree-sitter-rust 0.24.0 (npm); crates.io latest is 0.24.2

# 2. Node-type inventory (280 types: 169 named, 111 anonymous)
node -e "const j=require('tree-sitter-rust/src/node-types.json'); console.log(j.length, j.filter(n=>n.named).length)"

# 3. Live parse + mutation verification
#    Scripts used for this ticket (kept in /tmp/grammar_research):
#      verify.mjs   — structural probes + all 8 mutations, ERROR/MISSING counts
#      edge.mjs     — 18 edge-case syntax probes (if-let, let-else, match, dyn, impl Trait, ...)
#      edge_mut.mjs — 6 edge-case mutations (? as statement, turbofish, &mut patterns, ...)
node verify.mjs      # expect: every mutation PARSE-CLEAN (ERROR=0 MISSING=0)
node edge.mjs        # expect: every probe ERROR=0 MISSING=0
node edge_mut.mjs    # expect: every mutation PARSE-CLEAN

# 4. Grammar source (for token/keyword evidence)
#    node_modules/tree-sitter-rust/grammar.js  (rules: try_expression, await_expression,
#      closure_expression, reference_expression, mutable_specifier, generic_type, ...)
#    node_modules/tree-sitter-rust/src/node-types.json
```

The verification harness pattern (parse → apply tree-guided text edits → re-parse → count ERROR/MISSING by walking the tree) is exactly the loop rust_mutant's mutation engine will run per mutant, so these results transfer directly to the implementation.
