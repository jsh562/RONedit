//! The in-memory editor document model and its byte-fidelity profile.
//!
//! Two concerns live here:
//!
//! * [`ByteFidelityProfile`] (FR-020) captures everything a *lossless* save needs
//!   to re-emit a file exactly as the user expects: original line-ending style,
//!   whether the file had a trailing newline, whether it carried a UTF-8 BOM, and
//!   a cheap content hash of the loaded bytes. RONin's first principle is "never
//!   corrupt user data" (project-instructions §I); this profile is how the shell
//!   honours that on round-trip.
//! * [`EditorDocument`] (FR-007) is the per-tab document: the editable buffer, its
//!   on-disk identity, a saved snapshot for dirty-tracking, cursor/scroll state,
//!   and optional derived parse/highlight artifacts produced off the UI thread.
//!
//! # E007 — Autosave / crash recovery (OBJ2)
//!
//! The document carries an autosave/recovery lifecycle hook
//! ([`EditorDocument::recovery`], an [`AutosaveDebounce`](crate::recovery::AutosaveDebounce))
//! that the shell drives off the per-frame path: it debounces a recovery-sidecar
//! write while the buffer is dirty + actually changed, and the shell removes the
//! sidecar on a clean save / clean exit. An **untitled** buffer (no `path`) has
//! **no** sidecar (TR-017).
//!
//! # E007 — Bounded CST-backed undo/redo (OBJ3)
//!
//! The document owns a WASM-clean [`UndoStack`](ron_core::UndoStack) keyed to its
//! [`CursorState`] ([`EditorDocument::undo`]). The shell records a snapshot at
//! coalesce-unit boundaries off the per-frame hot path
//! ([`EditorDocument::record_undo_snapshot`]) — at most once per coalesce window,
//! not per keystroke (TR-016/TR-023, SC-008) — and [`EditorDocument::undo`] /
//! [`EditorDocument::redo`] restore the **exact prior in-memory bytes** + cursor
//! by replacing the buffer with the entry's `source_text` and bumping
//! `edit_generation` so a reparse runs and dirty-tracking recomputes. Undo/redo
//! operate **solely** on the in-memory buffer/CST/cursor and never read or write
//! the file (TR-018). The coalesce *timing* decision is computed here
//! (`Instant` elapsed since the last edit vs the configured window) and passed to
//! `ron-core` as a plain `bool`, keeping `ron-core` clock-free (TR-014).
//!
//! The dirty-tracking and `edit_generation` machinery here is the seam both OBJ2
//! and OBJ3 build on.

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use ron_core::{UndoEntry, UndoStack};

use crate::binding::{BindingOrigin, BindingState, DocumentOverride, TypeBinding};
use crate::completion::CompletionState;
use crate::diagnostics_map::{map_diagnostic, DiagnosticView};
use crate::editor_view::build_highlight_model;
use crate::reparse::{BoundType, ParseResult, ReparseWorker};

/// Process-wide monotonic source of per-document identity tokens.
///
/// Each [`EditorDocument`] takes one at construction; the token is stable for the
/// document's lifetime and is never reused, so batch tab operations can track a
/// tab by identity even as tab indices shift around it (FR-026).
static NEXT_DOC_ID: AtomicU64 = AtomicU64::new(1);

/// Mint the next process-unique document identity token.
fn mint_doc_id() -> u64 {
    NEXT_DOC_ID.fetch_add(1, Ordering::Relaxed)
}

/// The newline convention detected in a loaded file (FR-020).
///
/// `Crlf`/`Lf` describe a file that uses one style uniformly. `Mixed` marks a
/// file that contained *both* `\r\n` and lone `\n`; the dominant style is then
/// carried separately on [`ByteFidelityProfile::dominant`] so a later save can
/// normalise to a single, predictable convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineEnding {
    /// Windows-style carriage-return + line-feed (`\r\n`).
    Crlf,
    /// Unix-style line-feed (`\n`).
    Lf,
    /// The file mixed `\r\n` and lone `\n`.
    Mixed,
}

/// Byte-level fidelity metadata captured when a file is loaded (FR-020).
///
/// Everything here is needed to reproduce the user's file faithfully on save:
/// the line-ending style (with a never-`Mixed` [`dominant`](Self::dominant) for
/// re-emission), trailing-newline presence, BOM presence, and a content hash of
/// the originally loaded bytes for cheap change detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteFidelityProfile {
    /// The detected line-ending style of the loaded file.
    pub line_ending: LineEnding,
    /// For `Mixed` files, the more frequent concrete style to normalise to on
    /// save; for uniform files this equals [`line_ending`](Self::line_ending).
    /// Invariant: this is always [`LineEnding::Crlf`] or [`LineEnding::Lf`],
    /// never [`LineEnding::Mixed`]. Ties resolve to [`LineEnding::Lf`].
    pub dominant: LineEnding,
    /// `true` when the loaded file ended with a newline (`\n`).
    pub had_trailing_newline: bool,
    /// `true` when the loaded bytes began with a UTF-8 BOM (`EF BB BF`).
    pub had_bom: bool,
    /// A cheap hash of the originally loaded raw bytes, for change detection.
    pub original_hash: u64,
}

/// The UTF-8 BOM byte sequence (`EF BB BF`).
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

impl ByteFidelityProfile {
    /// Analyse raw file bytes and capture the fidelity profile (FR-020).
    ///
    /// Detection rules:
    /// * BOM: leading `EF BB BF`.
    /// * Line endings: count `\r\n` pairs versus lone `\n` (a `\n` not preceded
    ///   by `\r`). All-CRLF → [`LineEnding::Crlf`]; all-LF → [`LineEnding::Lf`];
    ///   both present → [`LineEnding::Mixed`]. A file with no newlines is treated
    ///   as [`LineEnding::Lf`] (the safe default for re-emission).
    /// * `dominant`: the more frequent of CRLF/LF; ties (including the
    ///   no-newline case) resolve to [`LineEnding::Lf`]; never `Mixed`.
    /// * `had_trailing_newline`: the bytes end in `\n`.
    /// * `original_hash`: hash of the raw bytes (length-sensitive).
    #[must_use]
    pub fn from_bytes(raw: &[u8]) -> Self {
        let had_bom = raw.starts_with(&BOM);

        // Count CRLF pairs and lone LFs in a single pass.
        let mut crlf = 0usize;
        let mut lone_lf = 0usize;
        let mut prev_cr = false;
        for &b in raw {
            if b == b'\n' {
                if prev_cr {
                    crlf += 1;
                } else {
                    lone_lf += 1;
                }
            }
            prev_cr = b == b'\r';
        }

        let line_ending = match (crlf > 0, lone_lf > 0) {
            (true, true) => LineEnding::Mixed,
            (true, false) => LineEnding::Crlf,
            (false, true) => LineEnding::Lf,
            // No newlines at all: default to LF for predictable re-emission.
            (false, false) => LineEnding::Lf,
        };

        // Dominant is the more frequent concrete style; ties (and the
        // no-newline case) resolve to LF. Never `Mixed`.
        let dominant = if crlf > lone_lf {
            LineEnding::Crlf
        } else {
            LineEnding::Lf
        };

        let had_trailing_newline = raw.last() == Some(&b'\n');

        let original_hash = hash_bytes(raw);

        Self {
            line_ending,
            dominant,
            had_trailing_newline,
            had_bom,
            original_hash,
        }
    }
}

/// Hash arbitrary bytes with the standard library's default hasher.
///
/// Used for cheap content-change detection; the exact algorithm is an
/// implementation detail (not stable across toolchains) and is never persisted.
fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// A minimal placeholder for computed syntax-highlight spans.
///
/// Wave 1 only needs a concrete, real type so [`EditorDocument`] can hold an
/// `Option<HighlightModel>`; later waves (editor view) will populate it with
/// actual highlight spans derived from the CST. It is intentionally cheap and
/// inert for now — not a `// TODO` stub but a real, empty-by-default model.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HighlightModel {
    /// The reparse generation the spans were computed from, when populated.
    /// `None` means "no highlight computed yet".
    pub generation: Option<u64>,
    /// Highlight spans as `(char_start, char_end, class)` triples. Empty until a
    /// later wave computes them from the CST.
    pub spans: Vec<HighlightSpan>,
}

/// A single highlight span over character offsets. Reserved for the editor view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Inclusive start char offset.
    pub start: usize,
    /// Exclusive end char offset.
    pub end: usize,
    /// A stable, human-readable highlight class name (e.g. `"string"`).
    pub class: String,
}

/// A snapshot of the last-saved (or last-loaded) document state, used to derive
/// the dirty flag without retaining a second full copy of the buffer.
///
/// We store a content hash plus length: comparison is O(1) and false-positives
/// are astronomically unlikely for editor-sized buffers. Length is included so
/// trivially-different buffers never alias on hash alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedSnapshot {
    /// Hash of the buffer contents at save/load time.
    content_hash: u64,
    /// Byte length of the buffer at save/load time.
    len: usize,
}

impl SavedSnapshot {
    /// Capture a snapshot of the given buffer contents.
    #[must_use]
    pub fn of(buffer: &str) -> Self {
        Self {
            content_hash: hash_bytes(buffer.as_bytes()),
            len: buffer.len(),
        }
    }

    /// `true` if `buffer` still matches this snapshot (same length and hash).
    #[must_use]
    pub fn matches(&self, buffer: &str) -> bool {
        self.len == buffer.len() && self.content_hash == hash_bytes(buffer.as_bytes())
    }
}

/// Caret, selection, and scroll state for a document, in **character** offsets.
///
/// Character offsets (not byte offsets) are used throughout the editor surface so
/// the model is independent of UTF-8 encoding width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorState {
    /// Caret position as a character offset into the buffer.
    pub caret: usize,
    /// Active selection as an ordered `(anchor, head)` char-offset pair, if any.
    pub selection: Option<(usize, usize)>,
    /// Vertical scroll offset (logical pixels), preserved across reparses.
    pub scroll: f32,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            caret: 0,
            selection: None,
            scroll: 0.0,
        }
    }
}

/// One editor tab's document: editable text plus on-disk identity and derived
/// state (FR-007).
///
/// Construct via [`EditorDocument::from_loaded`] (an existing file) or
/// [`EditorDocument::new_untitled`] (a fresh buffer). The [`dirty`](Self::dirty)
/// and [`oversize`](Self::oversize) predicates are derived, never stored.
#[derive(Debug, Clone)]
pub struct EditorDocument {
    /// A process-unique identity token, stable for this document's lifetime.
    ///
    /// Used by batch tab operations (FR-026) to track a specific tab by identity
    /// while indices shift as other tabs close. Cloning a document copies the
    /// token (a clone is "the same document" for tracking purposes).
    id: u64,
    /// The live, editable text buffer (always valid UTF-8).
    pub buffer: String,
    /// The file this document maps to on disk, or `None` for an unsaved buffer.
    pub path: Option<PathBuf>,
    /// Snapshot of the content at the last save/load, for dirty-tracking.
    pub last_saved: SavedSnapshot,
    /// Byte-fidelity metadata captured at load (or defaults for a new buffer).
    pub byte_profile: ByteFidelityProfile,
    /// Caret/selection/scroll state in character offsets.
    pub cursor: CursorState,
    /// The most recent off-thread parse result, when one has been installed.
    pub parse: Option<ParseResult>,
    /// The most recent computed highlight model, when one has been installed.
    pub highlight: Option<HighlightModel>,
    /// The last-good diagnostics projected into editor coordinates (FR-008).
    ///
    /// Refreshed only when a fresh, current [`ParseResult`] lands via
    /// [`poll_parse`](Self::poll_parse); deliberately **not** cleared on edit so
    /// the views keep showing the last-good problems while a reparse is in flight
    /// (FR-006).
    pub diagnostics: Vec<DiagnosticView>,
    /// For untitled documents, the workspace-assigned sequence number used to
    /// render a stable `Untitled-N` title. `None` for on-disk documents.
    pub untitled_seq: Option<u32>,
    /// Monotonic edit generation: bumped on every buffer mutation (FR-006). A
    /// landed [`ParseResult`] installs only when its generation equals this, so
    /// stale results are discarded.
    edit_generation: u64,
    /// The generation last handed to the [`ReparseWorker`]. Coalesces requests:
    /// a request is only sent when the edit generation has actually advanced past
    /// what was last requested, so rapid keystrokes collapse to the latest text.
    last_requested_generation: u64,
    /// A pending caret jump to apply to the editor on the next frame, expressed
    /// as a **character** offset into the buffer (FR-009).
    ///
    /// Set when the user clicks a Problems-panel row; consumed by `editor_view`,
    /// which pushes it into the live `TextEdit` cursor state and scrolls it into
    /// view. A stale offset (past the current buffer) is clamped to the buffer's
    /// character length when consumed, so navigation never lands out of bounds
    /// even if the buffer shrank since the diagnostic was produced.
    pending_cursor_jump: Option<usize>,
    /// The structural-autocomplete popup state for this document (E005 Wave 3).
    ///
    /// Cross-frame state for the custom completion popup `editor_view` renders over
    /// the editor: open/closed, the candidate items, the explicitly-highlighted
    /// index (never preselected), and the trigger offset. Recomputed from the live
    /// buffer + caret each frame; default is closed.
    pub completion: CompletionState,
    /// The live snippet tab-stop navigation session, when one is in progress (E005
    /// Wave 4, FR-016).
    ///
    /// Set when a snippet is inserted (the buffer is spliced and the caret jumps to
    /// the first tab-stop); `editor_view` drives `Tab`/`Shift+Tab` over it and clears
    /// it once navigation ends (`$0` reached, `Esc`, or the buffer edited out from
    /// under it). `None` when no snippet navigation is active.
    pub snippet_session: Option<crate::snippets::SnippetSession>,
    /// The type this document is currently bound to, when any (E006/FR-006).
    ///
    /// Passed to the off-frame [`ReparseWorker`] on each
    /// [`request_reparse`](Self::request_reparse) so type validation runs against
    /// it on the worker thread. `None` means no binding resolved — only structural
    /// diagnostics are produced (FR-015). The real `BindingConfig`→`BoundType`
    /// resolution that populates this is Phase 4 (US2); for now it defaults to
    /// `None` and is the seam the binding resolver / per-document override will set.
    pub bound_type: Option<BoundType>,
    /// The resolved active binding for this document, for **display** (E006 US2 —
    /// FR-011).
    ///
    /// This is the user-facing answer to "which type, from which source, does this
    /// document conform to?" — or [`BindingState::NoBinding`]. It is recomputed by
    /// the shell's binding-resolution step (`App::apply_binding_to_active`) whenever
    /// the document becomes active / is opened or its override / the project config
    /// changes, then surfaced via [`binding_label`](Self::binding_label). It is
    /// kept *separate* from [`bound_type`](Self::bound_type) (which the worker runs
    /// against): `binding` is always meaningful for the UI even when acquisition
    /// degrades to structural-only (so the indicator shows the *intended* type while
    /// `bound_type` stays `None`). Defaults to [`TypeBinding::none`].
    pub binding: TypeBinding,
    /// The per-document **session** override, when the user has explicitly bound
    /// this document to a chosen type + source (E006 US2 — FR-009).
    ///
    /// When set it takes precedence over any project [`BindingConfig`](crate::binding::BindingConfig)
    /// rule (override > config) and produces a [`BindingOrigin::Override`] binding.
    /// Never persisted — only the project config persists. `None` means the document
    /// falls back to config resolution (or no binding). Set/cleared via the shell's
    /// override control, which then re-applies the binding so it takes effect
    /// immediately.
    pub override_: Option<DocumentOverride>,
    /// Whether type validation is currently degraded for this document because it is
    /// **oversize** past E003's large-file threshold (E006 T040 — FR-015/FR-024).
    ///
    /// This mirrors, for type validation, exactly what E003 already does for
    /// highlighting / squiggles: past
    /// [`AppSettings::effective_large_file_threshold`](crate::settings::AppSettings::effective_large_file_threshold)
    /// the always-on intelligence degrades. When `true`,
    /// [`request_reparse`](Self::request_reparse) ships **no** bound type to the
    /// worker, so the worker produces zero type diagnostics (FR-015's structural-only
    /// behavior) — the document still parses structurally, identical to how an
    /// oversize document still parses but renders no squiggles/highlights. The flag
    /// is reconciled against the *live* buffer size every frame by the shell's
    /// document pump (`App::reconcile_validation_degrade`), so editing an oversize
    /// document back down below the threshold automatically resumes validation on the
    /// next reparse. It is purely derived (never persisted, never user-set).
    pub validation_suppressed: bool,
    /// The autosave / crash-recovery lifecycle hook for this document (E007 OBJ2 —
    /// TR-006..009/TR-016).
    ///
    /// A frame-driven, deterministic [`AutosaveDebounce`](crate::recovery::AutosaveDebounce):
    /// the shell calls [`note_change`](Self::note_change) when the buffer changes and
    /// [`should_autosave`](Self::should_autosave) each frame; when it fires, the shell
    /// hands a [`RecoverySidecar`](crate::recovery::RecoverySidecar) snapshot to the
    /// off-frame [`AutosaveWorker`](crate::recovery::AutosaveWorker) and then calls
    /// [`mark_autosaved`](Self::mark_autosaved). An untitled buffer (no `path`) is
    /// never autosaved (TR-017). Not persisted; rebuilt per session from settings.
    recovery: crate::recovery::AutosaveDebounce,
    /// The bounded, WASM-clean CST-backed undo/redo history for this document
    /// (E007 OBJ3 — TR-010..014/TR-018/TR-024/TR-027).
    ///
    /// Keyed to the document's [`CursorState`]; each [`UndoEntry`] snapshots the
    /// exact buffer bytes + CST + cursor at a coalesce-unit boundary. The shell
    /// records snapshots off the per-frame path via
    /// [`record_undo_snapshot`](Self::record_undo_snapshot) and drives
    /// [`undo`](Self::undo) / [`redo`](Self::redo), which restore the exact prior
    /// **in-memory** bytes (never the on-disk file, TR-018). Constructed with the
    /// default config; the shell syncs the live cap + coalesce window each frame
    /// via [`set_undo_config`](Self::set_undo_config). In-memory / session-scoped:
    /// never persisted (no revision log; Scope/Excluded).
    undo: UndoStack<CursorState>,
    /// The edit generation the last undo snapshot was recorded for (E007 OBJ3).
    ///
    /// Coalesces undo bookkeeping off the per-keystroke path: a snapshot is taken
    /// only when the live [`edit_generation`](Self::edit_generation) has advanced
    /// past this, so a burst of edits collapses to the latest text — the same
    /// generation-keyed pattern the reparse/autosave seams use. `None` until the
    /// initial state is seeded by [`seed_undo`](Self::seed_undo).
    last_undo_generation: Option<u64>,
    /// The instant the last undo snapshot was recorded (E007 OBJ3 — TR-027).
    ///
    /// The caller-side coalesce timing source (`ron-core` measures no clock,
    /// TR-014): [`record_undo_snapshot`](Self::record_undo_snapshot) compares the
    /// elapsed time since this against the configured coalesce window to decide
    /// whether the new edit extends the current undo unit or starts a new one.
    /// `None` until the first snapshot is recorded.
    last_undo_instant: Option<Instant>,
    /// The coalesce window the undo stack was configured with, as a `Duration`
    /// (E007 OBJ3 — TR-027). Kept here so the caller-side coalesce decision uses
    /// the same (clamped) window the stack carries; synced by
    /// [`set_undo_config`](Self::set_undo_config).
    undo_coalesce_window: std::time::Duration,
}

impl EditorDocument {
    /// Build a document from a freshly loaded file's raw bytes (FR-007).
    ///
    /// Decodes UTF-8 (rejecting invalid input), captures the byte-fidelity
    /// profile, strips a leading BOM from the editable buffer (its presence is
    /// remembered on the profile for faithful re-emission), and records the
    /// loaded content as the saved snapshot so the document starts clean.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`std::str::Utf8Error`] if `raw` is not valid
    /// UTF-8. (Higher layers — see `fileio` — map this to a user-facing error.)
    pub fn from_loaded(path: impl Into<PathBuf>, raw: &[u8]) -> Result<Self, std::str::Utf8Error> {
        let profile = ByteFidelityProfile::from_bytes(raw);
        let text = std::str::from_utf8(raw)?;
        // The BOM is fidelity metadata, not editable content: keep it out of the
        // buffer but remembered on the profile so save can re-emit it.
        let buffer = text.strip_prefix('\u{FEFF}').unwrap_or(text).to_string();
        let last_saved = SavedSnapshot::of(&buffer);
        let mut doc = Self {
            id: mint_doc_id(),
            buffer,
            path: Some(path.into()),
            last_saved,
            byte_profile: profile,
            cursor: CursorState::default(),
            parse: None,
            highlight: None,
            diagnostics: Vec::new(),
            untitled_seq: None,
            edit_generation: 0,
            last_requested_generation: 0,
            pending_cursor_jump: None,
            completion: CompletionState::new(),
            snippet_session: None,
            bound_type: None,
            binding: TypeBinding::none(),
            override_: None,
            validation_suppressed: false,
            recovery: crate::recovery::AutosaveDebounce::new(
                crate::settings::AutosaveConfig::default(),
            ),
            undo: UndoStack::new(),
            last_undo_generation: None,
            last_undo_instant: None,
            undo_coalesce_window: ron_core::undo::DEFAULT_COALESCE_WINDOW,
        };
        // Seed the undo baseline at the loaded (generation-0) state so the first
        // edit's snapshot pushes the original as the first undo boundary (TR-010).
        doc.seed_undo();
        Ok(doc)
    }

    /// Create a fresh, empty untitled document with a workspace-assigned
    /// sequence number used for its `Untitled-N` title.
    #[must_use]
    pub fn new_untitled(seq: u32) -> Self {
        let buffer = String::new();
        let last_saved = SavedSnapshot::of(&buffer);
        let mut doc = Self {
            id: mint_doc_id(),
            buffer,
            path: None,
            last_saved,
            // A new buffer has no file bytes; default to LF, no BOM, no trailing
            // newline. `original_hash` is the hash of empty content.
            byte_profile: ByteFidelityProfile {
                line_ending: LineEnding::Lf,
                dominant: LineEnding::Lf,
                had_trailing_newline: false,
                had_bom: false,
                original_hash: hash_bytes(&[]),
            },
            cursor: CursorState::default(),
            parse: None,
            highlight: None,
            diagnostics: Vec::new(),
            untitled_seq: Some(seq),
            edit_generation: 0,
            last_requested_generation: 0,
            pending_cursor_jump: None,
            completion: CompletionState::new(),
            snippet_session: None,
            bound_type: None,
            binding: TypeBinding::none(),
            override_: None,
            validation_suppressed: false,
            recovery: crate::recovery::AutosaveDebounce::new(
                crate::settings::AutosaveConfig::default(),
            ),
            undo: UndoStack::new(),
            last_undo_generation: None,
            last_undo_instant: None,
            undo_coalesce_window: ron_core::undo::DEFAULT_COALESCE_WINDOW,
        };
        // Seed the undo baseline at the empty (generation-0) state.
        doc.seed_undo();
        doc
    }

    /// Reconstruct a document from a recently-closed record's fields (FR-012).
    ///
    /// Rebuilds a document with the closed buffer text, the saved baseline it had
    /// at close (so its [`dirty`](Self::dirty) state is reconstructed faithfully —
    /// a reopened-but-unsaved buffer comes back dirty), the carried original-on-load
    /// byte-fidelity profile (so a subsequent Save stays byte-preserving), and the
    /// restored cursor. Derived parse/highlight/diagnostic state starts empty; the
    /// caller requests a fresh parse after reopen. The edit generation is reset to
    /// the baseline so that fresh parse is requested exactly once.
    #[must_use]
    pub fn from_restorable(
        path: Option<PathBuf>,
        buffer: String,
        last_saved: SavedSnapshot,
        byte_profile: ByteFidelityProfile,
        cursor: CursorState,
        untitled_seq: Option<u32>,
    ) -> Self {
        let mut doc = Self {
            id: mint_doc_id(),
            buffer,
            path,
            last_saved,
            byte_profile,
            cursor,
            parse: None,
            highlight: None,
            diagnostics: Vec::new(),
            untitled_seq,
            edit_generation: 0,
            last_requested_generation: 0,
            pending_cursor_jump: None,
            completion: CompletionState::new(),
            snippet_session: None,
            bound_type: None,
            binding: TypeBinding::none(),
            override_: None,
            validation_suppressed: false,
            recovery: crate::recovery::AutosaveDebounce::new(
                crate::settings::AutosaveConfig::default(),
            ),
            undo: UndoStack::new(),
            last_undo_generation: None,
            last_undo_instant: None,
            undo_coalesce_window: ron_core::undo::DEFAULT_COALESCE_WINDOW,
        };
        // Seed the undo baseline at the restored (generation-0) state so the first
        // post-reopen edit pushes the restored content as the first undo boundary.
        doc.seed_undo();
        doc
    }

    /// The process-unique identity token for this document (FR-026).
    ///
    /// Stable for the document's lifetime; used to track a specific tab across
    /// index shifts during batch close/quit operations.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }

    /// `true` when the buffer differs from the last saved/loaded snapshot.
    #[must_use]
    pub fn dirty(&self) -> bool {
        !self.last_saved.matches(&self.buffer)
    }

    /// Re-baseline the saved snapshot to the current buffer (call after a save).
    pub fn mark_saved(&mut self) {
        self.last_saved = SavedSnapshot::of(&self.buffer);
    }

    // --- E007 OBJ2: autosave / crash-recovery lifecycle hooks ----------------

    /// Sync the autosave debounce with the live [`AutosaveConfig`](crate::settings::AutosaveConfig)
    /// (E007 TR-025/TR-026).
    ///
    /// The document is constructed with the default config; the shell calls this so
    /// the debounce honours the user's persisted (and clamped) idle interval /
    /// edit-count threshold. Cheap; safe to call every frame.
    pub fn set_autosave_config(&mut self, config: crate::settings::AutosaveConfig) {
        self.recovery.set_config(config);
    }

    /// Record that the buffer changed at `now`, keyed on the current
    /// [`edit_generation`](Self::edit_generation) (E007 TR-006).
    ///
    /// Idempotent per generation, so the shell may call it every frame: only a
    /// genuinely new generation resets the idle timer and advances the edit-count
    /// accumulator. This is the *only-when-changed* signal the debounce gates on.
    pub fn note_change(&mut self, now: std::time::Instant) {
        self.recovery.note_change(self.edit_generation, now);
    }

    /// The cheap per-frame check: should this document autosave its sidecar at `now`
    /// (E007 TR-006/TR-016)?
    ///
    /// Returns `true` only when the buffer changed since the last sidecar write AND
    /// an autosave trigger binds (idle OR edit-count). An **untitled** buffer (no
    /// `path`) is never autosaved (TR-017), so this is always `false` for it. Performs
    /// no I/O; the caller hands a snapshot to the off-frame writer when it fires.
    #[must_use]
    pub fn should_autosave(&self, now: std::time::Instant) -> bool {
        self.path.is_some() && self.recovery.poll(now)
    }

    /// The deterministic test/force hook (E007 TR-020): `true` when there is a
    /// changed buffer to autosave for a titled document, bypassing the thresholds.
    ///
    /// Honours the only-when-changed gate and the untitled-no-sidecar rule, so a
    /// forced tick on an unchanged or untitled document still writes nothing.
    #[must_use]
    pub fn force_autosave_tick(&self) -> bool {
        self.path.is_some() && self.recovery.force_tick()
    }

    /// Build the [`RecoverySidecar`](crate::recovery::RecoverySidecar) snapshot for
    /// this document, or `None` for an untitled buffer (no `path` → no sidecar,
    /// TR-017).
    ///
    /// Captures the live buffer + fidelity profile against the document's `path`. The
    /// shell hands this to the off-frame [`AutosaveWorker`](crate::recovery::AutosaveWorker).
    #[must_use]
    pub fn recovery_snapshot(&self) -> Option<crate::recovery::RecoverySidecar> {
        let path = self.path.clone()?;
        Some(crate::recovery::RecoverySidecar::new(
            path,
            self.buffer.clone(),
            &self.byte_profile,
        ))
    }

    /// Mark that a sidecar write for the current generation has been dispatched
    /// (E007 TR-006).
    ///
    /// Resets the debounce's edit-count accumulator and records the written
    /// generation so the next [`should_autosave`](Self::should_autosave) only fires
    /// after a *new* change — one write per debounce window, never per keystroke
    /// (SC-010).
    pub fn mark_autosaved(&mut self) {
        self.recovery.mark_written();
    }

    // --- E007 OBJ3: bounded CST-backed undo/redo (TR-010..014/018/024/027) ----

    /// Sync the undo stack with the live [`UndoConfig`](crate::settings::UndoConfig)
    /// (E007 TR-024/TR-026/TR-027).
    ///
    /// The document is constructed with the default undo config; the shell calls
    /// this so the stack honours the user's persisted (and clamped) history cap and
    /// coalesce window. Rebuilding the stack would lose history, so this updates the
    /// cap/window **in place** by reconstructing only when the config actually
    /// changed and otherwise leaving the existing history intact. Cheap; safe to
    /// call every frame.
    pub fn set_undo_config(&mut self, config: crate::settings::UndoConfig) {
        let cap = config.to_engine_cap();
        let window = config.effective_coalesce_window();
        // Only rebuild when the effective config changed, so calling this every
        // frame does not discard the accumulated history. A change in cap/window
        // is rare (settings edit), so dropping history then is acceptable and keeps
        // the bound authoritative.
        if self.undo.cap() != cap || self.undo_coalesce_window != window {
            self.undo = UndoStack::with_config(cap, window);
            self.undo_coalesce_window = window;
            // Re-seed from the current buffer so undo has a valid baseline after a
            // config-driven rebuild (no prior boundary; just the current state).
            self.last_undo_generation = None;
            self.last_undo_instant = None;
            self.seed_undo();
        }
    }

    /// Seed the undo stack with the document's current state as the baseline
    /// (E007 OBJ3 — TR-010).
    ///
    /// Records the current buffer + CST + cursor as the stack's `current` with no
    /// prior boundary (the first `record` just seeds). Idempotent per generation:
    /// only seeds when no snapshot has been recorded yet. Call once after a load /
    /// reopen / config rebuild so the first edit's snapshot has a baseline to push.
    pub fn seed_undo(&mut self) {
        if self.last_undo_generation.is_some() {
            return;
        }
        let entry = self.undo_entry_of_current();
        self.undo.record(entry, false);
        self.last_undo_generation = Some(self.edit_generation);
        // Leave `last_undo_instant` as `None` so the very first real edit starts a
        // fresh unit (no coalesce against the seed).
    }

    /// Build an [`UndoEntry`] snapshotting the document's current in-memory state.
    ///
    /// Parses the live buffer into a CST and captures the exact buffer bytes +
    /// cursor. The CST snapshot is structurally shared and cheap to retain
    /// (AD-002/ADR-0001); the parse cost is paid at most once per coalesce window
    /// (see [`record_undo_snapshot`](Self::record_undo_snapshot)), never per
    /// keystroke, so it stays off the per-frame hot path (TR-016/TR-023, SC-008).
    fn undo_entry_of_current(&self) -> UndoEntry<CursorState> {
        UndoEntry::new(
            ron_core::parse(&self.buffer),
            self.buffer.clone(),
            self.cursor,
        )
    }

    /// Record an undo snapshot for the document's current state at `now`, if the
    /// buffer advanced since the last snapshot (E007 OBJ3 — TR-010/TR-027, T035..T037).
    ///
    /// This is the **only** undo bookkeeping the shell drives, and it is coalesced
    /// off the per-keystroke path: it does work only when the live
    /// [`edit_generation`](Self::edit_generation) has advanced past the last
    /// recorded generation (a burst of edits in one frame snapshots once, the
    /// latest text). The coalesce *timing* decision is made here, caller-side
    /// (`ron-core` measures no clock, TR-014): the new edit **extends** the current
    /// undo unit when it falls within the configured coalesce window of the prior
    /// snapshot, and **starts a new unit** otherwise (TR-027). A new edit after an
    /// undo clears the redo stack inside `ron-core` (TR-012).
    ///
    /// Returns `true` when a snapshot was recorded (the buffer had advanced).
    pub fn record_undo_snapshot(&mut self, now: Instant) -> bool {
        // Seed the baseline lazily so the first edit always has a prior state.
        if self.last_undo_generation.is_none() {
            self.seed_undo();
        }
        if self.last_undo_generation == Some(self.edit_generation) {
            return false; // no new edit since the last snapshot
        }
        // Caller-side coalesce decision: within the window → extend the unit.
        let coalesce = self
            .last_undo_instant
            .is_some_and(|prev| now.duration_since(prev) < self.undo_coalesce_window);
        let entry = self.undo_entry_of_current();
        self.undo.record(entry, coalesce);
        self.last_undo_generation = Some(self.edit_generation);
        self.last_undo_instant = Some(now);
        true
    }

    /// Whether an undo step is currently available (E007 OBJ3).
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    /// Whether a redo step is currently available (E007 OBJ3).
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// Undo the last change, restoring the **exact prior in-memory bytes** + cursor
    /// (E007 OBJ3 — TR-010/TR-018, SC-005).
    ///
    /// Operates **solely** on the in-memory document: it replaces the buffer with
    /// the prior boundary's `source_text` byte-for-byte (no reflow), restores the
    /// cursor, and bumps [`edit_generation`](Self::edit_generation) so a reparse
    /// runs and dirty-tracking recomputes against the restored bytes. It NEVER
    /// reads or writes the on-disk file — undo of a buffer whose file changed on
    /// disk still restores the in-memory prior bytes (TR-018). Before stepping it
    /// flushes any open coalescing run by recording a pending snapshot, so the run
    /// in progress is itself undoable. Returns `true` when a step was taken.
    pub fn undo(&mut self, now: Instant) -> bool {
        // Flush any pending coalesced edits into the stack first so the current
        // run is a recoverable boundary before we step back.
        self.record_undo_snapshot(now);
        let Some(entry) = self.undo.undo() else {
            return false;
        };
        self.apply_restored(&entry);
        true
    }

    /// Redo the last undone change, replaying its exact bytes + cursor (E007 OBJ3 —
    /// TR-010/TR-018, SC-005).
    ///
    /// The inverse of [`undo`](Self::undo): replaces the buffer with the replayed
    /// state's exact bytes, restores the cursor, and bumps the edit generation so a
    /// reparse runs. In-memory only; never touches the file (TR-018). Returns `true`
    /// when a step was taken.
    pub fn redo(&mut self) -> bool {
        let Some(entry) = self.undo.redo() else {
            return false;
        };
        self.apply_restored(&entry);
        true
    }

    /// Apply a restored undo/redo entry to the live in-memory document state.
    ///
    /// Replaces the buffer with the entry's exact bytes, restores the cursor, and
    /// bumps the edit generation so the off-frame reparse re-runs against the
    /// restored text and dirty-tracking recomputes. The undo bookkeeping
    /// generation is synced to the post-restore generation so the restore itself is
    /// not re-snapshotted as a fresh edit (the stack already tracks it as
    /// `current`). The restore is byte-faithful — no normalization (TR-010).
    fn apply_restored(&mut self, entry: &UndoEntry<CursorState>) {
        self.buffer = entry.source_text().to_string();
        self.cursor = *entry.cursor();
        self.on_edit();
        // The restored state is already the stack's `current`; mark it recorded so
        // the next `record_undo_snapshot` does not push it back as a new edit.
        self.last_undo_generation = Some(self.edit_generation);
        // A restore is a discrete action: start a fresh coalesce unit after it by
        // clearing the timing anchor (the next real edit will not coalesce into it).
        self.last_undo_instant = None;
    }

    /// The number of committed undo boundaries currently retained (E007 OBJ3; for
    /// tests / host integration).
    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// The total retained undo+redo snapshot byte-size (E007 OBJ3 — SC-009; for
    /// tests asserting the bound is independent of file size).
    #[must_use]
    pub fn undo_total_bytes(&self) -> usize {
        self.undo.total_bytes()
    }

    /// `true` when the buffer is strictly larger than `threshold` bytes.
    ///
    /// The comparison is **strict** greater-than: a buffer whose length exactly
    /// equals the threshold is *not* oversize (boundary owned by FR for the
    /// large-file warning).
    #[must_use]
    pub fn oversize(&self, threshold: u64) -> bool {
        self.buffer.len() as u64 > threshold
    }

    /// The display title: the file name when saved, else a stable `Untitled-N`
    /// placeholder built from the workspace-assigned sequence number.
    #[must_use]
    pub fn title(&self) -> String {
        if let Some(path) = &self.path {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                return name.to_string();
            }
        }
        match self.untitled_seq {
            Some(n) => format!("Untitled-{n}"),
            // A document with neither a path nor a sequence number is degenerate;
            // fall back to a stable label rather than panicking.
            None => "Untitled".to_string(),
        }
    }

    /// The current edit generation (monotonic; bumped by [`on_edit`](Self::on_edit)).
    #[must_use]
    pub fn edit_generation(&self) -> u64 {
        self.edit_generation
    }

    /// Record that the buffer was mutated (FR-006).
    ///
    /// Bumps the monotonic [`edit_generation`](Self::edit_generation) so the next
    /// [`request_reparse`](Self::request_reparse) ships the latest text and any
    /// in-flight stale result is later discarded by generation comparison. This is
    /// the *only* per-frame edit hook; it never calls `ron_core::parse` directly.
    pub fn on_edit(&mut self) {
        self.edit_generation = self.edit_generation.wrapping_add(1);
    }

    /// Queue an off-thread reparse of the current buffer, coalesced (FR-006).
    ///
    /// Sends `(edit_generation, buffer.clone())` to the worker only when the edit
    /// generation has advanced past the last requested one — so a burst of
    /// keystrokes collapses to a single request for the newest text (only the
    /// latest generation matters). The per-frame UI path never parses inline; all
    /// parsing happens on the worker thread.
    ///
    /// Type validation degrades on E003's oversize signal exactly like highlighting
    /// and squiggles (E006 T040 — FR-015/FR-024): when
    /// [`validation_suppressed`](Self::validation_suppressed) is set (the document is
    /// oversize), **no** bound type is shipped to the worker, so it produces zero type
    /// diagnostics (structural-only, FR-015). The structural parse still runs, mirroring
    /// how an oversize document still parses but renders no squiggles. The shell
    /// reconciles the flag against the live buffer size every frame, so editing the
    /// document back below the threshold resumes validation on the next reparse.
    pub fn request_reparse(&mut self, worker: &ReparseWorker) {
        if self.edit_generation == self.last_requested_generation {
            return;
        }
        self.last_requested_generation = self.edit_generation;
        // Carry the active binding so the worker validates against it off-frame
        // (E006/FR-006). Cloning is cheap — the `TypeModel` is behind an `Arc`.
        // When validation is degraded for an oversize document, ship `None` so the
        // worker runs structural-only (no type diagnostics), consistent with E003
        // disabling highlighting/squiggles past the same threshold (T040).
        let bound = if self.validation_suppressed {
            None
        } else {
            self.bound_type.clone()
        };
        worker.request(self.edit_generation, self.buffer.clone(), bound);
    }

    /// Drain finished reparse results from `worker` and install the current one
    /// (FR-006, FR-019).
    ///
    /// Discards stale results (generation older than the current edit) and only
    /// acts on a result matching the live [`edit_generation`](Self::edit_generation).
    /// When a current result lands it (1) becomes the installed [`parse`](Self::parse),
    /// (2) rebuilds [`diagnostics`](Self::diagnostics) from BOTH the structural and
    /// type sets via [`merge_type_diagnostics`], and (3) recomputes the
    /// [`highlight`](Self::highlight) model from the CST. Old diagnostics and
    /// highlights are kept until a fresh result lands — never cleared on edit.
    /// Returns `true` if a result was installed (so the caller can repaint).
    ///
    /// The type set is **replaced** wholesale on each landed result while the
    /// structural set is recomputed and preserved (replace-not-merge for the type
    /// set, FR-006). Overlap dedup between the two sets is a later task (T031) — for
    /// now the merged view is simply structural-then-type concatenation.
    pub fn poll_parse(&mut self, worker: &ReparseWorker) -> bool {
        let mut installed = false;
        // Drain everything queued this frame; keep only the latest *current* one.
        while let Some(result) = worker.poll() {
            // Stale: an edit happened after this parse was requested. Discard it
            // but keep the last-good parse/diagnostics/highlight intact.
            if result.generation != self.edit_generation {
                continue;
            }
            let generation = result.generation;
            self.diagnostics = merge_type_diagnostics(&result, &self.buffer);
            self.highlight = Some(build_highlight_model(&result, generation));
            self.parse = Some(result);
            installed = true;
        }
        installed
    }

    /// The number of Unicode scalar values (characters) in the buffer.
    ///
    /// Cursor jumps and the editor's `TextEdit` cursor work in **character**
    /// offsets, so this is the inclusive upper bound for a valid caret position.
    #[must_use]
    pub fn char_len(&self) -> usize {
        self.buffer.chars().count()
    }

    /// Request the editor to move its caret to `char_offset` on the next frame
    /// (FR-009).
    ///
    /// Used by the Problems panel's click-to-navigate. The offset is stored as-is
    /// and clamped to the live buffer only when consumed by
    /// [`take_cursor_jump`](Self::take_cursor_jump), so a range that became stale
    /// after edits still resolves to the nearest valid caret position (best-effort,
    /// self-correcting once a fresh parse lands).
    pub fn request_cursor_jump(&mut self, char_offset: usize) {
        self.pending_cursor_jump = Some(char_offset);
    }

    /// Take the pending caret jump, if any, clamped to `[0, char_len]` (FR-009).
    ///
    /// Returns `None` when no jump is pending. A pending offset beyond the current
    /// buffer length is clamped down to the buffer's character length so a stale
    /// diagnostic range can never move the caret out of bounds. Consuming clears
    /// the pending jump so it applies exactly once.
    #[must_use]
    pub fn take_cursor_jump(&mut self) -> Option<usize> {
        self.pending_cursor_jump
            .take()
            .map(|offset| offset.min(self.char_len()))
    }

    /// `true` when a caret jump is queued for the next frame (for tests/hosts).
    #[must_use]
    pub fn has_pending_cursor_jump(&self) -> bool {
        self.pending_cursor_jump.is_some()
    }

    /// A short, human-readable label for the active binding, for the status
    /// indicator and tests (E006 US2 — FR-011).
    ///
    /// * [`BindingState::Bound`] → `Type: <name> (<origin>)` where `<origin>` is
    ///   `override` or `config`, e.g. `Type: Entity (config)` /
    ///   `Type: Entity (override)`.
    /// * [`BindingState::NoBinding`] → `no type bound`.
    ///
    /// The source locator is *not* in this short label (it can be a long path); the
    /// UI shows the source separately (see [`binding_source_label`](Self::binding_source_label)).
    /// When multiple config patterns matched, [`binding`](Self::binding) already
    /// holds the resolved (most-specific) one, so this reflects that single chosen
    /// binding (FR-011).
    #[must_use]
    pub fn binding_label(&self) -> String {
        match &self.binding.state {
            BindingState::Bound {
                type_name, origin, ..
            } => {
                let origin = match origin {
                    BindingOrigin::Override => "override",
                    BindingOrigin::Config => "config",
                };
                format!("Type: {type_name} ({origin})")
            }
            BindingState::NoBinding => "no type bound".to_string(),
        }
    }

    /// The bound type's source locator as a display string, or `None` when
    /// [`BindingState::NoBinding`] (E006 US2 — FR-011).
    ///
    /// Prefixes the path with its source kind so the user can tell a Rust source
    /// from a schema file, e.g. `schema: schemas/app.json` /
    /// `rust: src/scene.rs`. The full source locator is surfaced alongside the
    /// short [`binding_label`](Self::binding_label) so the active binding is fully
    /// visible (FR-011, data-model "active binding visible").
    #[must_use]
    pub fn binding_source_label(&self) -> Option<String> {
        match &self.binding.state {
            BindingState::Bound { type_source, .. } => {
                let (kind, path) = match type_source {
                    crate::binding::TypeSourceLocator::RustSource(p) => ("rust", p),
                    crate::binding::TypeSourceLocator::SchemaFile(p) => ("schema", p),
                };
                Some(format!("{kind}: {}", path.display()))
            }
            BindingState::NoBinding => None,
        }
    }
}

/// Build the merged editor-coordinate diagnostic view for a landed
/// [`ParseResult`] against `buffer` (E006/FR-006, FR-017).
///
/// The structural set (`result.diagnostics`) is published in full; the type set
/// (`result.type_diagnostics`) is first deduped against it via
/// [`ron_validate::dedup_against_structural`] so any type finding whose byte range
/// intersects a structural diagnostic is suppressed — structural always wins on
/// overlap (FR-017). The (kept) structural-then-type sets are then mapped into
/// [`DiagnosticView`]s (char + line/column) via [`map_diagnostic`]. Structural
/// findings come first so they take visual/list precedence; each is
/// distinguishable from the other by [`DiagnosticView::code`]'s
/// [`source`](ron_core::DiagnosticCode::source) tag (`"ron-core"` vs `"ron-types"`).
///
/// This is the single "replace, not merge" publish point for the type set: every
/// landed result recomputes the whole view, so no stale type finding survives
/// (FR-006). The structural set is NEVER dropped here — only overlapping TYPE
/// findings are (structural precedence, FR-017).
#[must_use]
pub fn merge_type_diagnostics(result: &ParseResult, buffer: &str) -> Vec<DiagnosticView> {
    // Suppress type findings that overlap a structural one (structural wins,
    // FR-017). The structural set is passed by reference and is never mutated.
    let type_diags = ron_validate::dedup_against_structural(
        result.type_diagnostics.clone(),
        &result.diagnostics,
    );
    result
        .diagnostics
        .iter()
        .chain(type_diags.iter())
        .map(|d| map_diagnostic(d, buffer))
        .collect()
}
