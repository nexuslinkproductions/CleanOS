# Feature: Guided Optimization Tour

**Status:** Intake / proposed
**Product:** CleanOS
**Related:** login-items-optimizer.md

## Problem

CleanOS can find dozens of optimization opportunities. Firing them all at a
user is noise. Users need one interactive pass where they see everything,
understand each option in one line, pick what they want, and hand the
resulting plan to an agent to execute and verify.

## Flow

1. **Scan** (automatic, read-only): collect every candidate from all
   optimizers (login items, stale processes, log growth, wakeups, swap,
   launch agents, display hints). Each candidate carries:
   - id, category, one-line summary, expected gain, risk, reversibility,
     evidence (measured values), requires-user-action flag.
2. **Tour**: one interactive screen. Grouped by category. Each row:
   `[x]  [gain]  [risk]  summary  (evidence)`.
   Defaults: safe reversible items pre-selected; anything touching data,
   network, or user workflow unselected. A detail view shows the full
   evidence for any row.
3. **Selection**: user toggles rows, or accepts the default set. No free-text
   config needed; the selection IS the config.
4. **Plan generation**: emit a deterministic execution plan
   (JSON + readable markdown): ordered steps, each with exact command,
   identity-verification rule, rollback command, and expected measurement.
5. **Delegation**: the plan is handed to an agent (or the user's own harness)
   as one bounded packet: target, change, acceptance criteria, non-goals.
   The agent executes step by step, verifying identity before every
   destructive action, and reports per-step before/after evidence.
6. **Verify and report**: re-run the scan; show what changed, what did not,
   and the full undo sheet.

## Properties

- Read-only until the user confirms the selection.
- Everything in the plan is reversible; the undo sheet is generated before
  the first mutation.
- The tour works in terminal (TUI) and can be embedded in a dashboard.
- Plans are portable: the same JSON plan can be executed by any harness that
  implements the safety contract (identity checks, dry-run first, verify
  after).
- Integrates with the stale-process lifecycle protocol (harnessreap): stale
  and orphan candidates surface in the tour as a category.

## Non-goals

- Not an autopilot that mutates without a selection.
- Not a recommender that hides evidence behind scores.
