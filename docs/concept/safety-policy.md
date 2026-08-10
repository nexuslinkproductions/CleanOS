# CleanOS Safety Policy

## Floor

CleanOS keeps every mutation behind measurement, identity validation, and explicit approval. The default posture is dry-run. The product publishes reversible experiment cards and requires user confirmation before any write.

## Defaults

| Setting | Default |
|---------|---------|
| Scan / collect | Read-only |
| Plan generation | Dry-run only |
| Mutation | Off until explicit approval |
| Process action | Prefer SIGTERM after identity match |
| System agents (`com.apple.*`) | Report only |
| SIP / NVRAM / overclock guidance | Unavailable as product features |
| Telemetry | Off |

## Approval gate

Before any mutation, CleanOS shows:

1. What will change (exact targets)
2. Why (FACT evidence)
3. Expected payoff class and risk class
4. Exact rollback steps
5. Verification probes that will run after

User approval is per selection set (tour) or per experiment (single runner). Silent mutation is unavailable.

## Identity validation (process actions)

Require all of:

1. Exact command path match
2. Parent process context (PPID)
3. Duration and CPU evidence
4. User approval for non-orphan cases
5. Re-check identity immediately before signal delivery

Escalate to SIGKILL only after SIGTERM fails and identity is revalidated.

## Revert plans

Every approved plan includes an undo sheet generated before the first mutation:

- exact reverse command or restore path
- when rollback is automatic vs manual
- what evidence proves help
- what evidence proves failure

Login-item disables prefer moving plists to a `disabled/` directory or recording `launchctl` undo commands.

## Kill-switch

Operators can stop CleanOS mutation mid-run:

1. Abort the current plan (no further steps)
2. Leave completed steps intact with their undo sheet
3. Offer immediate rollback for completed reversible steps
4. Keep the evidence store and report for review

The kill-switch is a first-class CLI/TUI control, not a hidden flag.

## Forbidden product behaviors

- Cache nuking presented as performance therapy
- Snapshot deletion for speed claims
- Identity-blind process killing by name alone
- SIP disablement guidance
- Unsupported overclock or NVRAM folklore
- Default telemetry or silent uploads
- Autopilot that mutates without a selection

## Protected surfaces

- Credentials and secrets files
- System integrity settings
- User identity and enrollment state
- Unrelated project working trees
- Apple system launch agents beyond reporting

## Evidence note (policy learning)

On 2026-08-10, Adobe agents looked dead by stale crash-log mtimes, then live process and recent log activity reclassified them as active. Policy consequence: log mtime alone is insufficient evidence. Combine process state, log activity, and product usage before disable proposals.
