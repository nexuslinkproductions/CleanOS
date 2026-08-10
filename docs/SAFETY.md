# CleanOS Safety

## Floor

- Measure before mutate
- Prefer reversible experiments
- Validate process identity before any kill or stop action
- Keep evidence local by default
- Anonymize public case studies

## Forbidden product behaviors

- Cache nuking presented as performance therapy
- Snapshot deletion for speed claims
- Identity-blind process killing by name alone
- SIP disablement guidance
- Unsupported overclock or NVRAM folklore
- Default telemetry or silent uploads

## Process actions

Before terminating a process, require:

1. Exact command path match
2. Parent process context
3. Duration and CPU evidence
4. User approval for non-orphan cases
5. Prefer SIGTERM, escalate only when validated and necessary

## Protected surfaces

- Credentials and secrets files
- System integrity settings
- User identity and enrollment state
- Unrelated project working trees

## Rollback

Every proposed mutation must include:

- how to undo it
- what evidence proves it helped
- what evidence would prove it failed
