# CleanOS Harness-Fleet Probe SPEC v1

**Status:** Implementation contract for build wave 4 (after bench).
**Scope decision (2026-08-11):** detect and characterize AI-harness process
fleets on the local machine with identity-grade evidence. Detection only:
reaping stays in the parked plan/execute layer. This is the harnessreap
protocol integration point: findings carry everything the future reaper
needs, computed but never acted on.

## 1. Goal

`cleanos collect` and `cleanos report` gain a harness-fleet classification
lane: stale or detached AI-harness processes (MCP servers, LSP instances,
agent daemons, dev servers) are identified and reported with the identity
evidence block required by docs/SAFETY.md (PID, PPID, executable, command,
duration, CPU, launchd-managed state). The canonical pattern is the
2026-08-10 GitNexus incident: wrapper-spawned node MCP servers reparented to
launchd (PPID=1) with no launchd job behind them.

## 2. New finding classes (taxonomy category `harness-fleet`)

| Class | Marker rules (command/args contains) | Label |
|---|---|---|
| `harness_mcp_server` | `mcp` (token), `--mcp`, `mcp.json`, `stdio` + `mcp`, socket path under `/tmp/` or `~/Library/.../mcp` | INFERENCE |
| `harness_lsp` | `--stdio` plus one of: `typescript-language-server`, `pyright-langserver`, `rust-analyzer`, `vscode-langservers`, `lsp` | INFERENCE |
| `harness_agent_daemon` | known agent binaries or their daemon args: `codex`, `opencode`, `cursor-agent`, `claude`, `hermes` (except the cleanos process itself) | INFERENCE |
| `stale_dev_server` | node/python/deno process with a LISTEN socket on 127.0.0.1 (or any localhost) and elapsed >= 6 hours | INFERENCE |

Rules:
- Reuse the scan-core orphan foundation: PPID=1 + user-land + not
  launchd-managed is the strongest signal; attached-but-marker processes are
  still REPORTED (state field distinguishes `orphaned` vs `attached`).
- Exclusions (never flagged): the cleanos process itself, launchd-managed
  processes, system-path processes (existing exclusion), kernel processes.
- `reap_safe: bool` is computed for every harness finding: true only when
  user-land AND not launchd-managed AND not self AND not system-path AND
  state == orphaned. Computed for the future reaper, never acted on.

## 3. New probe: sockets

`lsof -nP -iTCP -sTCP:LISTEN` (present on macOS, no sudo). Parsed into a
pid -> [port, host] map in the run JSON (`sockets` field). Used by the
`stale_dev_server` classifier. When lsof is unavailable, the socket probe is
skipped with a note and `stale_dev_server` findings degrade to elapsed-only
(no LISTEN evidence, label INFERENCE with lower confidence).

## 4. Schema changes

- `schemas/run.schema.json`: add `sockets` (map pid -> [{port, host}]) and
  extend process entries with `harness_markers: [string]` when matched.
- `schemas/report.schema.json`: add the four finding classes with the full
  identity evidence block: pid, ppid, executable, command, cpu_pct,
  rss_bytes, elapsed_secs, launchd_managed, state (orphaned|attached),
  reap_safe, harnessreap_compatible: true (the block matches the harnessreap
  protocol field set; exact mapping documented in the plan layer later).

## 5. Tests (cargo test, all green required)

- Marker fixtures: each class with positive and negative cases (an MCP
  server line, a pyright --stdio line, a codex daemon line, a node
  http.server line with an lsof LISTEN entry, a gitnexus-style
  wrapper-spawned node line).
- lsof parser fixtures (LISTEN entries with pid, port, host; IPv4 + IPv6;
  missing columns).
- reap_safe computation: orphaned+user-land+unmanaged = true; attached =
  false; launchd-managed = false; system-path = false; self = false.
- Exclusion: cleanos itself never flagged; launchd-managed never flagged.
- Schema validation for both extended schemas.

## 6. Constraints

- Detection only. No kill, no bootout, no socket closing.
- No em-dashes, affirmative voice, no SSSF, no mutation, no telemetry, no
  network, no sudo.
- Reuse scan-core modules (parsers, classifier, redaction, model).
- Commit nothing: write files only.

## 7. Acceptance (verified by the orchestrator)

1. `cargo build` and `cargo test` green.
2. Fresh collect + report on this machine: the report lists harness findings
   with the full identity block; any running gitnexus MCP wrapper pair
   appears as `harness_mcp_server` with state attached (they are not
   orphans), and no system daemon appears.
3. `reap_safe` is false for every launchd-managed or attached finding on
   this machine.
4. The sockets field is populated and schema-valid.
