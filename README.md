# rust-mutant

`rust-mutant` is a Rust mutation-testing CLI for finding tests that do not
fail when production code is changed. It uses deterministic source mutants,
bounded execution, incremental caching, LLVM coverage routing, and a guarded
LLVM-IR trivial compiler equivalence pass.

## Install

From crates.io after the 1.0.0 release:

```text
cargo install rust-mutant
```

From a GitHub release on Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/SulthanZahran1/rust_mutant/main/install.sh | sh
```

On Windows, run `install.ps1` from PowerShell. Both installers verify the
release SHA-256 manifest before installing the binary. Homebrew users can
install the checked-in formula after adding this repository as a tap.

## Quick start

```text
rust-mutant --path . --format console
rust-mutant --path . --format json --quiet > mutation-report.json
rust-mutant --path . --format stryker-json --output mutation-reports
rust-mutant --path . --format junit --output mutation-reports
rust-mutant --path . --format html --output mutation-reports
```

The CLI contract is frozen in [docs/cli-contract.md](docs/cli-contract.md).
The agent envelope schema is in
[docs/schema/agent-report.schema.json](docs/schema/agent-report.schema.json).
The agent envelope uses `schemaVersion: 1`; Stryker output uses
mutation-testing-elements `schemaVersion: "2"`. Breaking contract changes
require a major release.

By default, trivial compiler equivalence analysis runs after a mutant
survives. Use `--no-tce` for a fast ordinary mutation campaign. Equivalent
mutants are reported separately and are excluded from MSI. TCE compilation
errors are visible in the mutant record and never become false equivalent
classifications.

## Operator families

The 18 public families are:

- Generic: `AOR`, `AOD`, `AOI`, `ROR`, `LOR`, `LCR`, `COR`, `SDL`, `RVR`,
  `loop-inc-dec`
- Rust idiomatic: `question-mark-removal`, `unwrap-expect-removal`,
  `await-removal`, `move-closure-removal`, `mut-to-shared`, `clone-removal`,
  `arc-rc-swap`, `iterator-chain`

Use `--list-operators --json` to inspect the machine-readable catalogue.

## Configuration

A project may contain `.rust-mutant.toml`. Explicit CLI values take
precedence. `--config <FILE>` selects a different file and `--no-config`
disables automatic loading.

```toml
path = "."
format = "console"
threshold = 80.0
parallel = 4
operators = ["AOR", "ROR", "clone-removal"]
no_tce = false
incremental = true
```

## Result buckets and exit codes

Every mutant has exactly one primary result: `killed`, `survived`,
`not_covered`, `compile_error`, `timeout`, or `equivalent`. Exit codes are:

- `0`: campaign completed and met the threshold
- `1`: campaign completed but failed the threshold
- `2`: invalid arguments, configuration, or project
- `3`: valid project with zero discovered mutants

## Development

The workspace is pinned to Rust 1.97.1. The standard local gate is:

```text
cargo fmt --all -- --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

The M4 soundness fixture is under `tests/fixtures/tce`: six equivalent and
six must-differ AOR cases across two source files. The repository CI runs the
same CLI smoke path on Linux and Windows. Release packaging creates verified
Linux and Windows archives, SHA-256 manifests, crates.io packages, and the
Homebrew formula target.
