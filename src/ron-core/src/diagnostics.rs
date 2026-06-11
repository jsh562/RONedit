//! The structured-diagnostic model for error-tolerant parsing (OBJ2).
//!
//! A [`Diagnostic`] records a single recovery decision the parser made while
//! building a lossless tree over malformed or incomplete input (TR-005). It
//! never alters the tree's byte coverage — diagnostics are a parallel,
//! side-channel report (INV-3): removing every diagnostic leaves the round-trip
//! identity untouched.
//!
//! # Stable public contract (AD-003 / TR-013)
//!
//! [`Severity`] and [`DiagnosticCode`] are part of `ron-core`'s 0.x public API.
//! Each code is a stable, namespaced `RON-Pxxxx` string (the `P` namespace is
//! reserved for *parse/recovery* diagnostics; later epics may add other
//! namespaces such as validation). Codes and their severities MUST NOT be
//! renumbered or repurposed; new recovery situations get new codes appended to
//! the registry.
//!
//! # One diagnostic per recovery point (TR-013)
//!
//! The parser emits exactly one [`Diagnostic`] per distinct recovery point, with
//! a precise source byte [`TextRange`] inside `[0, source_len)` (TR-006). The
//! range identifies the offending span (the unexpected token, the unclosed
//! delimiter's open bracket, or the construct that breached the depth guard).

use crate::syntax::TextRange;

/// Fixed severity classification for a [`Diagnostic`] (TR-013).
///
/// Part of the stable public API: the set of variants is closed and their
/// meaning does not change across 0.x releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// A recovery was required: the input is malformed or incomplete at this
    /// span. The tree still covers all input via `Error`/missing nodes.
    Error,
    /// A non-fatal concern: the input parsed, but something is suspect. Reserved
    /// for future lints; the OBJ2 recovery parser emits only [`Severity::Error`].
    Warning,
}

impl Severity {
    /// The stable lowercase label for this severity (`"error"` / `"warning"`).
    #[inline]
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stable, namespaced diagnostic code from the `RON-Pxxxx` parse registry
/// (AD-003 / TR-013).
///
/// Every variant maps 1:1 to a fixed `RON-Pxxxx` string via [`DiagnosticCode::code`].
/// The enum is `#[non_exhaustive]` so new recovery codes can be appended without
/// a breaking change, but **existing** variants, their code strings, and their
/// default severities are stable across 0.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DiagnosticCode {
    /// `RON-P0001` — an unexpected token at a position where a value (or other
    /// construct) was expected; the token was wrapped in an `Error` node.
    UnexpectedToken,
    /// `RON-P0002` — a delimiter (`(`, `[`, `{`) was opened but never closed
    /// before end-of-input; the matching close was synthesized as missing.
    UnclosedDelimiter,
    /// `RON-P0003` — nesting/recursion depth exceeded the configured guard
    /// (default 128); descent stopped and the remaining bytes were tokenized
    /// into `Error` nodes (no stack overflow, INV-5).
    NestingDepthExceeded,
    /// `RON-P0004` — a `:` separator was expected in a struct field or map
    /// entry but was absent; recovery continued with a missing separator.
    MissingSeparator,
    /// `RON-P0005` — a struct field or map entry was missing its value after a
    /// separator; an empty/missing value node was recorded.
    MissingValue,
}

impl DiagnosticCode {
    /// The stable `RON-Pxxxx` string for this code (part of the public API).
    #[inline]
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            DiagnosticCode::UnexpectedToken => "RON-P0001",
            DiagnosticCode::UnclosedDelimiter => "RON-P0002",
            DiagnosticCode::NestingDepthExceeded => "RON-P0003",
            DiagnosticCode::MissingSeparator => "RON-P0004",
            DiagnosticCode::MissingValue => "RON-P0005",
        }
    }

    /// The default [`Severity`] for this code. All current parse-recovery codes
    /// are [`Severity::Error`]; the mapping is part of the stable contract.
    #[inline]
    #[must_use]
    pub fn default_severity(self) -> Severity {
        match self {
            DiagnosticCode::UnexpectedToken
            | DiagnosticCode::UnclosedDelimiter
            | DiagnosticCode::NestingDepthExceeded
            | DiagnosticCode::MissingSeparator
            | DiagnosticCode::MissingValue => Severity::Error,
        }
    }
}

impl std::fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}

/// A single structured diagnostic produced during error-tolerant parsing.
///
/// Carries a precise source byte [`TextRange`] (TR-006), a human-readable
/// `message`, a [`Severity`], and a stable [`DiagnosticCode`] (TR-013). One
/// `Diagnostic` is emitted per recovery point; diagnostics never change the
/// tree's byte coverage (INV-3).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    /// Byte range the diagnostic refers to (a sub-range of `[0, source_len)`).
    pub range: TextRange,
    /// Human-readable description of the recovery.
    pub message: String,
    /// Fixed severity classification.
    pub severity: Severity,
    /// Stable namespaced `RON-Pxxxx` code.
    pub code: DiagnosticCode,
}

impl Diagnostic {
    /// Construct a diagnostic with the [`DiagnosticCode`]'s default severity.
    #[inline]
    #[must_use]
    pub fn new(code: DiagnosticCode, range: TextRange, message: impl Into<String>) -> Self {
        Self {
            range,
            message: message.into(),
            severity: code.default_severity(),
            code,
        }
    }

    /// This diagnostic's stable [`DiagnosticCode`].
    #[inline]
    #[must_use]
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// This diagnostic's [`Severity`].
    #[inline]
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// This diagnostic's source byte [`TextRange`].
    #[inline]
    #[must_use]
    pub fn range(&self) -> TextRange {
        self.range
    }

    /// This diagnostic's human-readable message.
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TR-013: the two-variant severity enum is fixed and its labels stable.
    #[test]
    fn severity_values_are_stable() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Error.to_string(), "error");
        // Ord is well-defined (Error < Warning by declaration order).
        assert!(Severity::Error < Severity::Warning);
    }

    /// AD-003/TR-013: every registry code maps to a stable `RON-Pxxxx` string,
    /// all codes are unique, and each has a defined default severity.
    #[test]
    fn codes_are_namespaced_unique_and_have_severity() {
        let all = [
            DiagnosticCode::UnexpectedToken,
            DiagnosticCode::UnclosedDelimiter,
            DiagnosticCode::NestingDepthExceeded,
            DiagnosticCode::MissingSeparator,
            DiagnosticCode::MissingValue,
        ];
        let mut seen = std::collections::BTreeSet::new();
        for c in all {
            let s = c.code();
            assert!(
                s.starts_with("RON-P"),
                "code {s:?} must be in the RON-P parse namespace"
            );
            assert_eq!(s.len(), "RON-P0000".len(), "codes are RON-Pxxxx (4 digits)");
            assert!(
                s["RON-P".len()..].chars().all(|ch| ch.is_ascii_digit()),
                "code {s:?} must end in 4 decimal digits"
            );
            assert!(seen.insert(s), "duplicate code string {s:?}");
            // default_severity must be total (no panic).
            let _ = c.default_severity();
            assert_eq!(c.to_string(), s);
        }
    }

    /// Specific code-string assertions (these strings are a public contract and
    /// must not drift).
    #[test]
    fn code_strings_are_pinned() {
        assert_eq!(DiagnosticCode::UnexpectedToken.code(), "RON-P0001");
        assert_eq!(DiagnosticCode::UnclosedDelimiter.code(), "RON-P0002");
        assert_eq!(DiagnosticCode::NestingDepthExceeded.code(), "RON-P0003");
        assert_eq!(DiagnosticCode::MissingSeparator.code(), "RON-P0004");
        assert_eq!(DiagnosticCode::MissingValue.code(), "RON-P0005");
    }

    /// `Diagnostic::new` adopts the code's default severity and stores the range.
    #[test]
    fn new_uses_default_severity() {
        let r = TextRange::new(2, 5);
        let d = Diagnostic::new(DiagnosticCode::UnexpectedToken, r, "boom");
        assert_eq!(d.code(), DiagnosticCode::UnexpectedToken);
        assert_eq!(d.severity(), Severity::Error);
        assert_eq!(d.range(), r);
        assert_eq!(d.message(), "boom");
    }
}
