# CleanOS Privacy Model

## Stance

CleanOS is local-first. Evidence collection, ranking, plan generation, mutation, and verification all run on the user's Mac. MVP ships with no telemetry and no phone-home analytics.

## What "evidence" means

Evidence is a local measurement packet: the probe or command used, a timestamp, a raw artifact path, and a FACT or INFERENCE label. Evidence proves what is happening on this machine. It is working material for ranking and approval, and it stays under user control.

Typical evidence includes:

- CPU load samples and top process identity (PID, PPID, command path)
- Memory, swap, and compressor pressure snapshots
- Thermal-pressure log events when available
- Display topology summaries
- Launch-item inventory and activity signals
- Before/after deltas for approved experiments

## What stays on the machine

By default, CleanOS keeps all of the following local:

- Raw probe output and run logs
- Structured reports (JSON and Markdown)
- Guided-tour selections and execution plans
- Experiment state and undo sheets
- Identity-validation records for process actions

Default local data root: `~/Library/Application Support/CleanOS/`.

## What never leaves the machine (MVP)

MVP does not upload:

- Process lists, command lines, or working-directory paths
- Hardware serials, UUIDs, UDIDs, or enrollment state
- Credentials, tokens, cookies, or keychain material
- Home-absolute usernames in exportable form without redaction
- Continuous usage analytics or crash telemetry

## Export and sharing

Optional export (if added later) is explicit and opt-in. Exportable reports redact serials, UUIDs, credentials, and home-absolute usernames by default. Public case studies use aggressive anonymization. Share paths are user-initiated; CleanOS waits for consent before any copy leaves the device.

## Open-source transparency

CleanOS is MIT-licensed and public. Users can inspect collectors, remediations, redaction rules, and approval gates in source. Privacy claims are verified by reading the code and the safety docs, not by trusting a marketing page.

## Alignment with product safety

Privacy and safety share the same floor: measure before mutate, keep evidence local by default, anonymize public docs, and require user approval before any mutation. See `docs/SAFETY.md` and `docs/concept/safety-policy.md`.
