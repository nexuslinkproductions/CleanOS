# CleanOS

**Evidence-first macOS performance.** CleanOS measures bottlenecks, ranks reversible fixes by evidence, and applies only what you approve.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20(Apple%20Silicon)-black.svg)](#)
[![Status](https://img.shields.io/badge/status-early%20docs-blue.svg)](docs/INTAKE.md)

CleanOS is a local-first toolkit for collecting measurable performance evidence, proposing reversible experiments, and reporting before/after results with explicit fact vs inference boundaries. It is built as an evidence product, distinct from junk-file cleaners or RAM-booster folklore apps.

## Why this exists

Modern Macs often feel constrained when the real problem is software lifecycle leakage, background contention, display topology cost, or memory pressure. CleanOS is designed to surface proof, rank reversible interventions, and keep the user in control of every mutation.

An anonymized originating incident (orphan developer-tool MCP servers consuming multiple CPU cores for hours) is documented in [`docs/SCOUT.md`](docs/SCOUT.md). That case informs the product thesis. It does not claim that CleanOS has already shipped measured product results.

## Principles

1. **Measure before mutate.** Baselines and matched probes precede any change.
2. **Reversible only.** Every experiment ships with a documented rollback path.
3. **Safe by default.** Product features exclude destructive cleans, cache nuking, snapshot deletion, and identity-blind process killing.
4. **Local-first privacy.** Evidence stays on-device by default. MVP has no telemetry phoning home.
5. **Evidence over folklore.** Claims require matched before/after measurement. Unsupported overclock and SIP-bypass narratives are out of scope.

## Documentation

| Doc | Purpose |
|-----|---------|
| [`docs/INTAKE.md`](docs/INTAKE.md) | Problem, users, non-goals, safety floor |
| [`docs/SCOUT.md`](docs/SCOUT.md) | Incident case skeleton, competitor/pain research, Reddit plan |
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | High-level system shape |
| [`docs/SAFETY.md`](docs/SAFETY.md) | Protected paths, approval gates, anti-folklore |
| [`docs/voice.md`](docs/voice.md) | Product copy rules |
| [`SPEC.md`](SPEC.md) | Deterministic MVP SPEC for later implementation |

## Current status

This repository currently holds early product docs and an MVP SPEC only.
No product runtime, CLI, or MCP server is implemented yet.

## Privacy stance

- Default: all collection and reports are local.
- Case studies and public docs use aggressive anonymization (no serials, UUIDs, credentials, or home-absolute usernames).
- Optional share/export (if ever added) will be explicit, opt-in, and redacted.

## License

[MIT](LICENSE) © Nexus Link Productions contributors.

## Disclaimer

Apple, macOS, and related marks are trademarks of Apple Inc. CleanOS is an independent project and is not affiliated with, endorsed by, or sponsored by Apple Inc. Nothing here claims to unlock unsupported hardware clocks or bypass system integrity protections.
