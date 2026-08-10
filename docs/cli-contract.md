# rust-mutant CLI contract

> **Status:** frozen for rust-mutant 1.0.0 by the GOAL-4 lock. Breaking changes require explicit human renegotiation.
> **Canonical executable:** `rust-mutant`

This document is the agent-facing CLI contract. The published executable uses the hyphenated name even though the repository is named `rust_mutant`.

## Invocation

The canonical invocation is:

```text
rust-mutant --path .
```

There is no required `run` subcommand in 1.0.0. Future subcommands may be added without changing this root invocation.

## Project and configuration

| Option | Meaning | Default |
|---|---|---|
| `--path <DIR>` | Project directory to mutate | `.` |
| `--manifest-path <FILE>` | Cargo manifest override | derived from `--path` |
| `--config <FILE>` | Explicit TOML configuration file | none |
| `--no-config` | Disable automatic configuration loading | false |

The tool automatically loads `.rust-mutant.toml` from the project root. Precedence is:

```text
CLI flags > explicit --config > project .rust-mutant.toml > built-in defaults
```

TOML is the only canonical configuration format. `.cargo/mutants.toml` is not implicitly read because that path belongs to cargo-mutants. Unknown configuration keys and operator names fail closed.

## Output

```text
--format console
--format json
--format stryker-json
--format junit
--format html
--output <DIR>
--json
--quiet
```

- `console` is the default format.
- `--json` is an alias for `--format json`.
- `--format json --quiet` emits exactly one agent JSON document on stdout.
- Progress and diagnostics use stderr, never stdout.
- File formats use `--output <DIR>` when a custom destination is needed; otherwise they use `<project>/mutation-reports`.
- `--format json` means the rust-mutant agent envelope.
- Stryker mutation-testing-elements output is explicit with `--format stryker-json`.
- `--format stryker-json` writes `mutation-report.json`, `--format junit` writes `mutation-results.xml`, and `--format html` writes `mutation-report.html` under `--output` or `<project>/mutation-reports`.
- Stryker JSON uses mutation-testing-elements schema version `"2"`, with object-valued `files` entries containing `language`, `source`, and `mutants`.

### Agent JSON envelope

The root object contains at least:

```json
{
  "schemaVersion": 1,
  "tool": {},
  "project": {},
  "summary": {},
  "mutants": [],
  "timing": {},
  "resources": {}
}
```

`schemaVersion` is an integer. Breaking changes require a major version bump. The summary contains total counts, MSI, threshold, threshold result, and excluded buckets. Each mutant record contains stable id, file, line, column, family, subtype, original/replacement text, status, tests run, duration, cache state, and failure/compile details where applicable. Timing records routing, execution, cache, TCE, and total wall time. Resource records requested/effective workers, global CPU budget, active sessions, memory ceiling, peak RSS, and wait/throttle intervals.

## Mutation and execution controls

```text
--dry-run
--list-operators
--operators <CSV>
--mutant <ID>
--mutants-file <FILE>
--parallel <N>
--timeout <DURATION>
--incremental
--base-ref <REF>
--no-tce
--threshold <PERCENT>
--no-routing
--no-cache
--max-memory <MiB>
```

Defaults:

- project path `.`
- console output
- threshold `80`
- deterministic mutant ordering
- adaptive timeout when available
- routing and cache enabled once M3 exists
- TCE enabled automatically after survivors

`--mutant` accepts one stable mutant ID or one-based discovery-index alias. `--mutants-file` accepts one stable ID or one-based alias per non-empty, non-comment line and runs the selected mutants in discovery order. The two selectors are mutually exclusive; missing IDs fail closed. Accepted numeric aliases include `1`, `0001`, and `m0001`. `--incremental` requires `--base-ref`. `--dry-run` performs discovery only and never runs the baseline or mutants. Post-survival LLVM-IR equivalence analysis is enabled by default in M4; `--no-tce` disables it for the run.

## Global resource governor

Per-session CPU calculation is not sufficient. Concurrent rust-mutant sessions share one host-wide pool.

```text
effective_cpu_budget = process-affinity/cgroup CPU capacity
global_mutant_budget = max(1, floor(0.75 * effective_cpu_budget))
session_workers = min(--parallel, currently_available_global_slots)
```

Rules:

- All active sessions claim slots from one crash-released semaphore.
- On a two-effective-CPU host, the global pool contains one mutant worker.
- A second session waits for a slot rather than independently starting another worker.
- `--parallel N` is a per-session upper bound, never permission to exceed the global pool.
- CPU capacity comes from process affinity or cgroup quota, not only physical CPU count.
- Nested Cargo builds use one build job per outer worker or a shared jobserver budget. Outer and inner parallelism must not multiply.
- Slots are released when a session exits, including abnormal process termination.

RAM is guarded globally as well:

- Detect the effective limit from cgroups when available, otherwise `/proc/meminfo`.
- Reserve at least 25% for the rest of the system.
- Register active sessions and child PIDs with the coordinator.
- Check aggregate registered RSS before launching another mutant.
- Under memory pressure, pause new workers and reduce concurrency toward one worker.
- Report resource throttling on stderr and in JSON; never convert it into a mutant status.
- `--max-memory <MiB>` can lower the safety budget but cannot raise it.
- Unsafe overcommit is not part of 1.0.0.

## Exit codes

| Code | Meaning |
|---:|---|
| `0` | Valid run at or above threshold; successful dry-run/list |
| `1` | Valid mutation run below threshold |
| `2` | Invalid arguments/config, missing project/toolchain, baseline failure, or internal tool error |
| `3` | No mutants found after filters |

`compile_error`, `timeout`, `not_covered`, and `equivalent` are valid mutation outcomes, not tool errors. MSI remains:

```text
killed / (killed + survived)
```

A valid JSON run emits its report even when exiting with code `1` or `3`. Resource throttling does not alter MSI or outcome classification. A zero-mutant report is emitted with exit code `3`.
