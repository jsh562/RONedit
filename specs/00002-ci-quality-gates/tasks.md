# Tasks: CI Quality Gates

**Input**: Design documents from `specs/00002-ci-quality-gates/`
**Prerequisites**: `plan.md` (required), `spec.md` (required), `checklists/security.md`, `checklists/testing.md`

**Tests**: No unit/integration test tasks requested. Pipeline self-validation is covered by the actionlint + injected-failure tasks in the Polish phase (per plan Testing Strategy).

**Organization**: Operational spec — tasks grouped by objective (`OBJ#`). Shared workflow scaffolding that blocks every job is lifted to Foundational; per-objective job definitions stay in their objective phase.

## Project Mode

`Brownfield`

- The E001 Rust workspace already exists (`src/ron-core`, `src/ron-types`, `src/ronin-app` + the excluded `src/ron-core/fuzz`).
- This feature adds CI config (`.github/workflows/ci.yml`, `.github/dependabot.yml`), operator runbooks (`docs/runbooks/`), and ensures `Cargo.lock` is committed. NO changes under `/src`.
- Reuse the existing `deny.toml`, `rust-toolchain.toml` (pins stable + rustfmt/clippy + `wasm32-unknown-unknown`), and the root `[workspace].exclude = ["src/ron-core/fuzz"]`.

## Epic / Capability Map

- `[OBJ1]` → Cross-platform validation pipeline (fmt + clippy + 3-OS test matrix) — P1
- `[OBJ2]` → WASM-clean gate (`ron-core` → `wasm32-unknown-unknown`) — P1
- `[OBJ3]` → Supply-chain scanning (`cargo-audit` + `cargo-deny`, PR/push + daily cron) — P1
- `[OBJ4]` → Build caching + reproducibility (rust-cache + committed `Cargo.lock` + `--locked`) — P2

## Brownfield Notes

- Existing flows touched: none in `/src`. New config files only; the gated test suite is E001's.
- Reuse: `deny.toml` (advisories/licenses/bans/sources), `rust-toolchain.toml` (toolchain + wasm32 target), root `[workspace].exclude` (keeps the nightly-only `fuzz` crate off the stable matrix per OR-009/HINT-002).
- Compatibility concern: `Cargo.lock` MUST be committed before any job uses `--locked` (HINT-003), else CI fails on a missing lockfile.
- Regression focus: fork PRs must produce identical gate results with no secret (OR-011/SC-010); use the `pull_request` trigger, never `pull_request_target` (HINT-005).

---

## Phase 1: Setup (Repository / Workspace Delta)

**Repo-root config the whole pipeline depends on: the committed lockfile and the Dependabot pin-updater.**

- [X] T001 Ensure `Cargo.lock` is committed at the repo root (generate via `cargo generate-lockfile` if stale; stage it) so CI `--locked` builds resolve a fixed dependency set in s:\claudecode\RONedit\Cargo.lock {OR-007}
- [X] T002 [P] {OR-012} Add Dependabot `github-actions` package-ecosystem config (weekly) to keep SHA-pinned actions current in s:\claudecode\RONedit\.github\dependabot.yml

---

## Phase 2: Foundational (Cross-Work-Item Blockers)

**Create the single `ci.yml` shell every job lands in: triggers (PR/push + daily cron), least-privilege token scope, and the reusable toolchain/cache step pattern. All jobs in OBJ1–OBJ4 are added into this same file, so these tasks must complete first and the in-file job tasks below are sequential (not `[P]`) against it.**

- [X] T003 {OR-011} Scaffold `.github/workflows/ci.yml` with `name: CI` and triggers `on: pull_request` + `push` to the default branch, plus `schedule` (daily cron `0 0 * * *`, AD-004) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T001]
- [X] T004 {OR-012} Add top-level least-privilege `permissions: contents: read` block (no write scopes; not inheriting repo defaults) to s:\claudecode\RONedit\.github\workflows\ci.yml [after:T003]
- [X] T005 {OR-008} Establish the shared job step pattern (`actions/checkout` + `dtolnay/rust-toolchain` honoring `rust-toolchain.toml` stable pin) as the reusable head of every job in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T004]

---

## Phase 3: OBJ1 - Cross-platform validation pipeline (Priority: P1) 🎯 MVP

**fmt + clippy in one Linux `check` job; `cargo test` on a {ubuntu,windows,macos} matrix; discrete named jobs selectable as required checks; fork-safe, no secrets; fuzz crate excluded via `--workspace`.**

- [X] T006 [OBJ1] {OR-001,OR-002} Add the `check` job (Linux only, AD-006) running `cargo fmt --all -- --check` then `cargo clippy --workspace --all-targets -- -D warnings`; any drift/lint fails the job in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T005]
- [X] T007 [OBJ1] {OR-003,OR-009} Add the `test` job with `strategy.matrix.os: [ubuntu-latest, windows-latest, macos-latest]` (`fail-fast: true`) running `cargo test --workspace --locked`; `--workspace` excludes the root-excluded `fuzz` crate (OR-009) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T005]
- [X] T008 [OBJ1] {OR-011} Verify no `secrets.*` reference in any gate job and the `pull_request` (not `pull_request_target`) trigger is used so fork PRs validate identically (SC-010) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T007]
- [X] T009 [OBJ1] {OR-008} [COMPLETES OR-008] Confirm `check`, `test`, `wasm`, and `supply-chain` jobs all consume the same `rust-toolchain.toml` stable pin with no per-job channel override in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T012]
- [X] T010 [OBJ1] {OR-010} [COMPLETES OR-010] Confirm discrete, lowercase, individually-selectable job ids (`check`, `test`, `wasm`, `supply-chain`) suitable as required status checks (SC-008) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T012]

---

## Phase 4: OBJ2 - WASM-clean gate (Priority: P1) 🎯 MVP

**Compile `ron-core` to `wasm32-unknown-unknown` as a dedicated named job; failure (native/fs/async/UI dep leaking in) reds the run, enforcing ADR-0002.**

- [X] T011 [OBJ2] {OR-004} [COMPLETES OR-004] Add the `wasm` job (Linux only, AD-006) running `cargo build -p ron-core --target wasm32-unknown-unknown --locked`; a build failure fails the run (SC-003, IP-003) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T005]

---

## Phase 5: OBJ3 - Supply-chain scanning (Priority: P1) 🎯 MVP

**One named `supply-chain` job runs both scanners with SHA-pinned prebuilt tool installs; hard-fail on any advisory/disallowed license; runs on PR/push and the daily cron; the only waiver is a dated `deny.toml`/audit-ignore entry (documented in RR-002).**

- [X] T012 [OBJ3] {OR-005} Add the `supply-chain` job (Linux only) installing pinned `cargo-audit` + `cargo-deny` via `taiki-e/install-action` (AD-003) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T005]
- [X] T013 [OBJ3] {OR-005} Run `cargo audit` (RustSec advisories) and `cargo deny check` (reads root `deny.toml`, IP-002) in the `supply-chain` job; any advisory/disallowed license hard-fails with the offending item reported (SC-004) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T012]
- [X] T014 [OBJ3] {OR-005} [COMPLETES OR-005] Ensure the `supply-chain` job runs on `pull_request`/`push` AND the daily `schedule` cron so newly published advisories are caught with no new commit (SC-004) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T013]
- [X] T015 [OBJ3] {OR-012} [COMPLETES OR-012] Pin every third-party action (`actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `taiki-e/install-action`) to a full commit SHA — no tag/major-only pins (AD-005, SC-009) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T013]

---

## Phase 6: OBJ4 - Build caching and reproducibility (Priority: P2)

**rust-cache keyed per OS/target across all build/test jobs; `--locked` everywhere against the committed `Cargo.lock`.**

- [X] T016 [OBJ4] {OR-006} [COMPLETES OR-006] Add `Swatinem/rust-cache` (SHA-pinned) to `check`, `test`, and `wasm` jobs, keyed on toolchain + lockfile + OS/target (AD-002, SC-005) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T011]
- [X] T017 [OBJ4] {OR-007} [COMPLETES OR-007] Confirm `--locked` is applied to every `cargo build`/`cargo test` invocation against the committed `Cargo.lock` so reruns of a commit resolve an identical dependency set (SC-006) in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T016]

---

## Phase 7: Polish & Cross-Cutting Concerns

**Operator runbooks (RR-001..RR-003) and end-to-end pipeline validation: actionlint on the YAML plus a real injected-failure run proving red-on-failure / green-on-clean.**

- [X] T018 [P] {RR-001} Write the branch-protection runbook (which jobs — `check`, `test (ubuntu/windows/macos)`, `wasm`, `supply-chain` — to mark as required status checks; admin-only step) in s:\claudecode\RONedit\docs\runbooks\branch-protection.md
- [X] T019 [P] {RR-002} Write the advisory-response runbook (escalation order: triage → patch/upgrade → last-resort dated, PR-reviewed waiver in `deny.toml`/cargo-audit ignore-list; no silent/CI-level override) in s:\claudecode\RONedit\docs\runbooks\advisory-response.md
- [X] T020 [P] {RR-003} Write the CI-local-repro runbook (exact `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --locked`, `cargo build -p ron-core --target wasm32-unknown-unknown` commands; optional `nektos/act`) in s:\claudecode\RONedit\docs\runbooks\ci-local-repro.md
- [X] T021 Validate `ci.yml` with actionlint (syntax/expressions/job graph) and fix any findings in s:\claudecode\RONedit\.github\workflows\ci.yml [after:T017]
- [X] T022 Run the pipeline end-to-end: green on a clean commit, then inject fmt/clippy/test/wasm/advisory failures one at a time and confirm each reds the corresponding gate job (SC-001, SC-002, SC-003, SC-004) [after:T021] — validated locally (actionlint v1.7.12 clean + all six gate commands green on clean tree + injected-failure red spot-check on `cargo fmt --check`); GitHub-hosted run + branch protection (RR-001) pending repo push to GitHub.

---

## Dependencies

Setup → Foundational → OBJ1 (P1) → OBJ2 (P1) → OBJ3 (P1) → OBJ4 (P2) → Polish

- **Phase 1 (Setup)**: T001 (Cargo.lock) is a prerequisite for all `--locked` usage and gates T003. T002 (dependabot.yml) is an independent file — `[P]`.
- **Phase 2 (Foundational)**: T003→T004→T005 build the single `ci.yml` shell and the shared step pattern every job reuses. All in-file job tasks depend on T005.
- **OBJ1–OBJ4 job tasks** all edit the same `ci.yml`; they are sequenced (not `[P]`) and each carries `after:T###` back to its producer (T005, or the job task it extends). OBJ2/OBJ3 job-add tasks (T011, T012) branch off T005 in parallel-in-principle but are serialized because they touch one file.
- **Cross-phase confirmation edges**: T009 (OR-008 toolchain consistency) and T010 (OR-010 named-job set) are the OBJ1 closure tasks; both carry `after:T012` because they verify the complete named-job set, which only exists once the `wasm` (T011) and `supply-chain` (T012) jobs are defined.
- **OBJ4 caching (T016)** depends on the `wasm` job existing (T011) so it can wire cache into all three build/test jobs; T017 depends on T016.
- **Polish runbooks (T018–T020)** are independent files — `[P]` together; they have no code dependency. T021 (actionlint) depends on the finished `ci.yml` (T017). T022 (injected-failure run) depends on a lint-clean workflow (T021).
- Tasks marked `[P]` operate on distinct files with no ordering dependency. No `[P]` batch contains a task and its `after:` producer.

## Requirement Coverage

| Req | Tasks | Completion Point |
|-----|-------|------------------|
| OR-001 | T006 | T006 |
| OR-002 | T006 | T006 |
| OR-003 | T007 | T007 |
| OR-004 | T011 | T011 [COMPLETES] |
| OR-005 | T012, T013, T014 | T014 [COMPLETES] |
| OR-006 | T016 | T016 [COMPLETES] |
| OR-007 | T001, T017 | T017 [COMPLETES] |
| OR-008 | T005, T009 | T009 [COMPLETES] |
| OR-009 | T007 | T007 |
| OR-010 | T010 | T010 [COMPLETES] |
| OR-011 | T003, T008 | T008 |
| OR-012 | T002, T004, T015 | T015 [COMPLETES] |
| RR-001 | T018 | T018 |
| RR-002 | T019 | T019 |
| RR-003 | T020 | T020 |
