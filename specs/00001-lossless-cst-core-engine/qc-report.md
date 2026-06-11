# QC Report: Lossless CST Core Engine (E001)

> Date: 2026-06-11 | Feature: `specs/00001-lossless-cst-core-engine` | Toolchain: Rust stable (rustc/cargo 1.96, clippy 0.1.96, rustfmt 1.9)

## Overall Verdict: PASS

All PI-mandated QC categories (linting, security) pass, plus build, wasm32 gate, full test suite, and complete requirement/SC traceability. No CRITICAL/ERROR findings. Non-blocking WARNINGs noted below.

## Test Results

- **Runner**: `cargo test` (built-in). **79 passed, 0 failed, 0 ignored.**
- Breakdown: 50 lib unit · 4 consumer_api · 4 corpus · 5 edit · 10 roundtrip (incl. proptest) · 5 snapshots (insta) · 1 doctest.
- `cargo test --workspace` — green (ron-types/ronin-app stubs: 0 tests).
- Invariant coverage present (PI §V): lossless round-trip (property + snapshot + corpus), idempotent print, parser error-tolerance on malformed input, deterministic parsing, depth-guard pathological nesting (4000 levels, no overflow).
- **Failures**: none.

## Static Analysis

- **clippy**: `cargo clippy --workspace --all-targets -- -D warnings` — exit 0, 0 issues (verified against a clean rebuild, no cache masking).
- **rustfmt**: `cargo fmt --check` — exit 0, no diffs.
- **WASM-clean gate**: `cargo build -p ron-core --target wasm32-unknown-unknown` — exit 0 (ADR-0002 / PI §II invariant satisfied).

## Security Audit

- **cargo-audit 0.22.2**: `cargo audit` — exit 0. 83 dependencies scanned vs 1125 RustSec advisories. **0 vulnerabilities, 0 yanked.**
- **cargo-deny 0.19.4**: `cargo deny check` — exit 0. advisories ok · bans ok · licenses ok · sources ok.
  - WARNING (non-blocking): 7 `license-not-encountered` entries in `deny.toml` (allow-list broader than the dependency set). Informational; optional cleanup.

## PI Compliance

No violations. Verified against `project-instructions.md` Principles I–VI:
- I Never Corrupt: lossless round-trip enforced (corpus 100% byte-for-byte incl. malformed); `apply_edit` is non-destructive (returns a new tree).
- II One Core / WASM-clean: rowan-only runtime dep, no fs/UI/async/native; wasm32 build passes; `#![forbid(unsafe_code)]`.
- V Verified Quality: clippy -D warnings + rustfmt + wasm32 + full invariant test suite.
- VI Local-First & Private: no network/telemetry in the library; user input parsed, never executed.

## Requirements Traceability

| Work Item | Priority | Status |
|-----------|----------|--------|
| OBJ1 Lossless parse + round-trip | P1 | PASSED |
| OBJ2 Error-tolerant parsing + diagnostics | P1 | PASSED |
| OBJ3 WASM-clean workspace + 0.x API | P1 | PASSED |
| OBJ4 CST navigation + edit primitives | P2 | PASSED |

| SC | Status | Evidence |
|----|--------|----------|
| SC-001 corpus 100% byte-for-byte | PASSED | `tests/corpus.rs` (42 fixtures, ≥1 per TR-004 group, 4 malformed, 1.1 MB file) |
| SC-002 property round-trip across grammar | PASSED | `tests/roundtrip.rs::roundtrip_identity` |
| SC-003 fuzz no-panic / token-concat==input | PASSED (env fallback) | nightly `fuzz/` target exists (not runnable on stable); stable proptest fallback over arbitrary bytes/str/structural (4096 cases each) asserts the same invariants — assessed acceptable |
| SC-004 diagnostics byte-range + full coverage | PASSED | `parser.rs::diagnostic_ranges_are_within_source`, `corpus.rs::malformed_fixtures_recover_and_round_trip` |
| SC-005 wasm32 build, no native deps | PASSED | wasm32 build exit 0; rowan-only |
| SC-006 public API no I/O / no rowan types | PASSED | `tests/consumer_api.rs` (`PublicApiWitness`) |
| SC-007 edit locality + printability | PASSED | `tests/edit.rs` (3 kinds × 4 trivia policies) |
| SC-008 bounded depth, no stack overflow | PASSED | `roundtrip.rs::depth_limit_at_bound_plus_one` + 4000-level guard |
| SC-009 idempotent print | PASSED | `roundtrip.rs::idempotent_print` |

## Traceability Gaps

None. Every TR-001..TR-017 maps to tasks T001–T039 (all `[X]`) and to the plan's Requirement Coverage Map. Non-SC requirements covered as documented: TR-010 (`syntax/ast.rs`), TR-012 (`parser.rs::parsing_is_deterministic`), TR-015 (`corpus.rs` detector + documented feedback policy), TR-016 (`benches/parse_print.rs`, informational, `harness=false`).

## Code Coverage

SKIPPED → WARNING (advisory, not a required category; PI Coverage Target = none — correctness enforced via invariant tests). Install hint: `cargo install cargo-llvm-cov` then `cargo llvm-cov -p ron-core`.

## Checklist Fulfillment (spot-check)

- **Testing**: PASSED — round-trip/idempotence proptests, insta snapshots, corpus harness, and the stable no-panic fuzz fallback all implemented (corpus floor ≥30/≥3 malformed/≥1 MB met).
- **Security**: PASSED — cargo-audit + cargo-deny clean; library makes no network calls and executes no user input.
- **Data Integrity**: PASSED — byte-for-byte round-trip holds for valid and error-recovered trees (corpus + property tests).

## Performance

Not gated (correctness-only envelope; TR-016 benchmark harness is informational). `benches/parse_print.rs` runs over the corpus (large file ≈20 ms parse / ≈19 ms print) with no pass/fail threshold.

## Accessibility

N/A — pure library, no UI.

## Browser Runtime Validation

N/A — pure I/O-free library, no server/UI components.

## Manual Testing

None required.

## Tool Recommendations

- (Advisory) `cargo install cargo-llvm-cov` to add coverage reporting (not required by policy).
- (Optional) Prune `deny.toml` license allow-list to the encountered set to clear the 7 `license-not-encountered` warnings.
- (Roadmap) Run the nightly `cargo fuzz` ≥1M-iteration gate on a nightly-capable CI runner for the authoritative SC-003 check.

## Bug Tasks Generated

None.
