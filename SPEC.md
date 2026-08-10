# CleanOS MVP SPEC (deterministic)

**Phase:** SPEC frozen for later PLAN → BUILD  
**Status:** Draft acceptance contract — **no product runtime exists yet**  
**Non-claim:** This SPEC does **not** assert that CleanOS has already produced measured end-user product results.

---

## 1. MVP goal

Ship a **local CLI** that:

1. Collects a read-only evidence snapshot on Apple Silicon macOS.
2. Produces a structured report with fact vs inference labels.
3. Can propose **and optionally apply** a **single** reversible experiment with before/after measurement and rollback.

Scope is intentionally narrow. GUI, MCP, cloud, and auto-clean packs are out of MVP.

---

## 2. Acceptance criteria (must all pass)

### AC-1 — Evidence collection (read-only)

- [ ] `cleanos evidence collect` exits 0 on a supported Mac without writing outside the write boundary.
- [ ] Output includes at least: timestamp, hostname hash (or redacted hostname), CPU model/core counts, memory size, load averages, top-N processes (pid/ppid/%cpu/%mem/command), `vm_stat` summary, swap usage, thermal/power summary available without sudo.
- [ ] Collection completes without requiring administrator password for the default probe set.
- [ ] Report JSON validates against a checked-in schema (`schemas/evidence.v1.json` — to be added in BUILD).

### AC-2 — Report clarity

- [ ] Human-readable markdown report lists **Confirmed**, **Hypothesis**, and **Deferred experiment** sections.
- [ ] No claim may appear in Confirmed without a cited probe field.
- [ ] Report contains an explicit disclaimer that single-run deltas are directional until repeated matched runs exist.

### AC-3 — Reversible experiment runner

- [ ] `cleanos experiment list` shows only experiments registered in an allowlist manifest.
- [ ] `cleanos experiment run <id> --dry-run` shows planned mutations and writes nothing durable outside the planned log path.
- [ ] `cleanos experiment run <id>` requires interactive approval (or `--approve <token>` from a prior dry-run).
- [ ] Runner records before snapshot → mutation → after snapshot → delta summary.
- [ ] `cleanos experiment rollback <run-id>` restores the pre-mutation state for that experiment or fails loudly with a supportable error.

### AC-4 — Safety gates

- [ ] Attempts to target protected paths (see [`docs/SAFETY.md`](docs/SAFETY.md)) are rejected before mutation.
- [ ] Process-stop experiments require exact argv + ppid identity checks; name-only kill is rejected.
- [ ] Destructive clean packs (cache wipe, snapshot delete, purge-as-feature) are **absent** from the allowlist.

### AC-5 — Privacy

- [ ] Default report redacts home directory usernames, serials, hardware UUIDs, and credential-looking strings.
- [ ] No network upload occurs in MVP binaries/scripts (verified by absence of telemetry endpoints + offline smoke).

### AC-6 — Packaging / DX

- [ ] README documents install/run for MVP CLI.
- [ ] Automated tests cover: schema validation, redaction helpers, protected-path rejection, dry-run approval gating.
- [ ] CI (when wired) runs unit tests on push; no secrets required.

---

## 3. Write boundaries (BUILD agents and runtime)

### 3.1 Repository write boundary (SSSF BUILD)

Allowed during BUILD (subject to PLAN):

- `src/` or `cleanos/` (implementation)
- `tests/`
- `schemas/`
- `docs/` (documentation only)
- `SPEC.md`, `README.md`, `.gitignore`
- packaging manifests (`pyproject.toml` / `package.json` — choose one stack in PLAN)

Forbidden without explicit engineer approval:

- `adws/` factory modules (if/when stamped)
- secrets, `.env`, credentials
- unrelated sibling repositories

### 3.2 Runtime write boundary (product)

Allowed:

- `~/Library/Application Support/CleanOS/` — reports, run logs, experiment state
- optional user-configured export directory

Forbidden:

- System and protected paths listed in [`docs/SAFETY.md`](docs/SAFETY.md)
- Silent writes under other apps' containers
- Network destinations

---

## 4. Non-functional requirements

| ID | Requirement |
|----|-------------|
| NF-1 | Prefer read-only probes that finish in < 30s on a healthy machine for default collect |
| NF-2 | Heavy benches (ffmpeg multi-run, sustained load) are opt-in and clearly labeled |
| NF-3 | Errors are actionable (what failed, what was not changed) |
| NF-4 | Deterministic CLI exit codes: 0 ok, 2 usage, 3 safety reject, 4 probe failure, 5 rollback failure |

---

## 5. Explicit non-goals for MVP

- GUI / menu bar app
- MCP server
- Cross-platform (Intel Mac may work later; MVP targets Apple Silicon)
- Automatic remediation without approval
- Competing with antivirus or backup products
- Claiming thermal-pressure logs alone prove frequency throttling

---

## 6. PLAN phase open questions (blockers / decisions)

1. **Language stack:** Python (rich macOS scripting) vs Node/TypeScript (ecosystem fit with MCP later)?
2. **First shipped experiment:** orphan MCP lifecycle stop-with-identity vs display refresh A/B vs launch-agent disable — which single experiment maximizes learning with minimal risk?
3. **Distribution:** Homebrew tap, plain pipx/uv tool, or signed `.pkg` later?
4. **Schema location and versioning policy** for evidence + experiment runs.
5. **How to detect “user busy / defer heavy probes”** without false negatives.
6. **Reddit SCOUT execution:** which subreddits and scrape method remain privacy-safe for a public repo?
7. **When to stamp full SSSF `adws/`** into this repo vs keep docs-only until PLAN is approved.
8. **Naming collision check** for “CleanOS” in App Store / GitHub / trademarks (follow-up legal/product).

---

## 7. Definition of done for BUILD (later)

MVP BUILD is done when AC-1…AC-6 are checked with deterministic test/command evidence in a VALIDATE envelope, then independently REVIEW'd. Until then, treat all product performance claims as **unimplemented**.
