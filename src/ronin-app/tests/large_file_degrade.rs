//! Large-file degrade reuse for E005 authoring (Wave 5, T041 — COMPLETES FR-026).
//!
//! Past E003's `large_file_threshold` the always-on E005 intelligence (the
//! structural completion popup) must degrade on the **same** signal E003 already
//! uses for highlighting / squiggles — the document being `oversize` — and reuse
//! E003's existing non-blocking degrade indicator + wording (no separate E005
//! message). The explicit Format commands are one-shot, verify-before-replace
//! actions, not per-frame intelligence, so they stay available on an oversize
//! document (the E003-consistent choice: degrade the always-on layer, not on-demand
//! commands).
//!
//! Per the E003 test boundary, full-frame rendering is manual/QC; the gating
//! *decision* ([`completion_enabled`]) and the Format-command behavior on an
//! oversize document are exercised here headlessly.

use ronin_app::app::{App, NoticeKind};
use ronin_app::document::EditorDocument;
use ronin_app::editor_view::completion_enabled;
use ronin_app::settings::AppSettings;

// ---- completion is gated on the E003 oversize signal (FR-026) ----------------

#[test]
fn completion_suppressed_when_oversize() {
    // Oversize ⇒ completion off (mirrors highlighting/squiggles suppression).
    assert!(
        !completion_enabled(true, false),
        "completion must be suppressed on an oversize file (E003 degrade reuse)"
    );
    // Not oversize ⇒ completion on (when no snippet session is active).
    assert!(
        completion_enabled(false, false),
        "completion runs normally below the large-file threshold"
    );
}

#[test]
fn completion_also_suppressed_during_snippet_session() {
    // While a snippet tab-stop session is active, completion is off so `Tab`
    // unambiguously drives snippet navigation — independent of oversize.
    assert!(!completion_enabled(false, true));
    assert!(!completion_enabled(true, true));
}

#[test]
fn oversize_decision_matches_e003_threshold() {
    // The gating signal is exactly `EditorDocument::oversize(threshold)` — the same
    // predicate E003 uses for highlighting/squiggles. A buffer past the threshold is
    // oversize (strict greater-than), so completion is gated off for it.
    let mut doc = EditorDocument::new_untitled(1);
    doc.buffer = "x".repeat(1_000);
    let threshold = 100u64;
    assert!(doc.oversize(threshold), "1000 bytes > 100 threshold");
    assert!(
        !completion_enabled(doc.oversize(threshold), doc.snippet_session.is_some()),
        "an oversize document gates completion off via the E003 signal"
    );

    // A small buffer under the threshold is not oversize, so completion runs.
    doc.buffer = "x".repeat(10);
    assert!(!doc.oversize(threshold));
    assert!(completion_enabled(
        doc.oversize(threshold),
        doc.snippet_session.is_some()
    ));
}

// ---- Format commands stay available on an oversize document (FR-026) ---------

/// The smallest effective large-file threshold (64 KiB; the app floors any lower
/// configured value to this). A test buffer must exceed it to be `oversize`.
fn min_threshold() -> u64 {
    AppSettings::min_large_file_threshold()
}

/// Settings whose large-file threshold is the floored minimum (64 KiB), so a
/// modestly-sized test buffer is enough to push a document `oversize`.
fn min_threshold_settings() -> AppSettings {
    let mut s = AppSettings::default();
    // Any value below the minimum is floored to it by the settings layer.
    s.set_large_file_threshold(1);
    s
}

/// A valid-but-messy RON list whose byte length exceeds `min`, so the document is
/// `oversize` once opened. The list is `[1,2,3,1,2,3,...]` with no canonical
/// spacing, so the formatter has work to do.
fn oversize_valid_buffer() -> String {
    // Each "1,2,3," chunk is 6 bytes; ~12k chunks comfortably exceed 64 KiB.
    let repeats = (min_threshold() as usize / 6) + 100;
    let mut s = String::with_capacity(repeats * 6 + 2);
    s.push('[');
    for _ in 0..repeats {
        s.push_str("1,2,3,");
    }
    s.push(']');
    s
}

#[test]
fn format_document_still_works_on_oversize_doc() {
    let mut app = App::new(min_threshold_settings(), None);
    app.new_untitled();

    let messy = oversize_valid_buffer();
    if let Some(doc) = app.active_document_mut() {
        doc.buffer = messy.clone();
    }
    let threshold = app.large_file_threshold();
    assert!(
        app.active_document().unwrap().oversize(threshold),
        "the test buffer must be oversize ({} bytes > {threshold})",
        messy.len()
    );

    // Format is an explicit command; it still reformats an oversize document.
    app.format_document();
    let after = app.active_document().unwrap().buffer.clone();
    assert_ne!(
        after, messy,
        "format must still run on an oversize document"
    );
    assert!(
        after.contains("1, 2, 3"),
        "format produced canonical spacing on the oversize buffer (head: {:?})",
        &after[..after.char_indices().nth(40).map_or(after.len(), |(b, _)| b)]
    );
    assert!(
        app.notices().iter().all(|n| n.kind != NoticeKind::Error),
        "a successful format on an oversize doc must not raise an error notice"
    );
}

#[test]
fn format_on_oversize_invalid_doc_is_byte_unchanged_and_errors() {
    // Even oversize, an invalid buffer is left byte-unchanged by the formatter's
    // no-op-on-failure path, with the standard format-skip error notice — the
    // oversize state changes nothing about that contract.
    let mut app = App::new(min_threshold_settings(), None);
    app.new_untitled();
    // An unterminated list (invalid) larger than the threshold.
    let repeats = (min_threshold() as usize / 2) + 100;
    let invalid = format!("[{}", "1,".repeat(repeats));
    if let Some(doc) = app.active_document_mut() {
        doc.buffer = invalid.clone();
    }
    let threshold = app.large_file_threshold();
    assert!(app.active_document().unwrap().oversize(threshold));

    app.format_document();
    assert_eq!(
        app.active_document().unwrap().buffer,
        invalid,
        "invalid oversize buffer must be byte-unchanged"
    );
    assert!(
        app.notices().iter().any(|n| n.kind == NoticeKind::Error),
        "a format no-op surfaces a persist-until-dismissed error notice"
    );
}
