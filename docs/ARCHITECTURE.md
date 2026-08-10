# CleanOS Architecture

## Shape

CleanOS is a local-first evidence and remediation toolkit for macOS on Apple silicon.

Pipeline:

1. Collect local performance evidence
2. Rank reversible interventions by measured payoff
3. Apply only user-approved mutations
4. Report before/after results with fact vs inference boundaries

## Core components (planned)

| Component | Role |
|-----------|------|
| Collector | Read-only probes: CPU, memory, swap, thermal pressure, display topology, process identity |
| Evidence store | Local JSON/MD artifacts with timestamps and command provenance |
| Ranker | Scores reversible experiments by expected payoff and risk |
| Approver | Explicit user gate before any mutation |
| Remediator | Bounded, reversible actions with rollback notes |
| Reporter | Before/after deltas; labels facts separately from inferences |

## Non-architecture

- No cloud telemetry in MVP
- No identity-blind process killing
- No destructive cleans as product features
- No unsupported silicon unlocking claims

## Evidence model

Every claim should carry:

- command or probe used
- timestamp
- raw artifact path
- fact vs inference label
- rollback path when a mutation is proposed

## MVP focus

1. Incident capture for orphan/high-CPU process clusters
2. Display topology and WindowServer cost measurement
3. Memory/swap pressure reporting without fake RAM freeing
4. Ranked reversible experiment cards with approval gates
