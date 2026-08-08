# Rust Mutation-Testing Landscape

Research ticket #2 (wayfinder map, rust_mutant). All facts verified 2026-08-08 via `gh api`, crates.io API, and shallow clones of tool sources in `/tmp`. Star counts and dates are live GitHub values at verification time; they drift.

## 1. Per-tool table

| Tool | Repo | Stars | Last push | License | Approach | Status |
|---|---|---|---|---|---|---|
| **cargo-mutants** (incumbent) | sourcefrog/cargo-mutants | 1,246 | 2026-07-06 | MIT | CLI; `syn` parse + **textual patch** of a scratch-tree copy; per-mutant `cargo test --no-run` viability check then `cargo test`; one scratch tree reused across mutants for incremental builds | **Active**; v27.1.0 released 2026-06-02; 94 open issues; 447,287 crates.io downloads; v27.1.0 installed on dev box |
| **mutagen** | llogiq/mutagen | 642 | 2023-05-29 | Apache-2.0 (Cargo.toml: Apache-2.0/MIT) | `#[mutate]` proc-macro bakes all mutations into one compile; runtime activation via `MUTATION_ID` env var; `cargo-mutagen` runner with `--coverage` mode (runs only tests that hit mutated code) | **Stale** (no push since 2023-05); nightly-only; crates.io max 0.1.2 (2018); README self-describes as "architecture-preview" |
| **mutantor** | Mahdi-Movahedian-Atar/mutantor | 0 | 2026-06-17 | none on GitHub; Apache-2.0 in Cargo.toml | Proc-macro **framework** (not a CLI): `#[generate_mutants(AOR, ROR, ...)]` attribute generates mutant functions run by `cargo test`; optional AI-assisted mutation planning (`ai` feature) | New (created 2026-06-05); crates.io 0.2.1, 59 downloads; no issues; effectively unproven |
| **mutest-rs** | zalanlevai/mutest-rs | 26 | 2026-08-08 | Apache-2.0 OR MIT | rustc **driver plugin** (nightly) + runtime-swappable "meta-mutant" compiled once; per-test reachability via call-graph analysis; `cargo mutest run` subcommand; HTML inspector; JSON metadata | **Active**; research tool (PhD; ICST 2023 paper, DOI 10.1109/ICST57152.2023.00014); 11 open issues |
| Geal/mutant | Geal/mutant | 13 | 2016-06-20 | MIT | Early mutation testing for Rust | Dead (2016) |
| heckle.rs | hecklers/heckle.rs | 6 | 2015-04-19 | NOASSERTION | "EXPERIMENTAL, ABANDONED" | Dead (2015) |
| muttest-rs (samuelpilz) | samuelpilz/muttest-rs | 2 | 2023-09-20 | MIT | "Rust Mutation Testing" | Dead (2023) |
| test-mutator | SuperInstance/test-mutator | 0 | 2026-07-12 | MIT | Library: generate mutants, run tests, score | New, unproven |
| morris | marcbrooker/morris | 31 | 2026-03-17 | Apache-2.0 | Experimental AI-powered mutation tester / test critic | Experimental |
| universalmutator | agroce/universalmutator | 161 | 2026-05-20 | NOASSERTION | Regexp-based generic multi-language mutator (not Rust-idiomatic) | Active but not Rust-specific |

Sources: `gh api repos/<owner>/<repo>` for stars/pushed/license; `gh api repos/sourcefrog/cargo-mutants/releases/latest` (v27.1.0, 2026-06-02); crates.io API (`/api/v1/crates/{cargo-mutants,mutants,mutagen,mutantor}`); READMEs of each repo.

## 2. cargo-mutants full mutation-genre inventory

Authoritative source: `src/mutant.rs` `enum Genre` + `src/visit.rs` (v27.1.0). Six genres:

### 2.1 FnValue — replace function body with a type-appropriate value
`src/fnvalue.rs` `return_type_replacements()`. Whole function body replaced (span excludes braces). Value table by return type:

| Return type | Replacement values |
|---|---|
| `()` (no return type) | `()` |
| `bool` | `true`, `false` |
| `String` | `String::new()`, `"xyzzy".into()` |
| `str` | `""`, `"xyzzy"` |
| unsigned ints | `0`, `1` |
| signed ints | `0`, `1`, `-1` |
| `NonZero<T>` | `1.try_into().unwrap()` (+ `(-1).try_into().unwrap()` if T signed) |
| floats | `0.0`, `1.0`, `-1.0` |
| `Result<T, E>` | `Ok(<T-reps>)`, plus `Err(<error_exprs>)` from `--error-values`; `fmt::Result` → `Ok(Default::default())` |
| `HttpResponse` | `HttpResponse::Ok().finish()` |
| `Option<T>` | `None`, `Some(<T-reps>)` |
| `Vec<T>` | `vec![]`, `vec![<T-rep>]` |
| `Cow<T>` | `Cow::Borrowed(<rep>)`, `Cow::Owned(<rep>.to_owned())` |
| containers `Box/Cell/RefCell/Arc/Rc/Mutex<T>` | `<T>::new(<rep>)` |
| collections `BinaryHeap/BTreeSet/HashSet/LinkedList/VecDeque<T>` | `<T>::new()`, `<T>::from_iter([<rep>])` |
| maps `BTreeMap/HashMap<K,V>` | `<T>::new()`, `<T>::from_iter([(k,v)])` (cartesian) |
| `[T; N]` | `[<rep>; N]` |
| slices / `&[T]` / `&mut [T]` | `Vec::leak(Vec::new())`, `Vec::leak(vec![<rep>])` |
| `&str` | `""`, `"xyzzy"` |
| `&T` / `&mut T` (other) | `Box::leak(Box::new(<rep>))` |
| tuples | cartesian product of per-element reps |
| `impl Iterator<Item=T>` | `::std::iter::empty()`, `::std::iter::once(<rep>)` |
| `!` | none |
| anything else | `Default::default()` |

### 2.2 BinaryOperator — exact replacement table
`src/visit.rs` `visit_expr_binary` (v27.1.0). One mutant per replacement; `==` deliberately never becomes `<=` ("can too easily go wrong with unsigned types compared to 0").

| Operator | Replacements |
|---|---|
| `==` | `!=` |
| `!=` | `==` |
| `&&` | `\|\|` |
| `\|\|` | `&&` |
| `<` | `==`, `>`, `<=` |
| `>` | `==`, `<`, `>=` |
| `<=` | `>` |
| `>=` | `<` |
| `+` | `-`, `*` |
| `+=` | `-=`, `*=` |
| `-` | `+`, `/` |
| `-=` | `+=`, `/=` |
| `*` | `+`, `/` |
| `*=` | `+=`, `/=` |
| `/` | `%`, `*` |
| `/=` | `%=`, `*=` |
| `%` | `/`, `+` |
| `%=` | `/=`, `+=` |
| `<<` | `>>` |
| `<<=` | `>>=` |
| `>>` | `<<` |
| `>>=` | `<<=` |
| `&` | `\|`, `^` |
| `&=` | `\|=` |
| `\|` | `&`, `^` |
| `\|=` | `&=` |
| `^` | `\|`, `&` |
| `^=` | `\|=`, `&=` |

### 2.3 UnaryOperator — deletion
`src/visit.rs` `visit_expr_unary`: `!` and `-` are deleted (replaced with empty text). No other unary ops.

### 2.4 MatchArm — arm deletion
`src/visit.rs` `visit_expr_match`: only when the match has a `_` wildcard arm; each non-wildcard, non-guarded arm is deleted (falls through to catch-all). Wildcard arm and guarded arms are never deleted.

### 2.5 MatchArmGuard — guard replacement
Each arm guard expression is replaced with `true` and with `false` (two mutants).

### 2.6 StructField — struct-literal field deletion
`src/visit.rs` `visit_expr_struct`: only for struct literals with a base expression (`..Default::default()` etc.); each named field (with trailing comma) is deleted. Emits structured `MutationTarget::StructLiteralField { field_name, struct_name }` in JSON.

### 2.7 Skip / exclude attributes
- `#[mutants::skip]` — suppress all mutants in scope; valid on functions, `impl` blocks, `trait` blocks, `mod` blocks, `const`/`static` items, file-level `#![mutants::skip]`, and attribute-carrying expressions; honored inside `#[cfg_attr(test, mutants::skip)]` (condition not evaluated). Requires the `mutants` proc-macro crate (0.0.3+; 0.0.5+ for `exclude_re`; 2,911,309 downloads).
- `#[mutants::exclude_re("regex")]` — exclude mutants whose generated name matches the regex; same placement scopes; multiple patterns; also inside `cfg_attr`.
- `--skip-calls` / `skip_calls` config — don't mutate arguments of calls to named functions/methods; default `with_capacity` (disable with `--skip-calls-defaults=false`).

## 3. cargo-mutants feature list (v27.1.0)

- **JSON output**: `--json` (with `--list`); `mutants.out/` contains `mutants.json` (all mutants, with diffs since 26.2.0), `outcomes.json` (per-mutant results + summary counts), `diff/` (one diff per mutant), `logs/` (per-mutant cargo logs), `caught.txt` / `missed.txt` / `timeout.txt` / `unviable.txt`, `previously_caught.txt`, `lock.json` (fs2 file lock). Files incrementally updated during the run. `--emit-schema` for JSON schema.
- **`--in-diff DIFF_FILE`**: test only mutants overlapping regions changed in a git-format diff (`b/` prefix or none); composable with other filters.
- **Sharding**: `--shard N/K` with `--sharding slice` (default) or `round-robin`.
- **nextest**: `--test-tool=nextest` (or `test_tool` config); per-test-process execution stops earlier on failure.
- **`--check`**: build each mutant (`cargo test --no-run`) but don't run tests.
- **Cache / warm-rerun**: **no content-addressed cache.** Speed comes from (a) reusing one scratch tree across all mutants so cargo's own incremental compilation carries over, and (b) `--iterate`, which skips mutants previously caught/unviable by matching name (file, line, col, description) against `previously_caught.txt` — a name-based warm-rerun, not content-addressed. No incremental mode beyond that.
- **Exit codes / thresholds**: `0` success, `1` usage, `2` survived mutants found, `3` timeout, `4` baseline failed, `5` filter-diff mismatch, `6` filter-diff invalid, `70` internal error. **No MSI threshold flag** (no fail-at-score gate).
- **Timeouts**: `--timeout`, `--timeout-multiplier`, `--build-timeout`, `--build-timeout-multiplier`, `--minimum-test-timeout`; adaptive (mutant timeout = baseline duration × multiplier).
- **Baseline**: `--baseline run|skip` (default run).
- **Parallelism**: `--jobs`, GNU jobserver (`--jobserver`, `--jobserver-tasks`).
- **Filters**: `--file`, `--exclude`, `--examine-re`, `--exclude-re`, `--regex`, `--package`, `--test-package`, `--test-workspace`, `--skip-calls`.
- **Other**: `--list`, `--list-files`, `--Zmutate-file` (mutate a single file without a package), `--shuffle`/`--no-shuffle`, `--error-values` (Err values for FnValue), `--annotations` (GitHub/GitLab review annotations), `--in-place`, `--leak-dirs`, `--profile`, `--features`/`--all-features`/`--no-default-features`, `--output`/`CARGO_MUTANTS_OUTPUT`, config file `.cargo/mutants.toml`.
- **Not present**: per-test coverage routing, TCE/equivalent detection, HTML report, Stryker JSON, content-addressed cache, MSI threshold.

Sources: `src/main.rs` (clap args), `src/options.rs`, `src/exit_code.rs`, `book/src/{mutants-out,iterate,performance,shards,nextest,in-diff,exit-codes,skip_calls,attrs}.md`, `NEWS.md`.

## 4. Claimed-vs-unclaimed matrix: 8 candidate Rust-idiomatic operators

| Operator | cargo-mutants | mutagen | mutantor | mutest-rs | Claimed anywhere? |
|---|---|---|---|---|---|
| `?` (try) removal | — | — | — | — | **Unclaimed** |
| `unwrap`/`expect` removal | — | — | — | — | **Unclaimed** |
| `await` removal | — | — | — | — | **Unclaimed** |
| `move` closure removal | — | — | — | — | **Unclaimed** |
| `&mut` → `&` | — | — | — | — | **Unclaimed** |
| `clone` removal | — | — | — | partial: `call_delete` deletes any call/method call (incl. `.clone()`) → `Default::default()`; `call_value_default_shadow` shadows return | **No dedicated operator**; adjacent generic call-deletion in mutest-rs; mutagen `stmt_call` deletes whole statements containing calls |
| `Arc`/`Rc`/`Box` swap | — (FnValue *constructs* `Arc::new(rep)` etc. for return types; never swaps types in place) | — | — | — | **Unclaimed** |
| iterator-chain mutation | — (FnValue handles `impl Iterator` return types only) | — | — | — | **Unclaimed** |

Verification: cargo-mutants `src/visit.rs` implements exactly 17 `visit_*` hooks (call, method_call, file, item_fn, impl_item_fn, trait_item_fn, const/static items, impl, trait, mod, binary, unary, match, struct) — **no** `visit_expr_try`, `visit_expr_await`, `visit_expr_closure`, `visit_expr_reference`, `visit_expr_macro`; method calls are only visited to apply `skip_calls` filtering, never mutated. mutagen's 10 mutators (`mutagen-core/src/mutator/`): binop bit/bool/cmp/eq/num, lit bool/int/str, stmt_call, unop_not — no semantic operators. mutantor's 15 operators (readme.md): AOD/AOI/AOR, COD/COI/COR, LOD/LOI/LOR, ROR, SOR, SDL, IPVR, IPEX, IMCD — all generic. mutest-rs's 18 operators (README + `mutest-operators/src/`): arg_default_shadow, bit_op_or_and_swap, bit_op_or_xor_swap, bit_op_shift_dir_swap, bit_op_xor_and_swap, bool_expr_negate, call_delete, call_value_default_shadow, continue_break_swap, eq_op_invert, logical_op_and_or_swap, math_op_add_mul_swap, math_op_add_sub_swap, math_op_div_rem_swap, math_op_mul_div_swap, range_limit_swap, relational_op_eq_swap, relational_op_invert — no semantic operators.

## 5. Gap statement

The incumbent cargo-mutants (1,246 stars, active, MIT) is a function-level + generic-operator tool: six genres (FnValue value replacement, binary/unary operator replacement, match-arm deletion/guard replacement, struct-field deletion) applied via syn parse + textual patch, with no per-test coverage routing, no TCE/equivalent detection, no content-addressed cache, no HTML/Stryker reports, and no MSI threshold. mutagen is stale (2023) and nightly-only; mutest-rs is an active research tool with the most sophisticated engine (rustc driver, per-test reachability, HTML inspector) but its 18 operators are all generic expression/statement swaps; mutantor is a 0-star proc-macro framework with generic operators and no CLI. **None of the eight Rust-idiomatic semantic operators — `?` removal, unwrap/expect removal, await removal, `move`-closure removal, `&mut`→`&`, clone removal, Arc/Rc/Box swap, iterator-chain mutation — is claimed by any tool** (clone removal has only adjacent generic call-deletion in mutest-rs). The rust_mutant build-for-gap bet therefore holds on both axes: the operator set is unclaimed, and the dart_mutant feature bar (per-test coverage routing, TCE, adaptive timeouts, content-addressed cache, Stryker JSON + JUnit + HTML, agent-friendly JSON) is unclaimed by cargo-mutants, which lacks every one of those features. The open research questions this map must still resolve (per-test coverage attribution mechanism for Rust, TCE mechanism via MIR/LLVM-IR/assembly, schemata vs per-mutant compile) are orthogonal to the gap: no incumbent exists to copy them from.
