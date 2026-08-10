# CleanOS Naming Shortlist

Date checked: 2026-08-10.

Method:

- GitHub: `curl -s -o /dev/null -w "%{http_code}" https://github.com/NAME`
- npm: `curl -s -o /dev/null -w "%{http_code}" https://registry.npmjs.org/NAME`
- Homebrew formula page: `curl -s -o /dev/null -w "%{http_code}" https://formulae.brew.sh/formula/NAME`

HTTP 404 means the checked URL is unused. HTTP 200 means a collision exists at that surface.

## Shortlist results

| Candidate | GitHub | npm | Homebrew formula | Notes |
|-----------|--------|-----|------------------|-------|
| CleanOS | 200 | 404 | 404 | Current working name. Org repo already exists at `nexuslinkproductions/CleanOS`. Root `github.com/CleanOS` is taken by another presence. |
| EvidenceMac | 404 | 404 | 404 | Clear product meaning. Strong local availability signal. |
| ProbeOS | 200 | 200 | 404 | Collides on GitHub and npm. Drop for primary branding. |
| MeasureMac | 404 | 404 | 404 | Affirmative and measurement-forward. Strong availability. |
| RevMac | 404 | 404 | 404 | Signals reversible remediation. Short and brandable. |
| AuditOS | 200 | 404 | 404 | GitHub root collision. npm and Homebrew look free. |
| RankOS | 200 | 404 | 404 | GitHub root collision. Fits ranking thesis if brand ownership can be cleared. |

## Recommendation

Keep **CleanOS** as the shipped GitHub product name under `nexuslinkproductions/CleanOS` while the org owns the namespace.

Treat **EvidenceMac**, **MeasureMac**, and **RevMac** as the cleanest rename shortlist if trademark or root-name pressure rises. Prefer EvidenceMac when the pitch must lead with proof. Prefer MeasureMac when the pitch must lead with matched before/after probes. Prefer RevMac when reversibility is the lead differentiator.

## Follow-up checks (manual)

1. Trademark search in relevant classes for CleanOS and the three free shortlist names.
2. Social handle availability on X, Mastodon, and Discord.
3. App Store name reservation risk if a Mac App Store build ever ships.
4. Domain check for `.dev` and `.app` variants of EvidenceMac, MeasureMac, and RevMac.

## Working rule

Public docs keep using CleanOS until an explicit rename decision lands. Collision data here is a naming input, not a rename.
