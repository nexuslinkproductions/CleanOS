# CleanOS Concept Architecture

## Decision: CLI-first

MVP is a local CLI. Reasons:

- Matches the SPEC (`cleanos collect`, `cleanos report`).
- Keeps mutation gated by explicit terminal approval.
- Makes dry-run and plan JSON portable for agent harnesses.
- Ships faster than a GUI while the safety contract stabilizes.

A guided TUI tour and optional dashboard can wrap the same modules later. The CLI remains the source of truth for scan, plan, execute, and verify.

## Module map

| Module | Responsibility |
|--------|----------------|
| Collector | Read-only probes: CPU, memory, swap, thermal pressure, display topology, process identity, launch items |
| Classifier | Maps findings into taxonomy leaves (`docs/features/category-taxonomy.md`) |
| Ranker | Scores candidates by measured evidence, expected payoff, and risk |
| Tour | Interactive selection UI (TUI) over ranked candidates |
| Planner | Emits deterministic JSON + markdown plans with rollback and verify steps |
| Approver | Explicit user gate before any mutation |
| Remediator | Bounded reversible actions with identity checks |
| Verifier | Re-runs matched probes and records before/after deltas |
| Reporter | Local reports with FACT vs INFERENCE labels |
| Evidence store | Local JSON/MD under `~/Library/Application Support/CleanOS/` |

## Data flow

```
Collector (read-only)
    -> Evidence store
    -> Classifier (taxonomy)
    -> Ranker
    -> Tour (user selection)
    -> Planner (dry-run plan + undo sheet)
    -> Approver (explicit confirm)
    -> Remediator (bounded mutations)
    -> Verifier (matched probes)
    -> Reporter (local artifacts)
```

Mutation never starts before Approver returns yes.

## Safety layers

1. Read-only default for collect and report.
2. Dry-run plan generation before execute.
3. Identity validation (PID + PPID + command path) before process actions.
4. Protected surfaces: credentials, SIP, Apple system agents, unrelated trees.
5. Kill-switch: abort current plan and leave undo sheet intact.

## Rollback model

Every planned step includes:

- exact undo command or restore path
- evidence that would prove success
- evidence that would prove failure

The undo sheet is written before the first mutation. Prefer reversible moves (plist relocate, SIGTERM with identity) over irreversible deletes.

## Local data layout

- `~/Library/Application Support/CleanOS/reports/`
- `~/Library/Application Support/CleanOS/runs/`
- `~/Library/Application Support/CleanOS/experiments/`

Public repo holds schemas and docs only. Runtime evidence stays on-device.

## Interfaces

- Human: CLI + guided TUI tour.
- Agent: portable plan JSON with safety contract fields.
- Future GUI: thin client over the same modules.

## Non-architecture

Cloud collectors, silent upload, identity-blind killers, and destructive cleans stay outside the product shape.
