# CleanOS Intake

## Problem

Macs often feel constrained when the real cause is measurable software contention: orphan processes, memory pressure, display topology cost, indexing spikes, or background services. Generic cleaner apps sell rituals. CleanOS sells proof.

## Users

- Power users and developers who keep many tools running
- People who want reversible, evidence-backed performance work
- Operators who refuse folklore maintenance scripts

## Product thesis

Collect local performance evidence. Rank reversible interventions by measured payoff. Apply only what the user approves. Report before and after results with explicit fact vs inference boundaries.

## Non-goals

- Junk-file or RAM-booster folklore
- Destructive cleans (cache nuking, snapshot deletion)
- Identity-blind process killing
- Unsupported overclock or SIP-bypass guidance
- Telemetry by default

## Safety floor

- Measure first
- Prefer reversible experiments
- Require identity validation before process actions
- Keep evidence local by default
- Anonymize any public case study

## Originating signal

An anonymized developer-tool MCP orphan incident (multiple parentless Node processes consuming several CPU cores for hours) informs the thesis. See `docs/SCOUT.md`. That incident is a case study input, not a claim that CleanOS already shipped product results.
