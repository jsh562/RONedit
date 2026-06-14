//! Filesystem entry points for opening RON files (FR-018).
//!
//! [`open_path`] reads a file's raw bytes, validates UTF-8 at the boundary, and
//! builds an [`EditorDocument`]. Non-UTF-8 input is rejected cleanly with
//! [`OpenError::NotUtf8`] — **no document is created** — honouring "never corrupt
//! user data" (project-instructions §I): the editor refuses to silently lossy-
//! decode a binary or non-UTF-8 file.
//!
//! # Atomic save (E007 / OBJ1)
//!
//! The Save path is the crash-safe persistence contract from project-instructions
//! §I — **atomic save**: serialize through the byte-fidelity re-emission
//! ([`save_bytes`]), write to a temp file in the **same directory** as the target,
//! flush it durably, then atomically replace the target ([`save_atomic`], TR-001/
//! TR-002). The original file is never modified until the replace commits, so any
//! failure (disk full, permission denied, partial write, crash) leaves it
//! byte-identical and the failure is surfaced ([`SaveError`], TR-003). Sidecar
//! crash recovery and undo/redo land in later E007 objectives.

use std::path::Path;

use atomicwrites::{AtomicFile, Error as AtomicError, OverwriteBehavior};

use crate::document::{ByteFidelityProfile, EditorDocument, LineEnding};

/// Why opening a file failed (FR-018).
///
/// All variants are error-severity. `NotUtf8` means the bytes were read but are
/// not valid UTF-8; `Io` wraps any filesystem read failure (missing file,
/// permission denied, etc.).
#[derive(Debug)]
#[non_exhaustive]
pub enum OpenError {
    /// The file's bytes are not valid UTF-8; no document was created.
    NotUtf8,
    /// A filesystem read error occurred.
    Io(std::io::Error),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::NotUtf8 => f.write_str("not valid UTF-8"),
            OpenError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OpenError::NotUtf8 => None,
            OpenError::Io(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for OpenError {
    fn from(e: std::io::Error) -> Self {
        OpenError::Io(e)
    }
}

/// Read `path` and build an [`EditorDocument`], rejecting non-UTF-8 (FR-018).
///
/// # Errors
///
/// * [`OpenError::Io`] if the file cannot be read.
/// * [`OpenError::NotUtf8`] if the bytes are not valid UTF-8 (no document is
///   created in this case).
pub fn open_path(path: &Path) -> Result<EditorDocument, OpenError> {
    let raw = std::fs::read(path)?;

    // Validate UTF-8 at the boundary using `ron-core`'s validator; reject cleanly
    // without constructing a document if the bytes are not valid UTF-8.
    if ron_core::validate_utf8(&raw).is_err() {
        return Err(OpenError::NotUtf8);
    }

    // UTF-8 is confirmed; `from_loaded` re-decodes (infallibly here) and captures
    // the byte-fidelity profile.
    EditorDocument::from_loaded(path, &raw).map_err(|_| OpenError::NotUtf8)
}

/// The UTF-8 BOM byte sequence (`EF BB BF`).
const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Why an atomic save failed (TR-003, E007/OBJ1).
///
/// Every variant carries the same hard guarantee: **the original target file is
/// byte-identical to before the save** (project-instructions §I). The atomic
/// pipeline ([`save_atomic`]) writes a same-directory temp file and only ever
/// touches the target through the atomic replace primitive, so a failure at any
/// stage leaves the original untouched and the buffer dirty (no silent success).
///
/// The variants name the atomic-save failure surface required by TR-003: disk
/// full, permission denied, a partial/interrupted temp write, a failed atomic
/// replace, and the same-filesystem-impossible degrade-and-surface case (TR-005).
/// The set is `#[non_exhaustive]` so later save modes can be added without a
/// breaking change. Every variant wraps the underlying [`std::io::Error`] so the
/// original OS detail is preserved for the user-facing notice and `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum SaveError {
    /// The target's filesystem is full (no space left to write the temp file).
    DiskFull(std::io::Error),
    /// The target (or its directory) denied write/replace permission.
    PermissionDenied(std::io::Error),
    /// The temp-file write was interrupted before it was fully written; the
    /// target was never touched, so the original is intact.
    PartialWrite(std::io::Error),
    /// The temp file was written and flushed, but the atomic replace of the
    /// target failed; the original target still holds its pre-save bytes.
    ReplaceFailed(std::io::Error),
    /// The same-filesystem temp could not be established (e.g. the parent
    /// directory is unwritable / missing), so an atomic replace cannot be
    /// performed; surfaced rather than falling back to a non-atomic write
    /// (TR-005). The original target, if any, is untouched.
    SameFilesystemImpossible(std::io::Error),
    /// Any other filesystem write failure; the on-disk file may be unchanged.
    Io(std::io::Error),
}

impl SaveError {
    /// The underlying I/O error this save failure wraps.
    #[must_use]
    pub fn io(&self) -> &std::io::Error {
        match self {
            SaveError::DiskFull(e)
            | SaveError::PermissionDenied(e)
            | SaveError::PartialWrite(e)
            | SaveError::ReplaceFailed(e)
            | SaveError::SameFilesystemImpossible(e)
            | SaveError::Io(e) => e,
        }
    }

    /// Classify a raw [`std::io::Error`] from the atomic pipeline into the most
    /// specific [`SaveError`] variant (disk-full / permission / partial / generic).
    ///
    /// `stage` records whether the error came from establishing or writing the
    /// same-directory temp file ([`Stage::Temp`]) or from the atomic replace of
    /// the target ([`Stage::Replace`]), so a permission/space failure is reported
    /// with the right replace-vs-write framing while the original-intact guarantee
    /// holds either way.
    fn from_io(e: std::io::Error, stage: Stage) -> Self {
        use std::io::ErrorKind;
        // Disk-full has its own ErrorKind on recent toolchains; older kernels may
        // surface it via the raw errno (ENOSPC = 28), so check both.
        let is_disk_full = matches!(e.kind(), ErrorKind::StorageFull)
            || e.raw_os_error() == Some(28)
            // Windows: ERROR_DISK_FULL (112), ERROR_HANDLE_DISK_FULL (39).
            || matches!(e.raw_os_error(), Some(112) | Some(39));
        if is_disk_full {
            return SaveError::DiskFull(e);
        }
        if e.kind() == ErrorKind::PermissionDenied {
            return SaveError::PermissionDenied(e);
        }
        match stage {
            // A temp-write failure that is neither disk-full nor permission is an
            // interrupted/partial temp write — the target is still untouched.
            Stage::Temp => SaveError::PartialWrite(e),
            // A replace-stage failure means the temp was written but the atomic
            // swap did not commit; the original target keeps its pre-save bytes.
            Stage::Replace => SaveError::ReplaceFailed(e),
        }
    }
}

/// Which stage of the atomic pipeline an I/O error came from, used to frame the
/// resulting [`SaveError`] (temp-write vs. atomic replace).
#[derive(Debug, Clone, Copy)]
enum Stage {
    /// Establishing or writing the same-directory temp file.
    Temp,
    /// The atomic replace of the target.
    Replace,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::DiskFull(e) => write!(f, "disk full: {e}"),
            SaveError::PermissionDenied(e) => write!(f, "permission denied: {e}"),
            SaveError::PartialWrite(e) => write!(f, "write interrupted: {e}"),
            SaveError::ReplaceFailed(e) => write!(f, "atomic replace failed: {e}"),
            SaveError::SameFilesystemImpossible(e) => {
                write!(f, "atomic save not possible at this location: {e}")
            }
            SaveError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.io())
    }
}

impl From<std::io::Error> for SaveError {
    fn from(e: std::io::Error) -> Self {
        SaveError::Io(e)
    }
}

/// Re-emit `buffer` to raw bytes per the load-time fidelity `profile` (FR-020/FR-023).
///
/// The editor's `TextEdit` normalises every line ending in the live buffer to a
/// single `\n`. To honour "never corrupt user data" (project-instructions §I) on
/// save, this re-applies the file's original byte fidelity:
///
/// * **Line endings.** For a uniform file the original style is re-emitted
///   verbatim (`\n` → `\r\n` for a CRLF file; left as `\n` for an LF file). For a
///   genuinely `Mixed` file the [`dominant`](ByteFidelityProfile::dominant) style
///   is re-emitted (ties resolve to LF), so a mixed input is normalised to a
///   single, predictable convention — the documented, intended limitation of
///   FR-020/FR-023 (a mixed file does **not** round-trip byte-for-byte).
/// * **Trailing newline.** Re-applied iff the original ended in one; if the
///   buffer already ends in a newline but the original did not, the trailing
///   newline is dropped, and vice versa.
/// * **BOM.** A leading UTF-8 BOM is re-emitted iff the original carried one.
///
/// The output is always valid UTF-8.
#[must_use]
pub fn save_bytes(buffer: &str, profile: &ByteFidelityProfile) -> Vec<u8> {
    // Choose the concrete EOL to emit. Uniform files keep their style; a Mixed
    // file normalises to `dominant` (which is never `Mixed`; ties → LF).
    let emit_crlf = match profile.line_ending {
        LineEnding::Crlf => true,
        LineEnding::Lf => false,
        LineEnding::Mixed => matches!(profile.dominant, LineEnding::Crlf),
    };

    // Normalise the buffer to bare `\n` first (defensive: the widget already does
    // this, but a stray `\r\n` in the buffer must not become `\r\r\n`), then
    // re-emit each `\n` in the chosen style.
    let normalised = normalise_to_lf(buffer);

    // Decide the trailing newline: strip any the buffer carries, then re-add iff
    // the original had one. This makes "no trailing newline" round-trip too.
    let core = normalised.strip_suffix('\n').unwrap_or(&normalised);
    let mut emitted = if emit_crlf {
        core.replace('\n', "\r\n")
    } else {
        core.to_string()
    };
    if profile.had_trailing_newline {
        emitted.push_str(if emit_crlf { "\r\n" } else { "\n" });
    }

    let mut out = Vec::with_capacity(emitted.len() + if profile.had_bom { BOM.len() } else { 0 });
    if profile.had_bom {
        out.extend_from_slice(&BOM);
    }
    out.extend_from_slice(emitted.as_bytes());
    out
}

/// Normalise any CRLF/CR in `s` to bare LF so re-emission starts from one style.
fn normalise_to_lf(s: &str) -> String {
    // First collapse CRLF, then any remaining lone CR (defensive; RON sources are
    // LF/CRLF in practice). Avoids `\r\r\n` artefacts on re-emit.
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Atomically save `buffer` to `path`, preserving load-time byte fidelity
/// (E007/OBJ1 — TR-001/TR-002/TR-004; **[COMPLETES TR-002]**).
///
/// The crash-safe save pipeline, the seam project-instructions §I's "never corrupt
/// user data" mandate is built on:
///
/// 1. **Serialize** the buffer through [`save_bytes`] with the load-time
///    [`ByteFidelityProfile`], so the original line-ending style, UTF-8 BOM, and
///    trailing-newline presence are re-emitted byte-for-byte (TR-004). The atomic
///    path never bypasses this re-emission, so it does not regress E003's fidelity.
/// 2. **Temp-write + durable flush + atomic replace** via [`atomicwrites`]: the
///    crate writes a temp file in a randomized subdirectory of the **target's own
///    directory** (so the temp is always on the *same filesystem* as the target —
///    TR-005 — and a cross-filesystem, non-atomic replace can never happen
///    silently), `fsync`s the temp file, then atomically replaces the target. On
///    Windows it uses `MoveFileExW` with `MOVEFILE_WRITE_THROUGH |
///    MOVEFILE_REPLACE_EXISTING` (the platform replace-over-existing primitive,
///    TR-002, AD-001); on POSIX it `renameat`s and `fsync`s the parent
///    directory/-ies (file + directory durable flush, AD-006).
///
/// **Original-intact guarantee (TR-001/TR-003/TR-019).** The original target is
/// never opened for writing; it is only ever swapped by the atomic replace. So if
/// any step fails — disk full, permission denied, an interrupted temp write, or a
/// failed replace — the original is byte-identical to before the call and a
/// [`SaveError`] is returned (the caller keeps the buffer dirty; no silent
/// success). A residual temp file lives in a `.atomicwrite*` subdirectory of the
/// target's directory, never *at* the target path, and is a cleanable non-target
/// artifact (TR-019b).
///
/// **Durable-flush policy (TR-028, AD-006).** This is the *explicit-save* path and
/// it performs the full durable flush (file `fsync` on all platforms; parent
/// directory `fsync` on POSIX; on Windows the `MOVEFILE_WRITE_THROUGH` replace
/// primitive provides the ordering/durability guarantee — there is no directory
/// `fsync` on Windows). It is **not** on the per-keystroke edit path, so there is
/// no per-keystroke `fsync` (the autosave sidecar's reduced-flush path is OBJ2).
///
/// **Same-filesystem constraint (TR-005, T012).** The temp stays in the target's
/// directory, so the same-filesystem requirement holds by construction. A location
/// where the atomic replace cannot hold (e.g. an unwritable/missing parent
/// directory) surfaces as a [`SaveError`] — there is no silent non-atomic
/// `std::fs::write` fallback.
///
/// **Local-only (TR-015).** This path touches only the local filesystem (the
/// [`atomicwrites`] crate and `std::fs`); it introduces no network or transport
/// dependency. The `network_audit` regression test (`tests/offline_logging.rs`)
/// and `cargo deny` guard the dependency graph.
///
/// # Downstream reuse (E005 / E008 — TR-013)
///
/// This is the single, reusable save seam the downstream editing epics build on:
/// **E005** (smart authoring — e.g. format-on-save and other transform-on-write
/// flows) and **E008** (structural / table editing). Call it with the edited
/// buffer plus the document's load-time [`ByteFidelityProfile`] to persist any
/// transformed text *without* re-deriving the atomic / fidelity / durability
/// guarantees. Two contracts those epics may rely on:
///
/// * **Byte-fidelity is preserved by default.** `save_atomic` re-emits through
///   [`save_bytes`] (EOL style, BOM, trailing-newline) and applies **no**
///   reformatting of its own. A transform-on-write feature (E005) supplies the
///   already-transformed `buffer`; the save path stays byte-faithful and never
///   silently rewrites bytes the caller did not change.
/// * **Original-intact-until-commit.** The target is only ever swapped by the
///   atomic replace, so a failed save leaves the user's file byte-identical and
///   returns a [`SaveError`] — downstream callers keep their buffer dirty and
///   surface the error rather than assuming success (no optimistic re-baseline).
///
/// # Errors
///
/// Returns a [`SaveError`] (with the original target intact) when the temp file
/// cannot be created/written (disk full, permission denied, interrupted), when the
/// same-filesystem temp cannot be established (TR-005), or when the atomic replace
/// fails. The variant names the failure surface (TR-003).
pub fn save_atomic(
    buffer: &str,
    profile: &ByteFidelityProfile,
    path: &Path,
) -> Result<(), SaveError> {
    // TR-004 / HINT-003: serialize through the byte-fidelity re-emission so CRLF,
    // BOM, and trailing-newline fidelity survive the atomic path unchanged.
    let bytes = save_bytes(buffer, profile);

    // TR-005 / T012: `AtomicFile::new` keeps the temp in the target's own
    // directory (same filesystem). If the target has no parent directory we cannot
    // establish a same-directory temp, so surface it rather than silently degrade.
    if path.parent().is_none() {
        return Err(SaveError::SameFilesystemImpossible(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path has no parent directory; cannot place a same-filesystem temp",
        )));
    }

    // Temp-write (same-dir) → durable flush → atomic replace (TR-001/TR-002).
    let af = AtomicFile::new(path, OverwriteBehavior::AllowOverwrite);
    let result: Result<(), AtomicError<std::io::Error>> =
        af.write(|f| std::io::Write::write_all(f, &bytes));

    match result {
        Ok(()) => Ok(()),
        Err(AtomicError::User(e)) => {
            // The user callback (`write_all` into the temp) failed: the target was
            // never touched — this is a temp-write / partial-write failure.
            Err(SaveError::from_io(e, Stage::Temp))
        }
        Err(AtomicError::Internal(e)) => {
            // Library-internal: either creating the same-dir temp subdir/file, or
            // the atomic move into place. We cannot distinguish the two from the
            // error alone, so classify by errno (disk-full / permission) and frame
            // the residual category as a replace failure — in every case the
            // ORIGINAL target is intact (it is only ever swapped atomically).
            Err(SaveError::from_io(e, Stage::Replace))
        }
    }
}

/// Atomically write `doc` to `path`, re-emitting the load-time byte fidelity
/// (E007/OBJ1 — TR-001; **[COMPLETES TR-001 via T018]**).
///
/// Routes through [`save_atomic`]: serialize via [`save_bytes`] (so EOL style, BOM,
/// and trailing-newline presence are honoured — TR-004), then write atomically
/// (same-directory temp → durable flush → atomic replace). Success is reported
/// only after the durable atomic replace commits; on any failure the original file
/// is byte-identical and a [`SaveError`] is returned (the caller keeps the buffer
/// dirty — TR-003). This is the explicit-save durable-flush path (TR-028); it is
/// not on the per-keystroke edit path.
///
/// The untitled / Save-As path uses this same function once a target path is
/// chosen, so it follows the identical atomic, byte-faithful contract (TR-017).
///
/// # Errors
///
/// Returns a [`SaveError`] (original target intact) if the atomic save cannot be
/// committed; see [`save_atomic`].
pub fn save_document(doc: &EditorDocument, path: &Path) -> Result<(), SaveError> {
    save_atomic(&doc.buffer, &doc.byte_profile, path)
}
