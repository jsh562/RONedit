---
feature_branch: "00001-lossless-cst-core-engine"
created: "2026-06-11"
input: "e001"
spec_type: "technical"
spec_maturity: "clarified"
epic_id: "E001"
epic_sources: "{PRD:CAP-001}{SAD:ADR-0001}{SAD:ADR-0002}"
---

# Feature Specification: Lossless CST Core Engine

**Feature Branch**: `00001-lossless-cst-core-engine`
**Created**: 2026-06-11
**Status**: Draft
**Spec Type**: technical
**Spec Maturity**: clarified
**Epic ID**: E001
**Epic Sources**: {PRD:CAP-001}{SAD:ADR-0001}{SAD:ADR-0002}
**Product Document**: specs/prd.md

## Problem Statement *(mandatory)*

Every RONin capability — validation, smart authoring, structural/table editing, persistence, and interop — needs one trustworthy in-memory representation of a RON file. The serde-based `ron` crate cannot serve this role: round-tripping through it discards comments, formatting, ordering, and struct names, which would violate RONin's core promise never to corrupt a file. Without a lossless, error-tolerant core engine that can parse arbitrary RON and reproduce it byte-for-byte, no downstream editor feature can be built safely. This engine must also stay portable (WASM-clean) so the desktop editor and future LSP/VSCode frontends reuse it unchanged.

## Clarifications

### Session 2026-06-11

- Q: Grammar-surface authority for the round-trip gate (TR-004)? -> A: The pinned `ron` crate version defines the in-scope grammar and extensions; that version is recorded in the plan.
- Q: Is byte-for-byte round-trip absolute or corpus-scoped? -> A: Absolute over every accepted (UTF-8) input — valid or malformed; corpus + fuzz + property tests verify it.
- Q: Diagnostic contract (severity/codes/emission)? -> A: Fixed severity enum (Error/Warning) + a stable namespaced code registry (e.g. `RON-Pxxxx`), one diagnostic per recovery point; part of the stable public API.
- Q: Edit-primitive unit and trivia handling (TR-011)? -> A: Support both syntax-node and token-span granularity with a caller-chosen trivia policy (keep/discard leading/trailing).
- Q: API stability level for ron-core (TR-009)? -> A: 0.x shape-stable — capability areas committed; breaking changes allowed only within 0.x until downstream epics validate the shape.
- Q: Performance/size envelope for E001? -> A: Correctness-only — termination on any input + a bounded nesting depth (no stack overflow); a benchmark harness with no pass/fail gate.
- Q: Commit to rowan, or stay library-agnostic? -> A: Commit rowan as the implementation (cstree acceptable); keep the public API free of rowan types so it stays swappable.
- Q: Input contract behind "parse any byte sequence" (TR-001)? -> A: Accept valid UTF-8 (a leading BOM preserved as trivia); reject non-UTF-8 cleanly at the boundary (never a panic), outside the round-trip domain.

## Scope *(mandatory)*

### Included

- A Cargo workspace (`ron-core`, `ron-types`, `ronin-app`, plus a reserved `ron-lsp` slot) rooted under `/src`.
- A lossless concrete syntax tree (CST) for RON that retains every byte of source (comments, whitespace, trailing commas, struct/variant names, tuples, chars, raw strings, and RON extensions).
- Byte-for-byte re-printing of an unmodified CST (parse→print is identity).
- Error-tolerant parsing that yields a complete tree plus structured diagnostics for any input, without panicking.
- A 0.x shape-stable `ron-core` public API for parsing, navigating the CST, reading diagnostics, and printing, with no underlying CST-library types in the surface.
- Foundational CST edit/transform primitives (insert/replace/remove) that preserve losslessness for unaffected regions.
- A WASM-clean `ron-core` that compiles to `wasm32-unknown-unknown`.

### Excluded

- Pretty-printing / reformatting that alters trivia — deferred to Smart Authoring (E005); E001 only re-prints exactly.
- Type-aware validation and schema/type acquisition — deferred to E004/E006 (`ron-core` exposes diagnostics, but type knowledge lives in `ron-types`).
- Incremental/partial reparse — deferred; E001 may fully reparse on change. Re-printing and editing remain correct regardless.
- The desktop UI itself — deferred to E003; this epic delivers the library it consumes.
- RON⇄JSON interop via serde `ron` — deferred to E010; serde `ron` is never the editing model.
- Transcoding or editing non-UTF-8 files — out of scope; such files are rejected cleanly and the editor (E003) surfaces the error.

### Edge Cases & Boundaries

- Empty file, whitespace-only file, and comments-only file.
- Missing trailing newline; CRLF vs LF line endings preserved exactly; a leading UTF-8 BOM preserved as leading trivia.
- Non-UTF-8 or wrong-encoding files: rejected at the API boundary with a clean error (never a partial or garbled load).
- Unicode in strings, char literals, and identifiers; raw strings (`r#"..."#`) with embedded quotes/hashes.
- Tuples vs lists/sequences vs maps with non-string keys preserved as distinct constructs.
- One or more extension attributes (`#![enable(implicit_some)]`, `unwrap_newtypes`, `unwrap_variant_newtypes`); unknown extension attribute still preserved as text.
- Malformed input: unclosed delimiters, stray tokens, partial top-level value — produce ERROR nodes covering all input, never a panic.
- Very large files (the **size** axis — total byte length, independent of structure) and deeply nested files (the **depth** axis — nesting depth, bounded by TR-014). These are two distinct axes and are not conflated: size has no committed bound, while depth is bounded per TR-014. For both axes the **only** committed behavior is correctness — correct byte-for-byte round-trip and guaranteed termination — with NO responsiveness, latency, throughput, or memory threshold (correctness-only envelope; memory is explicitly out of scope as a threshold for E001). The size axis is verified only via corpus round-trip (SC-001) and termination/no-panic (SC-003); it has no dedicated size-threshold success criterion by design (acknowledged gap, consistent with correctness-only). The depth axis is verified by SC-008.

## Technical Objectives *(mandatory for technical specs only)*

### Objective 1 - Lossless parse and round-trip (Priority: P1)

Parse RON source into a CST that preserves every byte, and re-print an unmodified CST so the output equals the input exactly.

**Why this priority**: This is the core value of the engine and the foundation every other epic depends on; without byte-for-byte round-trip the never-corrupt promise fails.

**Rationale**: The serde `ron` model loses comments/formatting/struct names (ADR-0001); a lossless CST (rowan red-green tree) is the only representation that satisfies the data-integrity principle.

**Deliverables**:
- A RON lexer/parser that builds a lossless CST retaining trivia and structure.
- A printer that emits an unmodified CST back to its exact source bytes.
- Coverage of the full RON grammar surface (structs, enums/variants, tuples, lists/seqs, maps incl. non-string keys, chars, strings incl. raw strings, numbers, bools, unit/`()`, `Option`/`implicit_some`, extension attributes).

**Validation Criteria**:
1. **Given** a valid RON file, **When** it is parsed and printed without edits, **Then** the output bytes equal the input bytes.
2. **Given** RON containing comments and trailing commas, **When** round-tripped, **Then** comments, ordering, and trailing commas are preserved verbatim.

### Objective 2 - Error-tolerant parsing and diagnostics (Priority: P1)

Always produce a complete tree covering all input plus structured diagnostics, for any byte sequence, without panicking.

**Why this priority**: The editor must function while a file is invalid or mid-edit; a parser that bails or panics blocks every interactive feature.

**Rationale**: Resilient LL parsing (error nodes + recovery sets + a must-consume-a-token invariant) keeps the tree usable and still round-trippable during editing.

**Deliverables**:
- Error-recovery parsing that wraps unexpected tokens in `ERROR` nodes and represents absent constructs as missing/empty nodes.
- A diagnostics model carrying a precise source byte range, message, a fixed severity enum (Error/Warning), and a stable namespaced code (e.g. `RON-Pxxxx`), emitting one diagnostic per recovery point.
- A bounded nesting/recursion guard that prevents stack overflow on deeply nested input and emits a diagnostic past the limit.

**Validation Criteria**:
1. **Given** malformed RON (e.g. an unclosed parenthesis), **When** parsed, **Then** a complete tree with ERROR node(s) and a diagnostic with a byte range is produced, with no panic.
2. **Given** arbitrary bytes, **When** parsed, **Then** parsing terminates and concatenating the tree's token texts equals the input.

### Objective 3 - WASM-clean workspace and core API (Priority: P1)

Establish the Cargo workspace and a 0.x shape-stable `ron-core` public API, with `ron-core` provably free of platform dependencies so it builds for `wasm32-unknown-unknown`.

**Why this priority**: Reuse across the desktop editor and future LSP/WASM frontends is a foundational architecture invariant; allowing native deps into the core now would force rework later.

**Rationale**: Hexagonal workspace with a WASM-clean core (ADR-0002); the only reliable proof of WASM-cleanliness is a successful wasm32 build.

**Deliverables**:
- Cargo workspace with `ron-core`, `ron-types`, `ronin-app` crates (and a reserved `ron-lsp`) rooted under `/src`.
- A documented `ron-core` public API (parse, navigate the CST, read diagnostics, print) that is 0.x shape-stable and exposes no underlying CST-library (rowan) types.
- Verification that `ron-core` compiles to `wasm32-unknown-unknown` with no filesystem/UI/async/native dependencies.

**Validation Criteria**:
1. **Given** the workspace, **When** `cargo build -p ron-core --target wasm32-unknown-unknown` runs, **Then** it compiles successfully.
2. **Given** a consumer crate depending only on `ron-core`, **When** it parses, navigates, reads diagnostics, and prints, **Then** it does so without any I/O or platform types in the API.

### Objective 4 - CST navigation and edit primitives (Priority: P2)

Provide typed navigation over RON constructs and foundational lossless edit primitives (insert/replace/remove) that downstream editing, persistence, and table epics build on.

**Why this priority**: Required by later epics (E007 undo, E008 structural/table editing) but not needed to demonstrate E001's core value of lossless, resilient round-trip.

**Rationale**: A single transform foundation in the core prevents each frontend from re-implementing tree mutation and keeps mutations lossless by construction.

**Deliverables**:
- Typed accessors to navigate RON nodes/values within the CST.
- Edit primitives (insert/replace/remove) supporting both syntax-node and token-span granularity, with a caller-chosen trivia policy (keep or discard leading/trailing trivia).

**Validation Criteria**:
1. **Given** a CST, **When** a node or token span is inserted, replaced, or removed under a chosen trivia policy, **Then** unaffected regions print byte-identically and the whole tree remains printable.

### Technical Constraints

- `ron-core` MUST be WASM-clean: no `std::fs`, `std::net`, `std::thread`, async runtimes, or native/`cc`-built dependencies (ADR-0002).
- The editing model MUST be the lossless CST, not serde `ron`; serde `ron` is reserved for later interop only (ADR-0001).
- The implementation uses rowan as the CST library (cstree is an acceptable substitute) per ADR-0001.
- Byte-for-byte round-trip is a hard invariant that MUST hold for every accepted (UTF-8) input — valid or malformed — including error-recovered trees. Non-UTF-8 input is outside this domain and is rejected cleanly.
- The `ron-core` public API is 0.x shape-stable and MUST NOT leak the underlying CST-library (rowan) types.
- E001 commits to correctness, not a performance threshold: the only non-functional guarantees are termination on any input and a bounded nesting depth (no stack overflow). A benchmark harness is established but sets no pass/fail gate. The project-wide Performance Standards (e.g. ~16 ms interactive frame budget, smooth 100k-row tables) are desktop-editor targets owned by later epics (E003+) and are explicitly NOT in scope for this library epic; no latency, throughput, or memory threshold applies to E001.
- Parsing MUST be deterministic (identical input yields an identical tree) and MUST terminate on any input.
- All project source under `/src`; Rust 2021 edition, stable toolchain. No network access or telemetry.

## Integration Points *(mandatory for technical and operational specs)*

- **IP-001**: Desktop editor shell (E003) depends on the `ron-core` parse/CST/print API to load, render, and save RON.
- **IP-002**: Type-aware validation (E006) depends on the `ron-core` CST and diagnostics surface; type knowledge is supplied separately by `ron-types` (E004).
- **IP-003**: Non-destructive persistence (E007) depends on the CST and the Objective-4 edit primitives for undo/redo.
- **IP-004**: Structural & table editing (E008) depends on the Objective-4 transform/edit primitives.
- **IP-005**: Round-trip & interop (E010) depends on the CST; serde `ron` is used only at the interop boundary, never inside `ron-core`.

## Requirements *(mandatory)*

### Technical Requirements *(technical specs only)*

- **TR-001**: System MUST accept any valid UTF-8 input (optionally prefixed with a UTF-8 BOM) and parse it into a CST without panicking; non-UTF-8 input MUST be rejected at the API boundary with a clean error (never a panic), outside the round-trip guarantee.
- **TR-002**: System MUST retain every source byte in the CST, including comments, whitespace, trailing commas, and struct/variant names.
- **TR-003**: System MUST reproduce an unmodified CST as output bytes identical to the original input.
- **TR-004**: System MUST represent the full RON grammar surface — structs, enums/variants, tuples, lists/sequences, maps (including non-string keys), chars, strings (including raw strings), numbers, booleans, unit, `Option`/`implicit_some`, and extension attributes — as distinct, preserved constructs. The in-scope surface is whatever the pinned `ron` crate version accepts; that version is recorded in the plan.
- **TR-005**: System MUST, for malformed or incomplete input, produce a complete tree whose tokens cover all input, using ERROR nodes, plus structured diagnostics.
- **TR-006**: System MUST attach a precise source byte range to every diagnostic.
- **TR-007**: System MUST keep `ron-core` free of filesystem, UI, async-runtime, and native dependencies and MUST compile it to `wasm32-unknown-unknown`.
- **TR-008**: System MUST provide a Cargo workspace containing `ron-core`, `ron-types`, and `ronin-app` crates (with a reserved `ron-lsp`) rooted under `/src`.
- **TR-009**: System MUST expose a `ron-core` public API to parse, navigate the CST, retrieve diagnostics, and print. The API is 0.x shape-stable — the parse/navigate/diagnostics/print capability areas are committed; breaking changes are allowed only within 0.x until downstream epics validate the shape — and MUST NOT expose the underlying CST-library (rowan) types, so the library remains swappable.
- **TR-010**: System MUST provide typed navigation accessors for each composite RON construct in the CST — struct, tuple, list/sequence, map and map-entry, enum/variant — plus scalar values, exposing their child elements/values through the public API.
- **TR-011**: System MUST provide edit primitives (insert/replace/remove) supporting both syntax-node and token-span granularity, with a caller-chosen trivia policy (keep or discard leading/trailing trivia); unaffected regions MUST print byte-identically and the tree MUST remain printable.
- **TR-012**: System MUST parse deterministically — identical input yields an identical tree AND an identical diagnostics set (same kinds, order, and byte ranges, including recovery and over-limit diagnostics), independent of any performance characteristic.
- **TR-013**: Diagnostics MUST use a fixed severity enum (Error, Warning) and a stable, namespaced code registry (e.g. `RON-Pxxxx`), emitting one diagnostic per recovery point; the severity values and codes are part of the stable public API.
- **TR-014**: The parser MUST terminate on all input and MUST NOT overflow the stack on deeply nested input — it enforces a documented nesting/recursion bound (default 128, configurable) and emits a diagnostic when input exceeds it.
- **TR-015**: Any RON construct or extension discovered in the corpus that is not already exercised by the grammar property-test strategies MUST be added to those strategies (closing the grammar-completeness loop), so corpus findings feed back into property coverage rather than remaining corpus-only.
- **TR-016**: System MUST provide a benchmark harness that measures parse/print over the corpus to inform later interactive-performance epics; the harness MUST NOT impose any pass/fail gate (no latency/throughput/memory threshold) and is therefore never a release-gating or QC criterion for E001.
- **TR-017**: Printing MUST be idempotent — for every accepted (UTF-8) input, `print(parse(print(parse(x))))` is byte-identical to `print(parse(x))`. (For unmodified trees this follows from round-trip identity TR-003; it is asserted independently as a property to guard the printer.)

### Key Entities *(include for product or technical specs if feature involves data)*

- **CST (Concrete Syntax Tree)**: The lossless in-memory representation of a RON document; the sum of its tokens' text equals the source bytes.
- **SyntaxNode / SyntaxToken**: Navigable tree elements; tokens carry verbatim text including trivia (comments, whitespace) and punctuation.
- **SyntaxKind**: The classification of nodes and tokens, including an `ERROR` kind for recovery.
- **Diagnostic**: A parse/structural finding with a source byte range, a message, a severity from a fixed enum (Error/Warning), and a stable namespaced code (e.g. `RON-Pxxxx`); one diagnostic is emitted per recovery point.

## Assumptions & Risks *(mandatory)*

### Assumptions

- A lossless red-green CST library (rowan, with pure-Rust deps) is suitable and WASM-clean, per the rust-analyzer precedent.
- A representative corpus of real serde and Bevy `.scn.ron` files can be assembled for round-trip and corpus testing (≥ 30 files; ≥ 1 per TR-004 construct group; ≥ 3 malformed; ≥ 1 file ≥ 1 MB), per SC-001.
- Full reparse on change is acceptable for this epic; incremental reparse is deferred without affecting correctness.
- The target grammar is the RON specification and its documented extensions as implemented by the `ron` crate.

### Risks

- **Grammar completeness** *(likelihood: medium, impact: high)*: A missing construct or extension silently breaks round-trip. Mitigation: property tests, fuzzing, and a real-file corpus gate.
- **Inconsistent trivia model** *(likelihood: low, impact: medium)*: Choosing trivia attachment inconsistently complicates navigation or losslessness. Mitigation: fix one attachment rule up front and assert round-trip everywhere.
- **Ambiguous literal forms** *(likelihood: medium, impact: medium)*: Raw strings, numeric/escape forms, and non-string keys may have subtle representations. Mitigation: targeted snapshot tests per literal kind.

## Implementation Signals *(mandatory)*

- `NEW-ENTITY` — The CST data model (nodes/tokens/kinds) and the Diagnostic model.
- `NEW-API` — The `ron-core` public library API (parse, navigate, diagnostics, print, edit primitives) consumed by all frontends.
- `NEW-CONFIG` — Cargo workspace and crate manifests (`ron-core`, `ron-types`, `ronin-app`, reserved `ron-lsp`) under `/src`, including the `wasm32` build target.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001** [OBJ1]: Across a representative serde + Bevy RON corpus, parse→print reproduces 100% of files byte-for-byte. A corpus file that is itself malformed still counts toward the 100%: its error-recovered tree (ERROR nodes covering all input, INV-3) MUST re-print to the original bytes, so every accepted UTF-8 corpus file — valid or malformed — is included in the 100% denominator. The corpus comprises at least 30 real RON files spanning serde structs/enums and Bevy `.scn.ron` scenes, with at least one file per TR-004 construct group, at least 3 malformed files, and at least one large file ≥ 1 MB (a representative-corpus/benchmark boundary, not an engine size limit).
- **SC-002** [OBJ1]: Property tests assert round-trip identity over generated RON covering every grammar construct in TR-004.
- **SC-003** [OBJ2]: Fuzzing finds no panics on arbitrary input; for every accepted (UTF-8) input — valid or malformed — the concatenation of the tree's token texts equals the input, and non-UTF-8 input is rejected without panicking. Acceptance is a seeded `cargo-fuzz` run of ≥ 1,000,000 iterations (the deterministic CI gate; wall-clock time is not a pass condition) seeded from the test corpus that finds zero panics; any crash input discovered is added to the seed corpus as a regression case.
- **SC-004** [OBJ2]: For a malformed-sample set, every produced diagnostic carries a correct source byte range and the tree still covers all input.
- **SC-005** [OBJ3]: `cargo build -p ron-core --target wasm32-unknown-unknown` succeeds with no filesystem/UI/async/native dependencies.
- **SC-006** [OBJ3]: A consumer can parse, navigate, read diagnostics, and print using only the `ron-core` public API, which exposes no I/O types and no underlying CST-library (rowan) types.
- **SC-007** [OBJ4]: After insert/replace/remove edits (node or token-span, under a chosen trivia policy), unaffected regions print byte-identically and the full tree remains printable.
- **SC-008** [OBJ2]: Deeply nested input does not overflow the stack; the parser enforces a documented nesting bound (default 128, configurable) and emits a diagnostic when it is exceeded.
- **SC-009** [OBJ1]: Property tests assert idempotent printing — for generated and corpus RON, re-parsing printed output and printing again is byte-identical to the first print (print∘parse is idempotent).

### Requirement Coverage Map

Every success criterion traces to at least one technical requirement and at least one invariant (INV-1..INV-9, defined in data-model.md). INV-9 captures the WASM-clean portability invariant backing the build-gate criterion SC-005.

| Success Criterion | Backing Requirement(s) | Backing Invariant(s) |
|-------------------|------------------------|----------------------|
| SC-001 | TR-003 | INV-2, INV-3 |
| SC-002 | TR-003, TR-004 | INV-2 |
| SC-003 | TR-001, TR-005 | INV-1, INV-3, INV-4 |
| SC-004 | TR-005, TR-006 | INV-1, INV-3 |
| SC-005 | TR-007 | INV-9 |
| SC-006 | TR-009 | INV-7 |
| SC-007 | TR-011 | INV-8 |
| SC-008 | TR-014 | INV-5 |
| SC-009 | TR-017 | INV-2 |

Requirements verified by tasks rather than a success criterion (acknowledged coverage routes, not gaps): TR-010 (typed accessors — exercised by the consumer test and by accessor usage in the edit tests), TR-015 (corpus→property feedback — verified by the corpus harness, task T038), and TR-016 (benchmark harness — informational and non-gating by design, verified only by the harness building and running over the corpus, task T036; no latency/throughput/memory threshold).

## Stress-Test Findings

### Session 2026-06-11

- **STF-001** *(category: terminology consistency, severity: low)*: The `ron-core` public API was described as both "stable" (Scope, Objective 3) and "0.x shape-stable" (TR-009, Constraints), implying conflicting stability guarantees. **Resolution (accepted, applied inline)**: standardized on "0.x shape-stable" across Scope, Objective 3, TR-009, and Technical Constraints.

No CRITICAL or HIGH internal contradictions found; the UTF-8 input contract (TR-001) and the absolute round-trip guarantee (Q2) were reconciled during integration by scoping round-trip to "every accepted (UTF-8) input."

## Glossary *(include when spec introduces 2+ domain-specific terms)*

| Term | Definition |
|------|------------|
| RON | Rusty Object Notation — a human-friendly serialization format for the Rust/serde ecosystem; richer than JSON and not self-describing. |
| CST | Concrete Syntax Tree — a tree that preserves every source byte (including trivia), enabling exact reconstruction. |
| Trivia | Source text with no semantic value to the data model (whitespace, comments) that must still be preserved for losslessness. |
| Round-trip | Parsing then re-printing an unmodified tree; "lossless" means the result equals the original bytes exactly. |
| Red-green tree | The rowan CST design: an immutable green tree of text+width plus an on-demand red tree of absolute offsets/parents. |
| Error-tolerant (resilient) parsing | Parsing that never panics and always yields a complete tree plus diagnostics, even for invalid input. |
| WASM-clean | Free of filesystem/UI/async/native dependencies so the crate compiles to `wasm32-unknown-unknown`. |
| serde `ron` crate | The existing serde-based RON library; used by RONin only for later interop, never as the editing model. |
| Recovery point | A position in malformed input where the parser resynchronizes via its recovery sets; exactly one diagnostic is emitted per recovery point. |
| 0.x shape-stable | The public API's capability areas (parse/navigate/diagnostics/print) are committed; signatures and types may still change in breaking ways within 0.x until downstream epics validate the shape. |

## Compliance Check

**Overall**: PASS — no contradictions with non-negotiable governance; zero CRITICAL items. Audited against `project-instructions.md` (Principles I–VI, Source Code Layout, Tech Stack, Workflow) and ADR-0001/ADR-0002.

| Rule | Verdict | Evidence |
|------|---------|----------|
| Principle I — Never Corrupt User Data | Compliant | Obj1, TR-002/003, round-trip hard-invariant constraint, SC-001/002 |
| Principle II — One Core, Many Surfaces (WASM-clean) | Compliant | Obj3, TR-007, no-fs/UI/async/native constraint, SC-005 |
| Principle V — Verified Quality | Compliant | Property (SC-002), fuzzing (SC-003), corpus (SC-001), wasm32 gate (SC-005); save/recovery fault-injection correctly deferred to E007 |
| Principle VI — Local-First & Private | Compliant | "No network access or telemetry" constraint |
| Source Code Layout — all source under /src | Compliant | Scope, TR-008, NEW-CONFIG |
| ADR-0001 — lossless CST is the editing model | Compliant | Rationale + constraint; serde `ron` reserved for E010 interop |
| ADR-0002 — hexagonal workspace, WASM-clean core | Compliant | Obj3, crate set matches ADR exactly |

**Advisory (non-blocking)**: rustfmt/Clippy/cross-OS CI gates from Principle V are owned by the Plan/QC phases, not spec success criteria; the spec does not contradict them.
