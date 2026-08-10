# CleanOS Positioning

## One-line positioning

CleanOS measures bottlenecks on macOS, ranks reversible fixes by evidence, and applies only what you approve.

## Audience

- Developers and power users who keep many tools, agents, and background services running
- Operators who want matched before/after measurement for every change
- Privacy-sensitive Mac owners who keep performance work local

## Core promise

CleanOS turns a messy "my Mac feels slow" feeling into a ranked tour of candidates. Each candidate carries measured evidence, expected gain, risk, and a rollback path. You select the set. CleanOS (or your agent) executes the approved plan and re-measures.

The originating signal is a real Apple Silicon incident class: orphan developer-tool MCP servers, memory and swap pressure, display-compositor cost, and leftover UI or media helpers. CleanOS is built to surface that class of proof and keep the user in control of every mutation.

## How CleanOS differs

| Product class | Typical move | CleanOS move |
|---|---|---|
| CleanMyMac-style cleaners | One-click junk and RAM rituals | Measure, classify, rank, then mutate only with approval |
| Onyx-style maintenance utilities | Broad system maintenance menus | Guided tour over evidence-backed candidates with rollback notes |
| TCC / privacy-grant tools | Permission and quarantine management | Performance evidence and reversible remediation, not permission theater |

### Evidence-first

Every candidate shows the probe, timestamped values, and a FACT vs INFERENCE label. Marketing language stays out of the report body.

### Reversible

Default actions are dry-run. Mutating actions ship with an undo sheet before the first write. Process stops require PID, PPID, and command identity validation.

### Measurement-driven

Success means matched before/after probes for the same metric set. Directional anecdotes stay labeled as directional. The product sells proof and ranked experiments.

## Competitive wedge

Open source, MIT licensed, local-first. Users can read the probes, the plan schema, and the safety contract. The guided optimization tour is the product surface: select, approve, execute, verify.

## Status framing

Public copy uses: early docs, MVP SPEC, alpha, beta. Product claims stay tied to shipped acceptance criteria in `SPEC.md`.