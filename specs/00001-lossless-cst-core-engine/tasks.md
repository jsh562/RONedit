# Tasks: Lossless CST Core Engine

**Input**: Design documents from `specs/00001-lossless-cst-core-engine/`
**Prerequisites**: plan.md (required), spec.md (required), research.md, data-model.md, checklists/{data-integrity,testing,performance}.md

**Tests**: Included — the spec mandates invariant tests (property round-trip/idempotence, insta snapshots, real corpus, cargo-fuzz no-panic) per project-instructions §V and plan §Testing Strategy. Test tasks are written to FAIL before their implementation lands.

**Organization**: Technical spec — grouped by objective (`OBJ#`) and ordered by the plan's implementation sequence (scaffold → SyntaxKind/CST → lexer → parser → printer → diagnostics → accessors → edit → bench).

## Project Mode

`Greenfield`

- Initial Cargo workspace and crate scaffolding under `/src` is part of this feature (TR-008, NEW-CONFIG).

## Epic / Capability Map *(OPTIONAL)*

- `[OBJ1]` → Lossless parse + byte-for-byte round-trip (P1): lexer, parser value grammar, printer.
- `[OBJ2]` → Error-tolerant parsing + structured diagnostics (P1): recovery, diagnostics model, depth guard.
- `[OBJ3]` → WASM-clean workspace + 0.x shape-stable public API (P1): rowan-free facade, consumer crate, wasm32 gate.
- `[OBJ4]` → CST navigation + node/token edit primitives (P2): typed accessors, insert/replace/remove with trivia policy.

---

## Phase 1: Setup (Repository / Workspace Delta)

- [X] T001 Create Cargo workspace root manifest with members under `src/` in Cargo.toml {TR-008} → exports: [workspace].members=[src/ron-core,src/ron-types,src/ronin-app]
- [X] T002 [P] Pin stable toolchain >= 1.77 (rowan MSRV) in rust-toolchain.toml {TR-008}
- [X] T003 [P] Add cargo-deny license/advisory/ban policy in deny.toml (security gate per project-instructions §Testing Policy)
- [X] T004 [P] Create ron-types stub crate (lib.rs placeholder; full impl deferred to E004) in src/ron-types/Cargo.toml + src/ron-types/src/lib.rs {TR-008}
- [X] T005 [P] Create ronin-app stub crate (main.rs placeholder; full impl deferred to E003) in src/ronin-app/Cargo.toml + src/ronin-app/src/main.rs {TR-008}
- [X] T006 Create ron-core crate manifest with rowan 0.16.1 only; no fs/UI/async/native deps (WASM-clean) in src/ron-core/Cargo.toml {TR-007,TR-008} after:T001 → exports: [dependencies].rowan="0.16.1"
- [X] T007 [P] Add dev-dependencies proptest 1.11.0 + insta 1.47.2 + arbitrary 1.4.2 (derive) to src/ron-core/Cargo.toml {TR-008} after:T006
- [X] T008 Verify workspace builds (cargo build) for all crates incl. stubs in Cargo.toml [COMPLETES TR-008] after:T006

---

## Phase 2: Foundational (Cross-Work-Item Blockers)

**SyntaxKind + the rowan-free CST newtype layer block the lexer, parser, printer, accessors, and edit ops. AD-001 trivia rule is locked here per HINT-001 before any parsing work.**

- [X] T009 Define closed SyntaxKind enum (Struct, Tuple, List/Seq, Map, MapEntry, EnumVariant, String, RawString, Char, Number, Bool, Unit, Ident/FieldName, ExtensionAttr, Comment, Whitespace, Error) per pinned ron 0.12.1 grammar in src/ron-core/src/syntax/kind.rs {TR-004} → exports: SyntaxKind(closed enum incl. Error)
- [X] T010 Wire rowan GreenNode/Language behind rowan-free SyntaxNode/SyntaxToken/SyntaxElement newtypes (no rowan types in public shape, HINT-005) in src/ron-core/src/syntax/mod.rs {TR-009} ← T009:SyntaxKind → exports: SyntaxNode, SyntaxToken, SyntaxElement (opaque)
- [X] T011 Lock AD-001 trivia attachment rule (leading trivia binds following token; trailing-at-EOF binds nearest preceding structure) as documented module invariant in src/ron-core/src/syntax/mod.rs {TR-002} after:T010 → exports: trivia-attachment doc/contract

---

## Phase 3: OBJ1 - Lossless parse and round-trip (Priority: P1) 🎯 MVP

**Goal**: Parse RON into a lossless CST and re-print an unmodified tree byte-for-byte (SC-001/002/009).

- [X] T012 [OBJ1] {TR-001} Implement UTF-8 boundary check that rejects non-UTF-8 input with a clean Err (never panics) and preserves a leading BOM (AD-008/HINT-002, INV-4) in src/ron-core/src/lexer.rs ← T009:SyntaxKind → exports: validate_utf8(&[u8]) -> Result<&str,Error>
- [X] T013 [OBJ1] {TR-002,TR-004} Implement lexer tokenizing the full ron 0.12.1 surface verbatim — strings, raw strings, chars, numbers, bools, idents, punctuation, comments, whitespace — every byte in exactly one token (INV-1) in src/ron-core/src/lexer.rs after:T012 → exports: tokenize(&str) -> Vec<Token{kind,text}>
- [X] T014 [OBJ1] {TR-004} Implement recursive-descent value parser for composite + scalar constructs (struct, tuple, list/seq, map incl. non-string keys, enum/variant, unit, Option/implicit_some, extension attrs) building the CST via GreenNodeBuilder in src/ron-core/src/parser.rs ← T010:SyntaxNode ← T013:tokenize → exports: parse(&str) -> CstDocument
- [X] T015 [OBJ1] {TR-002,TR-004} [COMPLETES TR-002] Attach trivia per AD-001 (incl. BOM/CRLF/LF, missing trailing newline, empty/whitespace-only/comments-only files) so all grammar constructs and trivia are distinct, preserved nodes in src/ron-core/src/parser.rs after:T014 ← T011:trivia-attachment
- [X] T016 [OBJ1] {TR-003,TR-017} Implement printer that walks the CST and concatenates token text to reproduce source bytes (round-trip identity + idempotent print, INV-2) in src/ron-core/src/printer.rs ← T010:SyntaxNode → exports: print(&CstDocument) -> String
- [X] T017 [P] [OBJ1] {TR-003,TR-004} Write proptest round-trip identity strategies covering every TR-004 construct (parse->print == input) in src/ron-core/tests/roundtrip.rs after:T016 ← T014:parse ← T016:print
- [X] T018 [P] [OBJ1] {TR-017} [COMPLETES TR-017] Write proptest idempotent-print property — print(parse(print(parse(x)))) == print(parse(x)) (SC-009, INV-2) in src/ron-core/tests/roundtrip.rs after:T016 ← T014:parse ← T016:print
- [X] T019 [P] [OBJ1] {TR-004} [COMPLETES TR-004] Write insta snapshot tests per literal kind (raw strings w/ embedded quotes/hashes, numeric/escape forms, chars, non-string map keys, extension attrs) in src/ron-core/tests/snapshots.rs after:T016 ← T014:parse

---

## Phase 4: OBJ2 - Error-tolerant parsing and diagnostics (Priority: P1) 🎯 MVP

**Goal**: Always produce a complete tree covering all input plus structured diagnostics, never panicking, with a bounded depth guard (SC-003/004/008).

- [X] T020 [OBJ2] {TR-006,TR-013} Define Diagnostic model — ByteRange, message, Severity{Error,Warning} enum, and a stable RON-Pxxxx DiagnosticCode registry (AD-003, part of public API) in src/ron-core/src/diagnostics.rs → exports: Diagnostic(range,message,severity,code), Severity, DiagnosticCode(RON-Pxxxx)
- [X] T021 [OBJ2] {TR-005} Add error-recovery to the parser — wrap unexpected tokens in Error nodes, represent absent constructs as missing/empty nodes, with recovery sets on `,` `)` `]` `}` and field idents so the tree covers all input (INV-3) in src/ron-core/src/parser.rs after:T014 ← T020:Diagnostic
- [X] T022 [OBJ2] {TR-005,TR-006,TR-013} Emit one diagnostic per recovery point with a precise byte range; enforce the must-consume-a-token invariant for termination (HINT-004) in src/ron-core/src/parser.rs after:T021 ← T020:Diagnostic
- [X] T023 [OBJ2] {TR-014} [COMPLETES TR-014] Add configurable nesting-depth guard (default 128, AD-005) that stops descent, emits an over-limit RON-Pxxxx diagnostic, and still tokenizes remaining bytes into Error nodes (INV-5; no stack overflow) in src/ron-core/src/parser.rs after:T022 ← T020:Diagnostic
- [X] T024 [OBJ2] {TR-012} [COMPLETES TR-012] Assert deterministic output — identical input yields an identical tree AND identical diagnostics set incl. recovery/over-limit (INV-6) in src/ron-core/src/parser.rs after:T023
- [X] T025 [P] [OBJ2] {TR-006,TR-013} [COMPLETES TR-006,TR-013] Unit-test the diagnostic contract — Severity enum values, RON-Pxxxx codes, one-per-recovery-point, byte-range within [0,source_len) (SC-004) in src/ron-core/src/diagnostics.rs after:T024 ← T020:Diagnostic
- [X] T026 [P] [OBJ2] {TR-005} Unit/property-test ERROR-node coverage on malformed samples (unclosed delimiters, stray tokens, partial top-level value) — token concat == input (SC-004, INV-3) in src/ron-core/tests/roundtrip.rs after:T024 ← T014:parse
- [X] T027 [P] [OBJ2] {TR-014} Unit-test depth-limit behavior at bound+1 — no overflow, over-limit diagnostic emitted, tree still round-trips (SC-008) in src/ron-core/tests/roundtrip.rs after:T024 ← T014:parse
- [X] T028 [OBJ2] {TR-001,TR-005} [COMPLETES TR-005] Create cargo-fuzz target asserting no-panic + token-concat==input on arbitrary input, clean non-UTF-8 rejection; seed from corpus, >=1M iters, crash inputs added as regression seeds (SC-003) in src/ron-core/fuzz/Cargo.toml + src/ron-core/fuzz/fuzz_targets/roundtrip.rs after:T024 ← T014:parse ← T016:print

---

## Phase 5: OBJ3 - WASM-clean workspace and core API (Priority: P1) 🎯 MVP

**Goal**: Expose a 0.x shape-stable rowan-free public API and prove WASM-cleanliness via the wasm32 build gate (SC-005/006).

- [X] T029 [OBJ3] {TR-009} Assemble the 0.x shape-stable public API facade (parse, navigate CST, read diagnostics, print) exposing no rowan and no I/O types (INV-7, HINT-005) in src/ron-core/src/lib.rs ← T014:parse ← T016:print ← T020:Diagnostic ← T010:SyntaxNode → exports: ron_core::{parse,print,CstDocument,SyntaxNode,Diagnostic}
- [X] T030 [OBJ3] {TR-007} [COMPLETES TR-007] Verify `cargo build -p ron-core --target wasm32-unknown-unknown` succeeds with no fs/UI/async/native deps (SC-005, INV-9, HINT-003) in src/ron-core/Cargo.toml after:T029
- [X] T031 [P] [OBJ3] {TR-009} [COMPLETES TR-009] Add a consumer crate/test that parses, navigates, reads diagnostics, and prints using only the public API with no leaked rowan/I/O types (SC-006) in src/ron-core/tests/consumer_api.rs after:T029 ← T029:parse

---

## Phase 6: OBJ4 - CST navigation and edit primitives (Priority: P2)

**Goal**: Typed CST navigation plus lossless node/token-span edit primitives with caller-chosen trivia policy (SC-007).

- [X] T032 [OBJ4] {TR-010} [COMPLETES TR-010] Implement typed accessors for RON constructs over the CST (navigate structs/tuples/lists/maps/entries/variants/values) in src/ron-core/src/syntax/ast.rs ← T010:SyntaxNode ← T009:SyntaxKind → exports: typed accessors (e.g. Struct::fields(), Map::entries())
- [X] T033 [OBJ4] {TR-011} Define EditTarget (node | token-span), EditKind (Insert/Replace/Remove), and TriviaPolicy (keep/discard leading/trailing, AD-004) in src/ron-core/src/edit.rs ← T010:SyntaxNode → exports: EditOperation(target,op,payload,trivia_policy), TriviaPolicy
- [X] T034 [OBJ4] {TR-011} Implement non-destructive insert/replace/remove producing a new tree; unaffected regions print byte-identically, no un-covered/overlapping ranges, tree stays printable (INV-8) in src/ron-core/src/edit.rs after:T033 ← T033:EditOperation ← T016:print → exports: apply_edit(&CstDocument,EditOperation) -> CstDocument
- [X] T035 [P] [OBJ4] {TR-011} [COMPLETES TR-011] Property/unit-test edit locality — after each op under each trivia policy, unaffected regions byte-identical + full tree printable (SC-007) in src/ron-core/tests/roundtrip.rs after:T034 ← T034:apply_edit ← T016:print

---

## Phase 7: Polish & Cross-Cutting Concerns

- [X] T036 [P] {TR-016} Add benchmark harness measuring parse/print over the corpus — informational only, NO pass/fail gate (not a QC/release criterion) in src/ron-core/benches/parse_print.rs after:T029 ← T029:parse
- [X] T037 Assemble corpus fixtures: >=30 real serde + Bevy .scn.ron files, >=1 per TR-004 construct group, >=3 malformed, >=1 file >=1 MB (SC-001) in src/ron-core/tests/corpus/ after:T016
- [X] T038 {TR-003,TR-015} [COMPLETES TR-003,TR-015] Implement corpus round-trip harness asserting 100% byte-for-byte (incl. malformed via error-recovered trees) and feed any newly discovered constructs back into proptest strategies (SC-001) in src/ron-core/tests/corpus.rs after:T037 ← T014:parse ← T016:print
- [X] T039 [P] Ensure rustfmt clean + clippy `-D warnings` across the workspace (project-instructions §V) in Cargo.toml after:T038

---

## Dependencies

Setup (Phase 1) → Foundational (Phase 2) → OBJ1 (Phase 3) → OBJ2 (Phase 4) → OBJ3 (Phase 5) → OBJ4 (Phase 6) → Polish (Phase 7)

- Phase 1 Setup has no dependencies; T006/T007/T008 follow T001 (workspace manifest first).
- Phase 2 Foundational depends on Setup; T009 (SyntaxKind) → T010 (CST newtypes) → T011 (trivia rule) are sequential.
- OBJ1 (Phase 3) depends on Foundational. OBJ2 (Phase 4) extends the OBJ1 parser (`after:T014`) and depends on the Diagnostic model (T020). OBJ3 (Phase 5) depends on parse/print/diagnostics existing (`after`/`←` on T014/T016/T020). OBJ4 (Phase 6) depends on the CST newtypes and printer.
- Phase 7 Polish depends on the public API (T029), the printer/parser, and the corpus (T037).
- Tasks marked `[P]` can run in parallel within their phase (distinct files / test modules, no intra-batch dependency).
- A task with `after:T###` or `← T###:Symbol` is never `[P]`-batched with its referenced task.
- All three P1 objective phases (OBJ1, OBJ2, OBJ3) carry 🎯 MVP — together they deliver the lossless, resilient, WASM-clean round-trip engine.
