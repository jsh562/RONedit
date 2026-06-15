//! Panel layout seams for the editor shell.
//!
//! This module is the *single place* later epics extend the shell's side/bottom
//! panels **without** editing shell-core code. It exposes:
//!
//! * an **active** diagnostics-panel region ([`render_diagnostics_seam`]);
//! * the **active** structural **table** host ([`render_table_seam`]) — the E008
//!   virtualized spreadsheet view, wired into the per-document view switcher's
//!   Table arm (US2 / T035); and
//! * one **reserved** seam rendered as a labeled, disabled placeholder:
//!   [`mode_selector_seam_stub`] (reserved for **E009** — the Bevy mode selector).
//!   The legacy [`tree_table_seam_stub`] placeholder remains for layout/host
//!   discoverability; the live table now renders through [`render_table_seam`].
//!
//! The reserved seams render a faint "coming soon" placeholder rather than being
//! empty or a `// TODO`, so the layout is visible and the integration point is
//! discoverable in the running app.
//!
//! # Deferred scope (E008 / E009)
//!
//! The two reserved seams here are deliberate, named hand-offs:
//!
//! * [`tree_table_seam_stub`] reserves the structural **tree / virtualized table**
//!   views — deferred to **E008**.
//! * [`mode_selector_seam_stub`] reserves the **Bevy mode** selector — deferred to
//!   **E009**.
//!
//! Those epics populate these seams without editing shell-core layout.

use crate::diagnostics_map::DiagnosticView;
use crate::document::EditorDocument;
use crate::editor_view::render_binding_indicator;
use crate::reparse::ReparseWorker;
use crate::structural::table::render_table_view;

/// Host the structural **spreadsheet/table** view for `doc` (E008 / US2 — T035,
/// [COMPLETES FR-005]).
///
/// Renders the always-visible active-binding indicator (FR-011) above the grid, so
/// type-awareness stays perceivable in every view, then the virtualized table
/// surface ([`render_table_view`]) where the user edits scalar cells inline, adds /
/// removes rows, and drills a nested cell into the tree/form surface — each routed
/// through the one-undo-unit structural-edit pipeline (FR-013/FR-014).
///
/// This is the [COMPLETES FR-005] host point: it replaces the Phase-1b table
/// placeholder pane so the table view is wired into the per-document view switcher's
/// Table arm (FR-017). The `worker` is the document's off-frame reparse worker, used
/// to re-derive the projection after an edit lands.
pub fn render_table_seam(ui: &mut egui::Ui, doc: &mut EditorDocument, worker: &ReparseWorker) {
    render_binding_indicator(ui, doc);
    render_table_view(ui, doc, worker);
}

/// Render the active diagnostics-panel region.
///
/// Lists the supplied [`DiagnosticView`]s (already projected into editor
/// coordinates) one per row: severity, code, `line:column`, and message. An
/// empty list shows a faint "No problems" state. This is the live seam — later
/// waves replace the row rendering with a richer, navigable problems panel.
pub fn render_diagnostics_seam(ui: &mut egui::Ui, diagnostics: &[DiagnosticView]) {
    if diagnostics.is_empty() {
        ui.weak("No problems");
        return;
    }
    for d in diagnostics {
        let (line, col) = d.line_col.0;
        // Lines/columns are zero-based internally; present them one-based.
        ui.label(format!(
            "{} {} [{}:{}] {}",
            d.severity,
            d.code,
            line + 1,
            col + 1,
            d.message
        ));
    }
}

/// Reserved seam for the **E008** structural tree/table views.
///
/// Renders a faint, disabled placeholder. Replace the body in E008 to mount the
/// tree/table widgets here without touching shell-core layout.
pub fn tree_table_seam_stub(ui: &mut egui::Ui) {
    ui.add_enabled_ui(false, |ui| {
        ui.weak("Structure (coming soon)");
    });
}

/// Reserved seam for the **E009** Bevy mode selector.
///
/// Renders a faint, disabled placeholder. Replace the body in E009 to mount the
/// mode selector here without touching shell-core layout.
pub fn mode_selector_seam_stub(ui: &mut egui::Ui) {
    ui.add_enabled_ui(false, |ui| {
        ui.weak("Mode (coming soon)");
    });
}
