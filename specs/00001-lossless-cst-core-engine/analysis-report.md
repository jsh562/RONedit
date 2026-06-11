# Analysis Report: Lossless CST Core Engine (E001)

> Date: 2026-06-11 | Artifacts: spec.md, plan.md, tasks.md (+ data-model.md, research.md, checklists/)

## Summary

- **Overall**: Implementation-ready. 0 CRITICAL, 0 HIGH findings.
- **Compliance** (Policy Auditor on plan.md): **PASS** — 0 project-instructions violations across Principles I–VI, Technology Stack, Testing & Quality Policy, Source Code Layout, Development Workflow, Governance, ADR-0001/0002.
- **Spec quality** (Spec Validator, read-only): 0 HIGH, 6 MEDIUM, 13 LOW — the four reconciled contradiction areas (UTF-8 vs round-trip, idempotence vs round-trip, 0.x stability wording) confirmed consistent; remaining items are testability/coverage refinements.
- **Coverage**: 17/17 TRs mapped to tasks (100%); cross-phase dependency edges all consistent; no convention violations.

## Findings Table

| ID | Category | Severity | Location(s) | Summary | Recommendation |
|----|----------|----------|-------------|---------|----------------|
| F-01 | Underspecification | MEDIUM | spec TR-010 | Typed-navigation accessors not enumerated; "RON constructs" is open-ended | Enumerate the construct set in TR-010 |
| F-02 | Coverage | MEDIUM | spec TR-015 | Corpus→property feedback requirement has no success criterion | Acknowledge it is verified by task T038 (corpus harness) |
| F-03 | Coverage | MEDIUM | spec TR-016 | Benchmark harness has no SC (non-gating by design) → unverifiable as written | Acknowledge as informational/non-gating; verified by T036 building/running |
| F-04 | Ambiguity | LOW→MEDIUM | spec SC-003 | Fuzz gate is "≥1M iterations OR a fixed time budget, e.g. 60s" — non-deterministic, and a wall-clock gate conflicts with the correctness-only (non-temporal) envelope | Make the gate a single deterministic iteration count |
| F-05 | Ambiguity | LOW | spec Glossary | "recovery point" (TR-013/SC-003 diagnostic counts) and "0.x shape-stable" (TR-009) undefined | Add Glossary entries |
| F-06 | Completion point | MEDIUM | tasks T016/T038 | TR-003 maps to 3 tasks (T016,T017,T038); `[COMPLETES TR-003]` sits on T016, not the last carrier T038 | Move the COMPLETES TR-003 marker to T038 |
| F-07 | File-path drift | MEDIUM | plan Project Structure | Tasks reference `src/ron-core/benches/parse_print.rs` (T036) and `tests/consumer_api.rs` (T031), absent from the plan's structure tree | Add both paths to plan Project Structure |
| F-08 | Duplication (by design) | MEDIUM/LOW | spec TR-001/005/SC-003; TR-017/003; TR-006/013 | Requirement↔criterion restatement of the token-coverage / round-trip / diagnostic-contract invariants | None — accepted requirement→criterion tracing (skipped) |
| F-09 | Resolved | LOW | spec TR-004 | "full grammar surface = whatever pinned ron accepts" is plan-dependent | Resolved — plan AD-002 pins `ron = "=0.12.1"` |
| F-10 | Resolved | LOW | spec Clarifications/STF-001 | UTF-8/round-trip, idempotence/round-trip, 0.x stability potential conflicts | Resolved — reconciled and consistent in current text |

## Quality Summaries

- **Spec Quality**: No HIGH/CRITICAL. Internally consistent on all four flagged contradiction areas. Primary actionable gaps: TR-010 testability (F-01), SC-003 determinism (F-04), explicit acknowledgment of TR-015/TR-016 verification routes (F-02/F-03).
- **Compliance**: PASS. rowan 0.16.1 is the only `ron-core` runtime dep (pure-Rust/WASM-clean); `ron = "=0.12.1"` is grammar-authority/interop-only, never a core dep — WASM-cleanliness preserved. All four mandatory gates (fmt, clippy -D warnings, test matrix, wasm32 build) declared.

## Coverage Summary (TR → Tasks)

| Requirement | Has Task? | Task IDs | Notes |
|-------------|-----------|----------|-------|
| TR-001 | ✅ | T012, T028 | |
| TR-002 | ✅ | T011, T013, T015 | completes T015 |
| TR-003 | ✅ | T016, T017, T038 | completion marker relocated to T038 (F-06) |
| TR-004 | ✅ | T009, T013, T014, T015, T019 | completes T019 |
| TR-005 | ✅ | T021, T022, T026, T028 | completes T028 |
| TR-006 | ✅ | T020, T022, T025 | completes T025 |
| TR-007 | ✅ | T006, T030 | completes T030 |
| TR-008 | ✅ | T001, T002, T004, T005, T006, T007, T008 | completes T008 |
| TR-009 | ✅ | T010, T029, T031 | completes T031 |
| TR-010 | ✅ | T032 | enumerated (F-01) |
| TR-011 | ✅ | T033, T034, T035 | completes T035 |
| TR-012 | ✅ | T024 | completes T024 |
| TR-013 | ✅ | T020, T022, T025 | completes T025 |
| TR-014 | ✅ | T023, T027 | completes T023 |
| TR-015 | ✅ | T038 | completes T038 |
| TR-016 | ✅ | T036 | informational, non-gating |
| TR-017 | ✅ | T016, T018 | completes T018 |

## Unmapped Tasks

None requiring a requirement tag. Untagged tasks (T003, T037, T039) are in Setup/Polish phases (allowed).

## Metrics

- Total requirements: 17 (TR-001..TR-017); success criteria: 9 (SC-001..SC-009, +SC-010 not added — see remediation)
- Total tasks: 39
- Requirement→task coverage: 100% (17/17)
- CRITICAL issues: 0 · HIGH: 0 · MEDIUM: 6 · LOW: ~13 (mostly by-design)

## Remediation (applied per "apply all")

See the remediation summary returned to the user. Applied: F-01, F-04, F-05, F-07, F-06; acknowledged F-02/F-03 in the coverage section. Skipped: F-08 (by-design tracing), F-09/F-10 (already resolved).
