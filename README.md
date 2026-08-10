# CleanOS

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-black.svg)](#)
[![Status](https://img.shields.io/badge/status-INTAKE%20%2F%20SCOUT-blue.svg)](docs/INTAKE.md)
[![Built with](https://img.shields.io/badge/built%20with-SSSF-6f42c1.svg)](docs/ARCHITECTURE.md)

**CleanOS collects measurable macOS performance evidence, proposes reversible experiments, and reports before/after results with explicit fact versus inference boundaries.**

## What it does

- Baseline CPU, memory, thermal pressure, display topology, and background services.
- Identify high-confidence contention sources with identity-checked process evidence.
- Run matched before/after probes after a single reversible change.
- Keep case studies local-first and anonymized for public docs.

## Why this exists

Macs often feel slow because of software lifecycle leakage, background contention, display topology cost, or memory pressure. CleanOS turns those hunches into measured deltas.

An anonymized originating incident (orphan developer-tool MCP servers burning multiple CPU cores for hours) lives in [`docs/SCOUT.md`](docs/SCOUT.md). That case informs the product thesis. It does not claim CleanOS has already shipped product-level measured results.

## Principles

1. **Measure before mutate** — baselines and matched probes precede any change.
2. **Reversible only** — every experiment includes a documented rollback path.
3. **Identity-checked actions** — process remediation requires PID, PPID, and argv validation.
4. **Local-first privacy** — evidence stays on-device by default.
5. **Fact versus inference** — reports separate measured observations from hypotheses.

## Documentation

| Doc | Purpose |
|-----|---------|
| [`docs/INTAKE.md`](docs/INTAKE.md) | Problem, users, non-goals, safety floor |
| [`docs/SCOUT.md`](docs/SCOUT.md) | Incident case skeleton, competitor/pain research, Reddit plan |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | High-level system shape |
| [`docs/SAFETY.md`](docs/SAFETY.md) | Protected paths, approval gates, mutation policy |
| [`docs/voice.md`](docs/voice.md) | Product-copy constraints |
| [`SPEC.md`](SPEC.md) | Deterministic MVP SPEC for later BUILD |
| [`sssf.config.yaml`](sssf.config.yaml) | SSSF phase stub |

## Current phase (SSSF)

```
INTAKE → SCOUT → PLAN → BUILD → VALIDATE → REVIEW → SHIP
  ■        ■       □       □         □         □       □
```

This repository currently holds **INTAKE + SCOUT scaffolding only**.
No product runtime, CLI, or MCP server is implemented yet.

## Privacy stance

- Default: all collection and reports are **local**.
- Case studies and public docs use aggressive anonymization (no serials, UUIDs, credentials, or home-absolute usernames).
- Optional share/export, if added later, will be explicit, opt-in, and redacted.

## License

[MIT](LICENSE) © Nexus Link Productions contributors.

## Disclaimer

Apple, macOS, and related marks are trademarks of Apple Inc. CleanOS is an independent project and is not affiliated with, endorsed by, or sponsored by Apple Inc.
