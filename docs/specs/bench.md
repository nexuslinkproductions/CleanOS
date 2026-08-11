# CleanOS Bench SPEC v1

**Status:** Implementation contract for build wave 3 (after scan core).
**Scope decision (2026-08-11):** `cleanos bench` codifies the post-reboot
benchmark suite from docs/benchmarks/baseline-2026-08-11.md as repeatable,
machine-readable probes. Measure only: no mutation, no telemetry, no network.

## 1. Goal

`cleanos bench` runs a bounded benchmark suite on the local machine, stores
the result as JSON, and can compare any two stored results (or a stored
result against the current machine) with deltas. This turns the evidence
floor into a product feature: matched probes, stored baselines, before/after
deltas, reproducible on any machine.

## 2. CLI surface

- `cleanos bench` - run the full suite (bounded, ~3-4 minutes).
- `cleanos bench --quick` - CPU burst 1 run only (~1 minute).
- `cleanos bench compare [REF]` - deltas between two stored results; with no
  REF, compare the two most recent. Table output plus JSON.
- `cleanos bench --power` - include the powermetrics probe (requires root;
  skipped with a note when `sudo -n true` fails).
- Exit codes: 0 success, 1 probe failure that prevents a result, 2 usage error.

No other bench subcommands in v1.

## 3. Probes

Every probe shells out to a macOS tool and parses its output: the raw system
output IS the evidence (same philosophy as the scan core). A probe that
cannot run (missing binary, no root) is recorded as skipped with a reason,
never a run failure.

### cpu_burst

`ffmpeg -y -loglevel error -f lavfi -i testsrc2=size=1920x1080:rate=60:duration=12
-c:v libx264 -preset medium -threads 12 -f null -` wrapped in
`/usr/bin/time -p`, 3 runs by default (`--runs N`), median real/user/sys.

### cpu_sustained

Same command with duration=60, 1 run.

### crypto

OpenSSL 3 (prefer `/opt/homebrew/opt/openssl@3/bin/openssl`):
`speed -evp aes-128-gcm -seconds 5 -multi 12` and
`speed -seconds 5 -multi 12 sha256`. Parse the 4 KB block row and the peak
(8 KB) row. Fallback to `/usr/bin/openssl` without `-multi` when brew
OpenSSL 3 is absent (report the fallback in the result).

### memory

Allocate 4 GB (touched, 1 byte per 16 KB page), capture vm_stat pages free
and `sysctl vm.swapusage` before / during / after, report pages-free delta,
swap delta, compressor delta. Page size read from the vm_stat header.

### power (optional, `--power`, root)

During a 20 s encode, run `powermetrics -n 8 -i 5000 --samplers cpu_power`.
Parse per sample: P0-Cluster and P1-Cluster HW active frequency, max
per-core frequency, E-Cluster HW active frequency, CPU Power / GPU Power /
Combined Power mW. Record the peak-cluster sample and the combined power at
that sample. Example block (format reference, real capture from 2026-08-11):

```
P0-Cluster HW active frequency: 2839 MHz
CPU 4 frequency: 3404 MHz
...
P1-Cluster HW active frequency: 2846 MHz
E-Cluster HW active frequency: 2186 MHz
CPU Power: 11024 mW
GPU Power: 2024 mW
Combined Power (CPU + GPU + ANE): 13047 mW
```

## 4. Storage and schema

- `~/Library/Application Support/CleanOS/benchmarks/YYYYMMDD-HHMMSS.json`
- `schemas/bench.schema.json` (draft-07) checked into the repo; every
  emitted field validated in tests.
- Result shape: collected_at, duration_ms, machine (chip, cpu_count,
  os_version), probes: { cpu_burst: { runs: [{real,user,sys}], median: {...} },
  cpu_sustained: {...}, crypto: { aes_128_gcm_4kb, aes_128_gcm_peak,
  sha256_4kb, sha256_peak, fallback: bool }, memory: { pages_free_delta,
  swap_used_bytes_before/after, compressor_bytes_before/after },
  power: { captured: bool, reason: string|null, peak_p0_mhz,
  peak_p1_mhz, max_core_mhz, e_cluster_mhz, cpu_mw, gpu_mw, combined_mw } },
  skipped: [{ probe, reason }] }.

## 5. Compare

`bench compare` computes per-probe deltas (absolute and percent) between two
results and prints a table: probe, before, after, delta, pct. Ordering is
canonical (cpu_burst median real first). JSON compare output under
`benchmarks/compare-<ref1>-<ref2>.json` when `--json` is passed.

## 6. Redaction

Same redaction module as the scan core applies to all bench output: no raw
home paths, no UUIDs, no tokens.

## 7. Tests (cargo test, all green required)

- Fixture parsers: `/usr/bin/time -p` output, openssl speed rows (multi and
  single formats), powermetrics sample block (use the format above plus a
  second block with different values), vm_stat header with 16384 page size,
  swapusage line.
- Memory probe: allocation lifecycle leaves no swap on a clean machine
  (fixture-driven assertion on the swap delta logic, no live alloc in unit
  tests; the live alloc runs in the smoke).
- Compare: delta math, percent rounding, canonical ordering, missing-probe
  handling (a skipped probe compares as n/a).
- CLI: bench/bench --quick/bench compare surface; unknown bench subcommand
  exits 2; bench help shows no mutation wording.
- Schema: bench JSON validates against schemas/bench.schema.json.

## 8. Constraints

- No new dependencies beyond the scan core set unless strictly required.
- No em-dashes in code, comments, help text, or output.
- Affirmative voice in help text.
- No sudo unless `--power` is passed and root is available.
- No telemetry, no network, no mutation, no writes outside the data root
  and the repo working tree.
- No SSSF or factory branding.
- Commit nothing: write files only.

## 9. Acceptance (verified by the orchestrator on the live machine)

1. `cargo build` and `cargo test` green.
2. `cleanos bench --quick` completes under 90 s and writes a schema-valid
   result JSON.
3. `cleanos bench compare` prints deltas between two stored results.
4. `cleanos bench --power` records the powermetrics envelope when root is
   available and a clear skipped note when it is not.
5. The cpu_burst median from the full run is within 20% of the documented
   2026-08-11 baseline (2.90 s) or the result explains the delta.
