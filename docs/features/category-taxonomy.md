# CleanOS Category Taxonomy

**Status:** Intake / proposed
**Purpose:** Every candidate CleanOS can find belongs to exactly one leaf
category. The taxonomy is the product skeleton: it drives the guided tour
grouping, the plan schema, the dashboard, and the module list.

## Top-level split

### CLEANUP — remove what should not be there

| Category | Subcategories | Examples |
|---|---|---|
| `processes` | orphans (PPID=1) · stale dev servers · busy-loop suspects · duplicate instances · stuck greps/CLIs | whatsapp-mcp fleet, dual astro dev, API_KEY grep zombies |
| `logs` | user logs · system logs · app crash logs · harness logs · rotation config | 457 MB stale Photoshop crash logs, 41 GB boot.logs incident |
| `caches` | app caches · framework caches · package-manager caches · thumbnail caches | brew/pip/npm caches, derived data |
| `temp-state` | /tmp and /private/tmp · stale sockets · lock files · dead-app state dirs | 19h astro server running from /private/tmp |
| `crash-data` | DiagnosticReports · crashpad handlers · minidumps · hang logs | versioned Cursor crashpad handlers |
| `old-artifacts` | stale worktrees · dead-project node_modules · backup files · duplicate downloads | October worktree http.server, `.bak` piles |
| `launch-items` | dead login agents · dead daemons · stale updaters | Adobe agents with 9 months idle (evidence-classified) |
| `registry-state` | stale MCP registrations · harness state files · service leftovers | duplicate gitnexus MCP registrations |

### OPTIMIZATION — make what is there faster and cheaper

| Category | Subcategories | Examples |
|---|---|---|
| `cpu` | wakeup leaders · busy renderers · background agents · QoS/priority | WindowServer 922 wakeups/s, Hermes renderer |
| `memory` | swap/compressor pressure · RSS hogs · Electron fleet · pressure events | 7.5 GB swap after 20 days uptime |
| `startup` | login items · launch chains · preloads · background items | 40+ launch agents inventory |
| `disk-io` | sync agents · indexing (Spotlight) · log write rates · SSD health | Dropbox sync, mds spikes |
| `network` | updaters · sync · background fetch · wake-on-LAN | womp 1 -> 0 (applied), keystone updaters |
| `display-gpu` | refresh rate · display count · compositor load · GPU residency | 240 Hz triple display WindowServer A/B |
| `power-thermal` | power settings · fan policy · LPM · thermal pressure · battery | plimit/thermal instrumentation, pmset tuning |
| `storage` | capacity hygiene · purgeable space · APFS snapshots · big-file find | 415 GB free, log bulk |
| `harness-fleet` | AI-harness stale processes · MCP server sprawl · LSP instances · dev servers | harnessreap protocol integration, 40-process census |

## Cross-cutting fields (every candidate)

`id` · `category` · `subcategory` · `summary` · `evidence` (measured values)
· `expected_gain` · `risk` (low/med/high) · `reversible` (yes/no + how)
· `requires_user_action` · `mode` (cleanup | optimization) · `auto_ok`

## Plan schema

The guided tour emits one plan per mode (or one combined plan). Each step:

```json
{
  "id": "cleanup.logs.adobe-acp-2026-08-10",
  "category": "logs",
  "subcategory": "app-crash-logs",
  "action": "delete_stale",
  "targets": ["~/Library/Logs/CreativeCloud/ACPLocalLogs"],
  "identity_rule": "path must match ^/Users/.../ACPLocalLogs$",
  "rollback": "no-rollback-needed (logs regenerate)",
  "expected_gain": "403 MB",
  "verified_by": "du before/after"
}
```

## Expansion rule

New surface = new subcategory under an existing category, or a new category
only when no existing one fits. Categories map 1:1 to CleanOS probe modules.
