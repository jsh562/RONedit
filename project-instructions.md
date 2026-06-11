<!-- template-version: 2 -->
# RONin Project Instructions

## Core Principles

### I. Never Corrupt User Data

The editor MUST preserve comments, formatting, key/field ordering, and struct names through every edit, and MUST persist changes via atomic save (temp-write + fsync + rename) with sidecar crash recovery. A failed or interrupted save MUST leave the original file untouched. — A format editor that loses or reflows a user's file is worse than no tool; data trust is RONin's core value (see `specs/adrs/0001-lossless-cst-editing-model.md`, `specs/adrs/0005-non-destructive-persistence.md`).

### II. One Core, Many Surfaces

All RON intelligence (parse, validate, format, transform) MUST live in a reusable, I/O-free `ron-core` crate that compiles to both native and `wasm32`. UI, filesystem, async-runtime, and other native-only dependencies MUST stay out of `ron-core`. — A single portable core lets the desktop editor and future LSP/VSCode frontends reuse one engine without rework (see `specs/adrs/0002-hexagonal-cargo-workspace.md`).

### III. Progressive Intelligence

Features MUST be useful with zero setup (structural intelligence) and MUST become type-aware automatically when type information is available. Missing or unresolved types MUST degrade gracefully and MUST NOT produce false-positive errors. — Low-friction onboarding with optional depth is RONin's differentiator versus manual-annotation tools (see `specs/adrs/0004-schema-optional-type-model.md`).

### IV. Agent Output Style

All agent output MUST be concise and outcome-oriented. This principle supersedes any verbose defaults.

- **Progress reports**: Facts and outcomes only — no narration, no restating the task.
- **Artifacts**: Emit required sections only — no preamble paragraphs, no summary epilogues.
- **Reasoning**: Omit unless the user asks "why" or the decision is non-obvious.
- **Errors / blockers**: State the problem, the attempted fix, and the result — nothing else.
- **Phase-boundary reports**: ≤ 5 bullet points.
- **Preserve without compressing**: Artifact template structure and required sections; explicit decision / registration / validation guidance in shared skills; delegation constraints and sub-agent role definitions; existing size limits (spec ≤ 1000 KB, research ≤ 400 KB, stories ≤ 200 words).

### V. Verified Quality

Lossless round-trip, parser error-tolerance, and save/recovery integrity MUST be covered by property, snapshot, and fault-injection tests on a real serde + Bevy RON corpus. Every change MUST pass rustfmt, Clippy (deny warnings), the test suite on Windows/macOS/Linux, and a `wasm32` build of `ron-core` before merge. — Invariants this strict can only be trusted if they are continuously and automatically verified (see `specs/dod.md` DDR-005).

### VI. Local-First & Private

The application MUST NOT make network calls or collect telemetry by default. User-provided Rust source MUST be parsed statically, never executed. — Offline usability and developer trust are non-negotiable for a local file editor (see `specs/dod.md` DDR-006).

## Technology Stack

<!-- Downstream phases (Plan, QC, Autopilot) read this section as the authoritative tech-stack reference. -->

- **Language/Runtime**: Rust (2021 edition, stable toolchain)
- **Frameworks**: egui/eframe + egui_extras (desktop GUI and virtualized tables); rowan (lossless concrete syntax tree); syn (static Rust type extraction); schemars + jsonschema (normalized type model and validation); serde + the `ron` crate (RON⇄JSON interop only, never the editing model); tracing (local structured logging)
- **Storage**: Local filesystem only — user RON files, sidecar autosave/recovery files, and app settings in the OS config directory. No database, no server.
- **Infrastructure**: Local-first desktop for Windows/macOS/Linux; future `wasm32` target for browser/VSCode-webview frontends; GitHub Actions for CI; crates.io + GitHub Releases for distribution. No hosted runtime.

## Testing & Quality Policy

<!-- QC extracts enforcement rules from this section. Use the keywords below so automated checks activate correctly. -->
<!-- Keywords recognised by QC: lint, static analysis, code quality, coverage, security, vulnerability, OWASP, WCAG, accessibility, benchmark, performance -->

- **Coverage Target**: none — correctness is enforced through mandatory invariant tests rather than a line-coverage percentage.
- **Required QC Categories**: linting / static analysis (Clippy); security scanning (cargo-audit + cargo-deny for vulnerability and license/advisory checks).
- **Test Strategy**: Test-after implementation, with MANDATORY invariant tests for `ron-core` and persistence — byte-lossless round-trip (property + snapshot), parser error-tolerance on malformed input, and save/recovery fault-injection (disk full, kill mid-save). Tooling: `cargo test`, `insta` (snapshots), `proptest` (properties), and corpus tests against real serde and Bevy RON files.
- **Linting / Formatting**: rustfmt (enforced) + Clippy with `-D warnings`. CI additionally requires a successful `wasm32` build of `ron-core` and a passing test matrix on Windows/macOS/Linux.

## Source Code Layout

- **Policy**: ENFORCE_SRC_ROOT
- **Convention**: All project source code MUST live under `/src`. The Cargo workspace crates are rooted there (`/src/ron-core`, `/src/ron-types`, `/src/ronin-app`, with a reserved `/src/ron-lsp` for the future frontend). Unit tests are co-located within each crate; integration tests and shared RON corpus fixtures live under each crate's `tests/` directory.

## Development Workflow

- **Branching**: Feature branches from `main`, squash merge. Feature branches follow the `#####-feature-name` convention.
- **Commit Convention**: Conventional Commits (consumed by `release-plz` for automated changelog and version bumps).
- **CI Requirements**: Before merge, all of the following must pass — rustfmt clean, Clippy `-D warnings`, test suite green on Windows/macOS/Linux, `wasm32` build of `ron-core`, and `cargo-audit` + `cargo-deny` clean.

## Performance Standards

<!-- Targets for the desktop editor; tracked via corpus benchmarks. Not yet a hard QC gate. -->

- Interactive editing within the ~16 ms frame budget; incremental reparse of the edited region only.
- Virtualized table rendering remains smooth for 100k+ rows; heavy reparse/validation runs off the per-frame UI path.
- Large single files (multi-MB Bevy scenes, large uniform datasets) stay responsive. Validated via corpus benchmarks.

## Governance

- Project instructions supersede all other documentation and practices.
- Amendments require a version bump with ISO-dated changelog entry.
- All implementations MUST pass the Instructions Check gate during planning.
- Complexity beyond these principles MUST be justified and documented.
- Project-level architectural decisions are recorded as ADRs under `specs/adrs/`; deployment/operations decisions are recorded as DDRs in `specs/dod.md`.
- `ron-core` MUST remain WASM-clean; the CI `wasm32` build gate enforces this invariant.
- No telemetry or network access is added without an explicit, documented, opt-in decision.

**Version**: 1.0.0 | **Last Amended**: 2026-06-11
