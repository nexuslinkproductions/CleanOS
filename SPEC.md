# CleanOS MVP SPEC

**Status:** Draft acceptance contract. No product runtime exists yet.
**Non-claim:** This SPEC does not assert that CleanOS has already produced measured end-user product results.

## 1. MVP goal

Ship a local CLI that:

1. Collects a read-only evidence snapshot on Apple Silicon macOS.
2. Produces a structured report with fact vs inference labels.
3. Can propose and optionally apply a single reversible experiment with before/after measurement and rollback.

Scope is intentionally narrow: one evidence path, one report format, one approved mutation class.

## 2. Acceptance criteria

### AC-1 Evidence collection (read-only)

- [ ] Collects CPU load, top processes, memory/swap pressure, thermal-pressure log events when available, display topology summary, and power source state.
- [ ] Runs without sudo for the default path.
- [ ] Never kills processes, disables services, or writes system settings during collection.
- [ ] Report JSON validates against a checked-in schema (to be added during implementation).

### AC-2 Report clarity

- [ ] Every finding is labeled FACT or INFERENCE.
- [ ] Every proposed fix includes expected payoff class, risk class, and rollback steps.
- [ ] Public/exportable reports redact serials, UUIDs, credentials, and home-absolute usernames by default.

### AC-3 Reversible experiment runner

- [ ] Supports exactly one experiment class in MVP: identity-validated orphan process remediation for a user-approved matching rule.
- [ ] Requires explicit approval before mutation.
- [ ] Captures before and after metrics for the same probe set.
- [ ] Records rollback instructions and whether rollback was needed.

### AC-4 Safety gates

- [ ] Process actions require PID + PPID + command identity validation.
- [ ] Destructive cleans are unavailable as product features.
- [ ] No SIP disablement, cache nuking, snapshot deletion, or unsupported overclock guidance.

### AC-5 Privacy

- [ ] Local-first by default.
- [ ] No telemetry in MVP.
- [ ] Optional export is opt-in and redacted.

### AC-6 Packaging / DX

- [ ] Installable as a local CLI with a short README quickstart.
- [ ] `cleanos collect` and `cleanos report` work on a fresh clone after documented setup.
- [ ] scripts/verify.sh covers redaction, identity validation, and CLI surface checks against the live machine.

## 3. Write boundary

Allowed during implementation:

- CLI source under `src/` or equivalent
- tests
- schemas
- docs
- packaging manifests

Protected / out of scope for MVP mutation features:

- SIP and other OS integrity controls
- APFS snapshot deletion
- blanket cache deletion
- identity-blind process killing
- any cloud upload without explicit opt-in

## 4. Local data layout

Default local data root:

- `~/Library/Application Support/CleanOS/` for reports, run logs, and experiment state

No secrets are stored in the public repo.

## 5. Open decisions for later planning

1. Language/runtime for the CLI.
2. First shipped experiment class: orphan MCP lifecycle stop-with-identity vs display refresh A/B vs launch-agent disable.
3. Whether a GUI ships in v1 or CLI-only.
4. Exact report schema versioning policy.
5. Whether experiment history stays forever local or supports optional redacted export.

## 6. Done definition for this SPEC phase

This SPEC phase is complete when:

- acceptance criteria above are explicit and testable
- safety floor is documented in `docs/SAFETY.md`
- public copy follows `docs/voice.md`
- no product factory jargon appears in the public repo
