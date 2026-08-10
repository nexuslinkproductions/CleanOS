# CleanOS MVP Scope

## Goal

Ship an evidence-first local pipeline that scans, classifies, ranks, tours, executes approved changes, and verifies results. The first mutation class is identity-validated orphan process remediation, with login-item inventory as the second reportable surface.

## Pipeline

### 1. Scan (read-only)

Collect local evidence: CPU load, top processes with identity fields, memory/swap/compressor pressure, thermal-pressure events when available, display topology, power source, and launch-item inventory.

Maps to:

- `docs/ARCHITECTURE.md` Collector
- `SPEC.md` AC-1
- `docs/features/login-items-optimizer.md` inventory step
- Originating evidence pattern in the 2026-08-10 M2 Pro ledger (orphan MCP cluster, residual WindowServer/storage UI/Electron costs)

### 2. Classify

Place every candidate in exactly one leaf under CLEANUP or OPTIMIZATION.

Maps to:

- `docs/features/category-taxonomy.md`
- Login-item classes (`active`, `idle`, `dead`, `unknown`) in `docs/features/login-items-optimizer.md`
- Process identity rules in `docs/SAFETY.md`

### 3. Rank by evidence

Score candidates by measured payoff, risk, reversibility, and whether user action is required. Keep FACT and INFERENCE labels on every finding. Prefer candidates with hard measured values over folklore claims.

Maps to:

- `docs/ARCHITECTURE.md` Ranker and Evidence model
- Cross-cutting candidate fields in `docs/features/category-taxonomy.md`
- Residual ranking lessons from the M2 Pro ledger (storage UI close-pane as cheap reclaim, display A/B as hypothesis)

### 4. Guided tour selection

Present an interactive checklist grouped by mode, category, and subcategory. Pre-select safe reversible items. Leave data, network, and workflow-touching items unselected until the user opts in. Selection is the config.

Maps to:

- `docs/features/guided-optimization-tour.md` tour and selection steps
- Taxonomy grouping in `docs/features/category-taxonomy.md`

### 5. Plan and execute approved changes

Generate a deterministic plan (JSON + readable markdown) with ordered steps, identity rules, rollback commands, and expected measurements. Apply only after explicit approval. Prefer SIGTERM, escalate to SIGKILL only when identity is revalidated and necessary.

Maps to:

- Guided tour plan generation and delegation steps
- Login-items execute step (move plist to `disabled/` or `launchctl bootout` with recorded undo)
- `SPEC.md` AC-3 reversible experiment runner
- `docs/SAFETY.md` process action gates

### 6. Verify

Re-run the same probe set. Report before/after deltas, what changed, what stayed, and the full undo sheet.

Maps to:

- Guided tour verify/report step
- Login-items verify step
- `SPEC.md` AC-2 report clarity and AC-3 before/after capture

## MVP in / MVP out

In scope:

- Local CLI entrypoints for collect, report, tour, and apply
- Orphan high-CPU process remediation with identity validation
- Launch-item inventory and classification (disable remains opt-in)
- Local evidence store under Application Support
- Dry-run by default

Out of scope for first ship:

- GUI dashboard (tour may be TUI)
- Cache nuking, snapshot deletion, SIP guidance
- Identity-blind kills
- Telemetry
- Broad autopilot across every taxonomy leaf

## Success signal

A user can reproduce a measured before state, approve one reversible experiment, and see a labeled after report with an undo path.