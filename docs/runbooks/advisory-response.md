# Runbook: Supply-Chain Advisory Response (RR-002)

Purpose: how to respond when the `supply-chain` CI job goes red — i.e. when
`cargo audit` reports a RustSec advisory or `cargo deny check` reports a denied
license / banned crate / disallowed source.

Policy (OR-005, spec Clarification 2026-06-11): **always hard-fail.** The only
permitted waiver is an explicit, PR-reviewed, **dated** entry in `deny.toml` or
the cargo-audit ignore-list. There is **no** silent or CI-level override
(do not add `continue-on-error`, do not delete the scan step, do not loosen
`deny.toml` wholesale).

## Trigger

The `supply-chain (audit + deny)` job fails on a PR/push, or the daily scheduled
run fails on the default branch (a newly published advisory against an
already-merged dependency set — SC-004).

## Escalation order (try in this order; stop at the first that works)

### 1. Triage — understand the finding

Reproduce locally (see `ci-local-repro.md`):

```bash
cargo audit
cargo deny check
```

Identify the offending item from the output:
- `cargo audit` → advisory id (`RUSTSEC-YYYY-NNNN`), affected crate + version,
  and whether it is reachable / has a patched version.
- `cargo deny check` → which check fired (`advisories`, `licenses`, `bans`,
  `sources`) and the specific crate / license / source.

Determine if it is a **direct** dependency (we control its version) or
**transitive** (pulled in by another crate).

### 2. Patch / upgrade (preferred fix)

- **Patched version exists**: bump it.
  - Direct dep: update the version in the owning crate's `Cargo.toml`.
  - Transitive dep: `cargo update -p <crate>` to pull the patched version, or
    bump the intermediate dependency that requires it.
- Commit the updated `Cargo.lock` (CI builds with `--locked`, so the lockfile
  must reflect the fix).
- Re-run `cargo audit` / `cargo deny check` locally to confirm green, then push.

For a **license** finding (`cargo deny check licenses`): if the license is
genuinely acceptable for the project, add it to the `allow` list in `deny.toml`
in a reviewed PR (this is a policy change, not a waiver). If it is not
acceptable, replace or drop the dependency.

For a **banned crate / source** finding: remove or replace the dependency; do
not relax `[bans]` / `[sources]` to accommodate it without review.

### 3. Last-resort waiver (only if no patch and the risk is accepted)

Use only when there is no fixed version and the advisory is not exploitable in
RONin's usage (e.g. the vulnerable code path is unreachable). The waiver MUST be
PR-reviewed and dated, and SHOULD carry an expiry/review note.

**cargo-audit ignore** — add the advisory id to `deny.toml` (cargo-deny reads
the advisories database too) under `[advisories].ignore`:

```toml
[advisories]
version = 2
yanked = "deny"
ignore = [
  # RUSTSEC-2026-0001: <crate> — no fixed release; vulnerable path unreachable
  # because RONin never calls <fn>. Reviewed 2026-06-11 (PR #NN). Re-review by
  # 2026-09-11 or when a patched release lands.
  "RUSTSEC-2026-0001",
]
```

For the standalone `cargo audit` CLI the equivalent is
`cargo audit --ignore RUSTSEC-2026-0001`, but prefer the `deny.toml` entry so a
single reviewed file is the source of truth for both scanners.

Every waiver entry MUST include, in a comment:
- the advisory id and crate,
- the justification (why it is safe / why no upgrade is possible),
- the review date and reviewing PR number,
- a re-review trigger (date or "when patched release available").

## After resolution

- Confirm `cargo audit` and `cargo deny check` are green locally.
- Push; confirm the `supply-chain` job is green in CI.
- For a scheduled-run failure on the default branch, open the fix/waiver PR
  promptly — the daily scan will keep failing until merged.

## Do NOT

- Add `continue-on-error: true` to the scan steps.
- Delete or comment out the `cargo audit` / `cargo deny check` steps.
- Blanket-disable a `deny.toml` check (e.g. set `advisories` to `allow`).
- Merge a waiver without a dated, justified, reviewed entry.
