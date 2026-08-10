# CleanOS Launch Plan

## First-release posture

Launch as early docs plus MVP SPEC, then ship a CLI alpha that can collect evidence and run one reversible experiment class. Public messaging stays evidence-first and local-first.

## GitHub launch checklist

### Repository hygiene

- [ ] MIT `LICENSE` present and linked from README
- [ ] Root README follows voice rules in `docs/voice.md`
- [ ] `CONTRIBUTING.md` explains docs-first contributions and safety floor
- [ ] `CODE_OF_CONDUCT.md` present
- [ ] Issue templates for bug, evidence report, and feature request
- [ ] Security policy explaining local data handling and how to report issues
- [ ] Topics set: `macos`, `apple-silicon`, `performance`, `cli`, `privacy`, `open-source`
- [ ] Default branch protected once CI exists
- [ ] Social preview image uploaded (Open Graph)

### README structure (target)

1. One-line pitch
2. Badges
3. Why this exists (anonymized incident pointer)
4. Principles
5. Quickstart (when CLI lands)
6. Documentation table
7. Privacy stance
8. Current status
9. License and Apple trademark disclaimer

### Badges (initial)

- License: MIT
- Platform: macOS (Apple Silicon)
- Status: early docs, then alpha when CLI ships
- Optional later: CI, Homebrew formula, release version

## Social preview image spec

- Size: 1280 x 640 px (GitHub Open Graph)
- Format: PNG
- Background: dark, high contrast
- Foreground text:
  - Title: CleanOS
  - Subtitle: Evidence-first macOS performance
  - Supporting line: Measure. Rank. Approve. Verify.
- Visual motif: simple metric cards (load, swap, process identity) plus a checkmark approval gate
- Avoid cluttered cleaner-app iconography and fake gauge needles
- Leave safe margin so text remains readable when cropped

## First-release scope

Ship when these exist:

1. Public docs pack (INTAKE, SCOUT, ARCHITECTURE, SAFETY, SPEC, voice, features, concept)
2. CLI stubs or working `collect` and `report` paths with local artifact output
3. One reversible experiment class with approval gate and before/after measurement
4. Redaction defaults for exportable reports
5. Test coverage for identity validation and approval refusal
6. Anonymized case study link from README to `docs/SCOUT.md`

Out of first release:

- Autopilot mutation
- Cloud sync or telemetry
- Broad cache deletion features
- App Store packaging
- Full GUI dashboard

## 30-day plan

### Days 1-7: Public foundation

- Finalize README and social preview
- Publish concept pack and feature docs links
- Open issues for collector probes and report schema
- Record naming decision (keep CleanOS or switch)

### Days 8-14: Collector MVP

- Implement read-only snapshot: CPU, processes, memory/swap, thermal events, display topology, power state
- Write JSON schema and local store under Application Support
- Add FACT vs INFERENCE labeling in report renderer

### Days 15-21: Tour and one experiment

- Implement candidate ranking for orphan/high-CPU process clusters
- Ship guided tour selection in terminal
- Execute approved identity-validated orphan remediation with rollback notes
- Capture before/after verification report

### Days 22-30: Hardening and announce

- Tests for redaction, identity checks, dry-run defaults, kill-switch
- Packaging notes for local install
- Cut alpha tag
- Announce with measured anonymous case narrative and clear status label

## Success signals for day 30

- Fresh clone can collect a local evidence report
- One approved mutation path works with undo sheet
- Public docs pass voice and safety review
- At least one external contributor can open an evidence-backed issue using the template

## Launch messaging cues

Lead with measurement, reversible change, and user approval. Point to the anonymized 2026-08-10 orphan process case as motivation. Keep status labels honest: early docs, then alpha.
