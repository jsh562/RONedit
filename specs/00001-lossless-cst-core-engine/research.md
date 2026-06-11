# Research: Lossless CST core engine (E001)
> E001 | 2026-06-11 | Inform ron-core architecture, dependency pins, and testing

## Lossless CST (rowan)
- **Decision**: Build the CST on rowan red-green trees; every source byte lives in exactly one token (trivia and punctuation included).
- **Rationale**: rust-analyzer-proven lossless model; pure-Rust and wasm32-clean.
- **Rejected**: serde `ron` (drops comments/formatting/struct names); tree-sitter (C runtime, weak rewrite control).
- **Pitfalls**: normalizing whitespace/numbers or dropping struct names silently breaks round-trip.
- **Sources**: https://github.com/rust-analyzer/rowan, https://github.com/ron-rs/ron/issues/216

## Error-tolerant parsing
- **Decision**: Hand-written resilient recursive-descent LL parser; wrap stray tokens in ERROR nodes, missing constructs as empty nodes; recovery sets on `,` `)` `]` `}` and field idents.
- **Rationale**: always yields a complete, round-trippable tree plus diagnostics; never panics.
- **Rejected**: bail-on-first-error (unusable mid-edit); parser generators (less recovery control).
- **Pitfalls**: a loop iteration consuming 0 tokens → non-termination; over-recovery swallows valid constructs.
- **Sources**: https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html

## Testing strategy
- **Decision**: proptest (round-trip identity + idempotent print), insta snapshots (CST/print), real serde+Bevy corpus, cargo-fuzz (no-panic + round-trip).
- **Rationale**: covers the three load-bearing invariants with both grammar and arbitrary-input reach.
- **Rejected**: example-only tests (miss edge cases); random-byte-only fuzz (shallow grammar reach).
- **Pitfalls**: snapshotting volatile data; fuzzing without a round-trip oracle.
- **Sources**: https://altsysrq.github.io/proptest-book, https://rust-fuzz.github.io/book/cargo-fuzz.html

## WASM-clean core
- **Decision**: ron-core carries no fs/net/thread/async/native deps; CI builds `wasm32-unknown-unknown` as the proof.
- **Rationale**: only a real wasm build proves cleanliness; enables LSP/VSCode reuse (ADR-0002).
- **Rejected**: source-review-only verification (unreliable).
- **Pitfalls**: a transitive native/`cc` dep; reliance on threads or `std::time::Instant`.
- **Sources**: https://rustwasm.github.io/docs/book/reference/which-crates-work-with-wasm.html

## Dependency version pins (2026-06)
- **Decision**: `ron = "=0.12.1"` (grammar authority; interop-only, E010), `rowan 0.16.1`; dev/test proptest 1.11.0, insta 1.47.2, cargo-fuzz 0.13.2, arbitrary 1.4.2; QC cargo-audit 0.22.2, cargo-deny 0.19.8, cargo-llvm-cov 0.8.7.
- **Rationale**: current stable; rowan's deps are all pure-Rust / wasm-clean (countme, hashbrown, rustc-hash, text-size).
- **Rejected**: cstree 0.14.0 alternative — requires edition 2024 / Rust 1.85, above the 2021/stable baseline (rowan MSRV 1.77).
- **Pitfalls**: ron 0.12 removed base64 byte strings and ignores `#![type]`/`#![schema]` — out of grammar scope.
- **Sources**: https://crates.io/crates/ron, https://crates.io/crates/rowan

## Summary
| Topic | Decision | Rationale |
|-------|----------|-----------|
| CST | rowan lossless red-green tree | proven, pure-Rust, wasm-clean |
| Parser | resilient LL + ERROR nodes | always round-trippable + diagnostics |
| Testing | proptest + insta + corpus + fuzz | covers round-trip / no-panic / idempotence |
| WASM | wasm32 CI build gate | only reliable proof of cleanliness |
| Pins | ron 0.12.1 / rowan 0.16.1 | current stable, MSRV-compatible |

## Sources Index
| URL | Topic | Fetched |
|-----|-------|---------|
| https://github.com/rust-analyzer/rowan | CST | 2026-06-11 |
| https://matklad.github.io/2023/05/21/resilient-ll-parsing-tutorial.html | Parser | 2026-06-11 |
| https://altsysrq.github.io/proptest-book | Testing | 2026-06-11 |
| https://rust-fuzz.github.io/book/cargo-fuzz.html | Fuzzing | 2026-06-11 |
| https://crates.io/crates/ron | Pins | 2026-06-11 |
| https://crates.io/crates/rowan | Pins | 2026-06-11 |
