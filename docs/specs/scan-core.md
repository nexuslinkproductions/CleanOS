# CleanOS Scan Core v1 SPEC

**Status:** Implementation contract for build wave 1 (scan + evidence core).
**Scope decision (2026-08-10):** mutation classes and the guided tour are
parked. This wave ships the read-only evidence pipeline only: Collect,
Classify, Rank, Report. No process actions, no service changes, no writes
outside the local data root.

## 1. Goal

Ship a local Rust CLI on macOS (Apple Silicon) that:

1. Collects a read-only evidence snapshot without sudo.
2. Classifies findings into taxonomy leaves with FACT vs INFERENCE labels.
3. Ranks candidates with a deterministic, documented score.
4. Renders a human-readable report and writes JSON, redacted by default.

This implements SPEC.md AC-1 (evidence collection), AC-2 (report clarity),
AC-5 (privacy), and the Collector, Classifier, Ranker, Reporter modules of
docs/concept/architecture.md. AC-3 (experiment runner), AC-4 (process action
gates) are out of scope for this wave; the CLI must expose no mutation
commands so the approval-refusal path has nothing to bypass.

## 2. Language and toolchain

Rust stable (edition 2021 or newer). Dependencies kept minimal: serde,
serde_json, clap (derive), chrono. No nightly features. Process data is
collected by shelling out to macOS system tools and parsing their output:
the raw system output IS the evidence, and fixture-based parser tests keep
the core deterministic. Cargo manifest at repo root (`Cargo.toml`), source
under `src/`, integration tests under `tests/`.

## 3. CLI surface

- `cleanos collect` - run all probes, write the run JSON, print its path.
- `cleanos report [RUN]` - load a run JSON (path or basename from the runs
  dir), classify, rank, print the table, write the report JSON.
- `cleanos --version`.
- Any other subcommand is an error. No mutation commands exist in this wave.

Exit codes: 0 success, 1 probe failure that prevents a run, 2 usage error.

## 4. Probes (AC-1 set)

Every probe: no sudo, read-only, isolated (a failing probe is recorded in
`probe_errors` and never aborts the run).

| Probe | Source | Fields |
|---|---|---|
| system | `sw_vers -productVersion`, `sysctl -n machdep.cpu.brand_string`, `sysctl -n hw.ncpu`, uptime via `sysctl -n kern.boottime` | os_version, chip, cpu_count, boot_time_epoch, loadavg 1/5/15 from `sysctl -n vm.loadavg` |
| memory | `vm_stat` + `sysctl vm.swapusage` + `sysctl -n kern.memorystatus_vm_pressure_level` | total/used/free bytes (page size from `vm_stat` header), swap_used/swap_total bytes, compressor bytes, pressure_level |
| processes | `ps -axo pid=,ppid=,pcpu=,rss=,etime=,comm=,args=` | per process: pid, ppid, cpu_pct (single sample, documented limitation), rss_bytes, elapsed_secs, executable, command |
| launchd | `launchctl list` | managed pid -> label map (used by the classifier) |
| power | `pmset -g batt` | source (AC/battery), percentage |
| thermal | `pmset -g therm` (when available) | thermal_pressure_level |
| display | `system_profiler SPDisplaysDataType` | display count, primary resolution/refresh summary (serials never collected) |

Every numeric field is a raw measured value. The run JSON records
`collected_at` (ISO 8601 local) and `duration_ms`.

## 5. Classification

Maps candidates into taxonomy leaves from
docs/features/category-taxonomy.md. v1 covers the `processes` (cleanup) and
`cpu`/`memory` (optimization) leaves only.

| Finding | Rule | Label |
|---|---|---|
| `orphan_candidate` | ppid == 1 AND pid not in launchd-managed set AND (cpu_pct >= 5.0 OR rss_bytes >= 536870912) | INFERENCE (single sample; sustained claim requires a second sample) |
| `runaway_suspect` | cpu_pct >= 50.0, ppid != 1 | INFERENCE |
| `memory_hog` | rss_bytes >= 1073741824 | FACT (measurement); impact inference stated |
| `duplicate_orphan` | 2+ processes with identical executable+args and ppid == 1, not launchd-managed | INFERENCE |
| `observed` | everything else with a score of 0 | none (listed, no action) |

Exclusions: pid 1 itself, kernel_task (pid 0), the cleanos process itself.
Thresholds are constants in one module with unit tests.

Launch-item inventory (report section only, no classification): counts of
plists under `~/Library/LaunchAgents`, `/Library/LaunchAgents`,
`/Library/LaunchDaemons`, with the top 10 labels by directory.

## 6. Ranking

Deterministic score (documented in code, unit-tested for tie stability):

```
base: orphan_candidate 40, runaway_suspect 25, memory_hog 15, duplicate_orphan 10
+ min(cpu_pct / 5, 20)
+ min(rss_bytes / 1073741824, 10)   (whole GBs, capped)
- 25 if launchd-managed (should not happen for orphan, guards the rule)
```

Ties break by pid ascending. Every ranked candidate carries the taxonomy
cross-cutting fields: id (category.subcategory.pid), category,
subcategory, summary, evidence (the measured values that fired the rule),
expected_gain (low/med/high class only), risk (low/med/high), reversible
(yes + how), requires_user_action, mode (cleanup|optimization), auto_ok
(always false in this wave), fact_or_inference.

## 7. Report and redaction (AC-2, AC-5)

- `cleanos report` renders: summary line (probe count, error count, findings
  by class), a table of ranked candidates (score, pid, cpu, rss, label,
  summary), and the launch-item inventory. If a class has zero findings the
  report says so explicitly.
- Redaction applied to ALL report output and the report JSON by default:
  - `/Users/<username>` -> `/Users/<user>`
  - UUIDs (8-4-4-4-12 hex) -> `<uuid>`
  - key-like tokens (sk-, pk-, AKIA, api_key=..., token=..., Bearer ...) -> `<redacted>`
  - serial-number patterns (C02.../F2... Apple shape) -> `<serial>`
- The run JSON under Application Support stays raw (local-first); the
  report JSON and terminal output are always redacted.

## 8. Data layout

- `~/Library/Application Support/CleanOS/runs/YYYYMMDD-HHMMSS.json`
- `~/Library/Application Support/CleanOS/reports/<run-basename>.report.json`
- `schemas/run.schema.json` and `schemas/report.schema.json` (draft-07)
  checked into the repo; tests assert every emitted field against them.

## 9. Tests (cargo test, all green required)

- Parser fixtures: ps output (incl. args with spaces, missing rss, header
  edge cases), launchctl list, vm_stat page conversion, pmset batt/therm,
  system_profiler display summary.
- Classifier: orphan vs launchd-managed vs normal; thresholds; exclusions
  (pid 1, kernel_task, self).
- Ranker: determinism (same input, same order), tie break by pid.
- Redaction: username, uuid, sk-token, bearer token, apple serial.
- Reporter: zero-finding honesty, output contains no unredacted home path.
- Schema: run JSON and report JSON validate structurally against the
  checked-in schemas.
- CLI: `collect --help` and `report --help` expose only the documented
  surface; an unknown subcommand exits 2.

## 10. Constraints

- No em-dashes anywhere in code, docs, or output.
- Affirmative voice in help text and messages (state what the tool does).
- No telemetry, no network calls, no sudo, no writes outside the data root.
- No SSSF or factory/orchestration branding anywhere in the repo.
- No mutation commands in this wave.
- Commit nothing: write files only. The orchestrator reviews and commits.

## 11. Acceptance (verified by the orchestrator on the live machine)

1. `cargo build` and `cargo test` are green.
2. `cleanos collect` runs without sudo, exits 0, writes a run JSON whose
   fields match the schema.
3. `cleanos report` prints the table and writes a redacted report JSON;
   every `/Users/...` path in the report is redacted.
4. Orphan classification agrees with a manual cross-check: any PPID=1
   process not in `launchctl list` appears as a candidate (or the report
   honestly states zero candidates).
5. `cleanos` with an unknown subcommand exits 2 and prints usage.
