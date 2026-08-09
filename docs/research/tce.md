# TCE (trivial compiler equivalence) feasibility for Rust

Research ticket #4 — rust_mutant. Date: 2026-08-08. Branch: `research/tce`.

**Question:** can rust_mutant detect *trivially equivalent* mutants (mutants that
provably change nothing) by compiling the mutant and comparing normalized IR against
the original, the way dart_mutant compares Dart kernel bytecode and gopher_mutant
compares normalized Go assembly?

**Answer in one line:** yes — LLVM IR (`--emit=llvm-ir`) or assembly (`--emit=asm`)
at `-C opt-level=2 -C debuginfo=0` with `--remap-path-prefix` normalizes to
byte-identical output across scratch-tree copies on rustc 1.97.1 stable, and passes
all 12 soundness cases (6 equivalent, 6 must-differ). MIR (`-Zdump-mir`) also works
but requires the `RUSTC_BOOTSTRAP=1` escape hatch on stable and misses one
commutativity case (safe direction).

---

## 1. IR candidates on rustc 1.97.1 (stable)

| Candidate | Flag | Available on stable 1.97.1? | Output |
|---|---|---|---|
| MIR | `-Zdump-mir=all` | **No** — `error: the option 'Z' is only accepted on the nightly compiler` (measured). Works with `RUSTC_BOOTSTRAP=1` env var (measured). | `mir_dump/` directory, one file per function per pass (110 files for a 1-function crate with `all`; exactly 1 file per function with `-Zdump-mir=runtime-optimized`) |
| LLVM IR | `--emit=llvm-ir` | **Yes** (stable) | Textual `.ll` — no `llvm-dis` needed (toolchain ships no LLVM tools; verified `rustlib/.../bin` contains only `gcc-ld`, `rust-lld`, `rust-objcopy`, `wasm-component-ld`) |
| Assembly | `--emit=asm` | **Yes** (stable) | Textual `.s` (AT&T, Intel-syntax-free) |

Sources: `-Zdump-mir` is an unstable compiler flag documented in the Rust Unstable
Book ("This feature has no tracking issue, and is therefore likely internal to the
compiler") and the rustc-dev-guide MIR debugging chapter; stable rustc rejects all
`-Z` flags by design. `--emit` and `-C` codegen options are stable rustc book
documentation.

### What differs between original and mutant, and the normalization needed

All measurements below: same crate content compiled at two different absolute paths
(`/tmp/tce/PA` vs `/tmp/tce/PB`), one with `a + b`, one with `a + b + 0`, via
`cargo build` with `RUSTFLAGS="--emit=<ir|asm> -C opt-level=2 -C debuginfo=0
--remap-path-prefix=<PA>=/SRC --remap-path-prefix=<PB>=/SRC"`.

**LLVM IR** — with the flags above, the two builds are **byte-identical** for
equivalent mutants (measured: `diff` empty). Without `-C debuginfo=0`, these
debug-info-only differences appear (all measured):

1. `!N = !DIFile(filename: "src/lib.rs", directory: "/SRC", checksumkind: CSK_MD5,
   checksum: "<md5 of file>")` — the checksum differs because the mutant file
   content differs. Fixed by `-C debuginfo=0`.
2. `!N = !DILocation(line: 1, column: 42, ...)` — line/column of the mutated
   expression. Fixed by `-C debuginfo=0`.
3. `directory: "/tmp/tce/PA"` vs `"/tmp/tce/PB"` — the scratch-tree path. Fixed by
   `--remap-path-prefix` (with debuginfo off this field disappears entirely).

With debuginfo off, the remaining normalization is small and mechanical:

- `@alloc_<16-hex>` content-addressed constant names (panic-location strings) —
  normalize to `@alloc_H`. The *values* of panic-location constants embed
  line/col (`c"\0A\00...\01\00\00\00#\00\00\00"` = line 1, col 35) and differ when
  a mutant shifts the column of a panicking operation (measured: `a / b` vs
  `0 + a / b` shifts col 35→39 and changes the constant). Normalize any
  `c"\0A..."` constant to `c"LOC"`.
- `ModuleID = 'lib.<16-hex>-cgu.0'` / `source_filename` — the cgu hash. Normalize
  to `lib.CGU`. (Under cargo the ModuleID is the metadata hash, which is
  path- and content-independent — see §2 — so this is only needed for direct
  `rustc` invocations.)
- `!N = !{i64 <number>}` type-hash metadata nodes — appear only in `cfg(test)`
  builds (measured: absent in plain `cargo build`, present in `cargo test
  --no-run` lib IR) and differ when source differs. Strip lines matching
  `^!\d+ = !\{i64 \d+\}$`.

**Assembly** — with `-C debuginfo=0` + remap, equivalent mutants are byte-identical
(measured). Without debuginfo=0: `.loc 1 <line> <col>` directives differ. Without
remap: `.file 1 "/tmp/tce/PA" "src/lib.rs"` differs. The only normalization needed
is the panic-location `.asciz "\n\000...\001\000\000\000#\000\000"` constant
(line/col, same column-shift case as LLVM IR) — normalize `.asciz` constants
starting with `\n` + NUL padding to `.asciz "LOC"`.

**MIR** — dumps contain **no absolute paths at all** (measured: zero path-like
strings in any dump), are byte-identical across scratch-tree paths, and are
deterministic across rebuilds (measured). No normalization needed. The comparison
point is the single `runtime-optimized` pass file per function
(`-Zdump-mir=runtime-optimized` → exactly 1 file per function, measured).

### The DCE trap (critical)

Direct `rustc --crate-type=lib --emit=llvm-ir -C opt-level=2` on a crate with plain
`pub fn`s emits **zero function definitions** — LLVM internalizes and dead-code
eliminates unused pub functions at O2 (measured: 0 defs). Cargo debug builds keep
them (measured: 1 def) because cargo passes `-C metadata` and enables incremental
compilation; `CARGO_INCREMENTAL=0` reproduces the DCE (measured: 0 defs), and
`cargo build --release` also DCEs them (measured: 0 defs). **Consequence: TCE must
run through cargo in debug mode (which is also what `cargo test` uses), never
direct rustc at O2, never release mode.** `#[no_mangle]` also keeps functions alive
(measured) but is not applicable to real code.

---

## 2. The mutant-path problem (Rust has no `-overlay`) and its fix

Go's `go test -overlay` lets gopher_mutant patch files in a scratch tree while
keeping the original paths. Rust has no overlay mechanism; cargo-mutants copies the
whole tree and patches textually, which would make every path in the IR differ.

**Measured facts on 1.97.1:**

1. **Direct `rustc` is path-independent already.** Compiling the same source from
   `/tmp/tce/variant` and `/tmp/tce/other` produces byte-identical LLVM IR and asm
   (measured) — paths in IR are relative (`src/lib.rs`), and the cgu hash in
   `ModuleID` is content-derived, not path-derived.
2. **Cargo's metadata hash is path- and content-independent.** The same crate at
   `/tmp/tce/cargo_a` and `/tmp/tce/cargo_b` produced the identical rlib/IR
   filename hash `a30d9f34306e901c`, and a crate with `a + b` vs `a - b` produced
   the identical symbol name `_RNvCsecxy1v86nAU_8tceprobe1f` (measured). So
   scratch-tree copies do **not** perturb symbol names or file names.
3. **The only path leak is debug info.** Cargo builds at two paths differed only in
   `!DIFile(directory: "/tmp/tce/cargo_a")` vs `".../cargo_b"` (LLVM IR) and
   `.file 1 "/tmp/tce/cargo_a" "src/lib.rs"` (asm). `--remap-path-prefix` (stable,
   rustc book "Remap source paths") fixes both: with
   `--remap-path-prefix=/tmp/tce/PA=/SRC --remap-path-prefix=/tmp/tce/PB=/SRC` the
   two builds are **byte-identical** (measured). With `-C debuginfo=0` the field
   disappears entirely.
4. **`-Zremap-crate-prefix` does not exist** on 1.97.1 (measured:
   `error: unknown unstable option: 'remap-crate-prefix'`). Not needed anyway.

**Fix (the `-overlay` equivalent for Rust):** scratch-tree copy + cargo build with
`RUSTFLAGS="--emit=llvm-ir -C opt-level=2 -C debuginfo=0
--remap-path-prefix=<orig>=/SRC --remap-path-prefix=<mut>=/SRC"`. No in-place
patch/restore dance is required — the remap makes the two trees indistinguishable
to the compiler output. Verified end-to-end on the full soundness suite (§4) and
on a multi-file crate with a submodule (`src/foo.rs` mutated, `src/lib.rs`
identical — EQ detected, measured).

---

## 3. Measured compile cost per candidate

**Box:** 2-core x86_64 (`nproc` = 2), 7.8 GiB RAM, Linux 5.15.0-179-generic,
rustc 1.97.1 (8bab26f4f 2026-07-14), cargo 1.97.1, dev profile (unoptimized +
debuginfo), default incremental. **Method:** wall clock via `time.perf_counter`;
cold = median of 2 runs (rm -rf target first), warm = median of 3 no-op rebuilds,
mutant = median of 3 rebuilds after rewriting one line of `src/lib.rs` (10 small
pub fns) and restoring it. All numbers in milliseconds.

| Config | Cold build | Warm rebuild | Mutant touch (1 line changed) |
|---|---|---|---|
| baseline (no RUSTFLAGS) | 650 | 164 | 360 |
| `--emit=llvm-ir -C opt-level=2 -C debuginfo=0` | 453 | 124 | 272 |
| `--emit=asm -C opt-level=2 -C debuginfo=0` | 581 | 111 | 327 |
| `-Zdump-mir=runtime-optimized -C opt-level=2` (RUSTC_BOOTSTRAP=1) | 973 | 159 | 459 |

Notes: the mutant-touch cost is the realistic per-mutant TCE cost (cargo recompiles
the touched crate; the IR file is a byproduct of the same compile, so TCE adds no
separate compile). MIR is the most expensive (extra dump pass) and requires the
bootstrap env var. These are single-crate numbers; real crates scale with crate
size, and the per-mutant cost is bounded by the incremental rebuild of the mutated
crate only.

---

## 4. Soundness test results

**Contract (one-sided):** a killable mutant must NEVER be marked equivalent.
Identical IR ⇒ equivalent (safe to exclude from the MSI denominator). Different IR
⇒ keep as LIVED (under-detection is safe). False *equivalence* is the only
unacceptable outcome.

**Method:** for each case, two scratch crates (original at `/tmp/tce/PA`, mutant
at `/tmp/tce/PB`), `cargo build` with the candidate flags + remap, compare
normalized IR. LLVM/asm normalization per §1; MIR compared on the
`runtime-optimized` pass file. All at `-C opt-level=2` (O0 fails the EQ cases for
all candidates because overflow-check branches are not folded — measured; O1
passes the spot-checked cases — measured).

| Case | Expect | LLVM IR | ASM | MIR |
|---|---|---|---|---|
| `a + b` vs `a + b + 0` | EQ | EQ ✓ | EQ ✓ | EQ ✓ |
| `a * b` vs `b * a` | EQ | EQ ✓ | EQ ✓ | **DIFF ✗** (safe) |
| `1 * a + b` vs `a + b` | EQ | EQ ✓ | EQ ✓ | EQ ✓ |
| `a * b` vs `a * b * 1` | EQ | EQ ✓ | EQ ✓ | EQ ✓ |
| `a / b` vs `a / b / 1` | EQ | EQ ✓ | EQ ✓ | EQ ✓ |
| `a - b` vs `a - b - 0` | EQ | EQ ✓ | EQ ✓ | EQ ✓ |
| `a + b` vs `a - b` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |
| `a + b` vs `a \| b` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |
| `a + b` vs `a + b + 1` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |
| `a / b` vs `a % b` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |
| `a << b` vs `a >> b` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |
| `a & b` vs `a \| b` | DIFF | DIFF ✓ | DIFF ✓ | DIFF ✓ |

**LLVM IR: 12/12. ASM: 12/12. MIR: 11/12** — MIR does not fold `a * b` into
`b * a` (operand order survives in `Mul(copy _1, copy _2)` vs `Mul(copy _2,
copy _1)`), so it reports DIFF for a genuinely equivalent mutant. That is the
*safe* direction (under-detection), but it means MIR-based TCE catches fewer
equivalents.

Also verified: the realistic `cargo test --no-run` flow (lib + test harness, same
test code on both sides) — EQ and DIFF both correct on lib IR (measured); the
`!{i64 ...}` type-hash strip is required there. Multi-file crate with submodule
mutation — EQ correct (measured). Column-shifting mutants (`0 + a / b` vs `a / b`)
— EQ correct after panic-location normalization (measured).

---

## 5. Recommendation

**Use LLVM IR: `cargo build` with `RUSTFLAGS="--emit=llvm-ir -C opt-level=2
-C debuginfo=0 --remap-path-prefix=<orig>=/SRC --remap-path-prefix=<mut>=/SRC"`,
compare normalized IR (strip `@alloc_<hash>` names, `lib.<hash>-cgu.N` in
ModuleID/source_filename, `c"\0A..."` panic-location constants, `!{i64 ...}`
type-hash nodes).**

Rationale:

- **Stable-only** — no `RUSTC_BOOTSTRAP` escape hatch (MIR's requirement is a
  fragility: the env var is a nightly backdoor that could break or be gated, and
  it makes the tool depend on unstable compiler internals).
- **Best soundness** — 12/12 vs MIR's 11/12; the missed MIR case (commutative
  multiply) is exactly the class of "trivial" mutants TCE exists to catch.
- **Cheapest** — 272 ms per mutant touch vs 459 ms for MIR on the probe crate.
- **Textual and diffable** — no LLVM tools needed; normalization is a handful of
  regexes. ASM is a close second (simpler normalization, same 12/12) and is the
  gopher_mutant precedent; LLVM IR is preferred because it is one level closer to
  semantics (register allocation and instruction scheduling noise are already
  gone) and its panic-location constants are easier to strip than asm `.asciz`
  strings.

**Caveats (one-sided soundness):**

1. TCE is a *filter that can only under-approximate equivalence*. Identical IR ⇒
   equivalent (safe to exclude). Different IR ⇒ LIVED, never EQUIVALENT. The
   contract holds: none of the 6 must-differ cases was ever marked EQ on any
   candidate.
2. TCE must run in **debug mode through cargo** (incremental on). Direct rustc at
   O2 and release builds DCE the functions under test and produce empty IR —
   comparing empty IR would mark *everything* equivalent, which is the one
   unsound failure mode. Guard: assert the IR contains the expected function
   definitions before comparing.
3. The comparison is whole-crate IR. A mutant that changes *any* function (or the
   test harness under `cfg(test)`) yields DIFF → LIVED, which is safe but reduces
   TCE's hit rate. Per-function extraction (slice the IR by symbol) is a possible
   refinement.
4. `-C opt-level=2` is required; at O0 the overflow-check lowering makes even
   `a + b` vs `a + b + 0` differ (measured), so TCE would miss those equivalents
   (safe direction, but weaker). O1 passes the spot checks (measured) and may be
   cheaper on larger crates — worth re-validating on the dogfood corpus.
5. MIR remains a viable fallback (path-free, deterministic, no normalization) if
   the bootstrap escape hatch is ever acceptable, at the cost of missing
   commutativity equivalents.

**Next step:** implement as a `tce` module in the runner crate (dart_mutant
playbook: crate split has a TCE component), gated behind the same "surviving
mutant" trigger, with the empty-IR guard and the normalization table above.
