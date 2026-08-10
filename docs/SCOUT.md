# SCOUT - CleanOS

**Phase:** SCOUT (evidence + research questions; no product coding)  
**Status:** Skeleton - research open  
**Date:** 2026-08-10

## 1. Originating incident (anonymized case skeleton)

> Aggressive anonymization: no hardware serials, UUIDs, credentials, tokens, or home-absolute usernames. Paths use placeholders. Numbers are directional field notes, not product KPIs.

### Case ID

`CASE-2026-08-10-orphan-mcp-cpu` (working title)

### Environment (sanitized)

| Field | Value |
|-------|-------|
| Machine class | Apple Silicon MacBook Pro (M2 Pro class) |
| Logical CPUs | 12 (P+E mix) |
| Unified memory | 16 GB class |
| OS | recent macOS 26.x |
| Power | AC adapter connected; Low Power Mode off |
| Displays | 3 active: high-refresh external main + built-in + rotated portrait external |
| Storage | Internal SSD healthy; free capacity not the bottleneck |

### Incident summary (facts)

1. **Acute CPU cluster:** Six parentless developer-tool MCP server processes (PPID = launchd/1) each consumed roughly 80-98% of one core for several hours.
2. **Aggregate drain:** Combined load approximated **~5.4 continuously occupied cores** during the hot window (rounded aggregate across samples; not a single instrumented counter).
3. **Identity evidence:** Commands resolved to a project-local MCP CLI invoked via a shared wrapper; sampled process had no network sockets; stack sample indicated an uncaught-exception busy loop.
4. **Lifecycle failure:** `SIGTERM` did not stop the validated set; after re-checking PID/PPID/command identity, `SIGKILL` terminated them.
5. **System state (hot window):** Very high 1-minute load (~21 on 12 CPUs), near-zero idle, heavy memory/swap pressure, compressor active.
6. **Thermal:** Unified logs showed Moderate/Heavy **thermal-pressure** events while the runaway cluster was alive. Thermal-pressure ≠ proven frequency throttle (power/frequency counters were not available without elevated auth).
7. **Directional workload probe:** A matched single-run encode-style probe showed substantially lower wall time after termination. Directional only - not a multi-run acceptance benchmark.
8. **Post-termination:** Original orphan PIDs absent; idle/load improved; residual actors remained (compositor/WindowServer, Electron desktop helpers, storage UI extensions when open, media apps, vendor audio services). Swap allocation remained elevated (chronic secondary pressure).

### What was *not* changed

No power-management writes, Spotlight disablement, cache deletion, APFS snapshot deletion, SIP/NVRAM changes, fan overrides, or generic “cleaner” installs were performed as remediation.

### Product lessons (inferences - labeled)

| Lesson | Confidence | Notes |
|--------|------------|-------|
| Lifecycle leakage in long-lived agent/MCP tooling is a high-leverage real-world pain | High | Mechanism + identity confirmed |
| Measure → identity-check → reversible terminate beats folklore cleans | High | Matches CleanOS safety floor |
| Multi-display / high-refresh topology is a plausible residual cost | Medium | Needs A/B; correlation ≠ proof |
| Electron/agent desktop self-load can dominate after acute orphans are gone | High as residual candidate | Needs idle-window experiments |
| Swap high after kill ≠ “kill fixed memory” | High | Adversarial review flagged causality overclaim |

### Open recurrence work (outside CleanOS product code)

- Map every launcher that can spawn the MCP seam (host configs differ).
- Add lifecycle tests: stdin EOF, signal forward, grace → escalate, no leftover children.
- Prefer a single supervised launch seam; avoid duplicate host registrations.

---

## 2. Competitor / alternative landscape (research questions)

Questions for PLAN / later SCOUT passes - **not** filled claims:

1. Which “Mac cleaner” products still market RAM freeing, “junk” deletion, or one-click optimize without matched before/after metrics?
2. Which Apple-native tools already cover parts of the job (Activity Monitor, `sysdiagnose`, Console, Instruments) and where do they fail for non-experts?
3. Which developer-facing tools diagnose MCP/agent runaway processes, and are they discoverable to non-developers?
4. Pricing and trust: paid cleaner market vs open-source audit tools - where does evidence-first open source win?
5. Legal / App Store constraints for process management and automation on modern macOS.
6. Naming collisions and trademark risk for “CleanOS” and shortlist alternatives (separate naming task).

---

## 3. Pain-point hypotheses (to validate)

| ID | Hypothesis | How to falsify |
|----|------------|----------------|
| P1 | Users feel “M-series is throttled” when the real cause is background software | Survey + case taxonomy with measured causes |
| P2 | Agent/MCP tooling creates orphan CPU loops more often than users realize | Public issue mining + synthetic reproduction |
| P3 | Cleaner apps train harmful habits (cache wipe, login-item thrash) | Content analysis of top apps’ claims vs Apple guidance |
| P4 | Multi-monitor creators accept WindowServer tax as inevitable | A/B refresh-rate / display-count experiments |
| P5 | Privacy fear blocks cloud “optimizer” products; local-first is a wedge | Privacy-sensitive interview / Reddit themes |

---

## 4. Reddit research plan (privacy-safe)

### Goals

- Collect **themes**, not doxxing.
- Prefer aggregate quotes and paraphrases.
- Never publish usernames, profile URLs, or verbatim posts that re-identify the OP without consent.

### Target communities (indicative)

- r/MacOS, r/MacBookPro, r/apple, r/MacApps  
- r/ClaudeAI / coding-agent communities (MCP / runaway process themes)  
- r/DataHoarder or sysadmin-adjacent only if privacy norms allow

### Method

1. Search queries (examples): `mac slow after update`, `WindowServer high CPU`, `purge RAM mac`, `cleaner safe?`, `MCP high CPU`, `kernel_task thermal`.
2. Code themes: folklore fixes, real root causes, trust/privacy objections, willingness to run local CLI.
3. Store notes as **paraphrase + subreddit + month/year** - strip usernames.
4. Separate **marketing claims** (competitor threads) from **lived incidents**.
5. No engagement that harvests PII; no DM outreach from research accounts without an ethics pass.

### Deliverable for PLAN

A short `docs/research/reddit-themes.md` (future) with: top 10 pains, top 10 folklore remedies, opportunity gaps CleanOS can own.

---

## 5. SCOUT exit criteria

SCOUT scaffolding is complete when:

- [x] Anonymized incident skeleton exists in-repo  
- [x] Competitor and pain research questions listed  
- [x] Privacy-safe Reddit plan written  
- [ ] Naming collision check executed (deferred)  
- [ ] Reddit theme pass completed (deferred)  
- [ ] Competitor matrix draft completed (deferred)

Next phase: **PLAN** - turn INTAKE + SCOUT into an implementable MVP goal tree with acceptance criteria (see open questions in the phase handoff).
