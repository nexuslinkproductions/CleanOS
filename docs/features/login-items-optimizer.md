# Feature: Login Items Optimizer

**Status:** Intake / proposed
**Product:** CleanOS
**Related:** guided-optimization-tour.md

## Problem

Developer machines accumulate launch agents and daemons: updaters, sync
helpers, crash reporters, peripheral managers, and harness-supervisor jobs.
Most sit idle at 0% CPU but add wakeups, RAM, login-time work, and log growth.
Some are genuinely dead (no activity in months) but keep loading at every
login. Users cannot tell which are safe to disable without deep-diving plists
and process histories.

## Behavior

1. **Inventory**: enumerate user LaunchAgents, system LaunchAgents, and
   LaunchDaemons. For each: label, program path, KeepAlive/RunAtLoad/interval
   flags, load state, current PID, RSS, CPU, wakeups, and last activity
   evidence (process uptime, log file mtimes, filesystem access).
2. **Classify** each item into one of:
   - `active` (running, nonzero recent cost, used by a product the user runs)
   - `idle` (loaded but 0 cost; safe to keep, cheap to disable)
   - `dead` (no activity for a long window, product not in use: evidence from
     log mtimes and process history)
   - `unknown` (needs user input)
3. **Present** a grouped checklist (see guided-optimization-tour.md) with
   evidence per item: cost now, last activity, what breaks if disabled.
4. **Execute**: for each selected item, disable in a reversible way:
   - move the plist to a `disabled/` directory (takes effect at next login),
   - or `launchctl bootout` for immediate effect,
   - record the exact undo command in the plan report.
5. **Verify**: re-scan after next login; report what changed and what did not.

## Safety rules

- Never disable an item whose product is in active use (evidence required).
- Never touch Apple system agents (`com.apple.*`) beyond reporting.
- Default is report-only; disabling is explicit opt-in per item.
- Every action ships with its exact reversal.

## Evidence note (from 2026-08-10 host)

- Adobe CC agents present; Adobe log activity stopped Nov 2025 (9 months idle)
  while 457 MB of stale crash logs accumulated. Class: `dead`.
- Perplexity/Comet updaters: product in daily use. Class: `active`.
- Google keystone / Dropbox updaters: tied to products in use; cost low.
