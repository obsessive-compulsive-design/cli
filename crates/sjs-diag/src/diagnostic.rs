//! A finding, where it is, and the collector that orders findings for the reporter.

use std::collections::BTreeSet;
use std::fmt;

use crate::code::{Code, Severity};

/// A 1-based line and column; the column counts Unicode scalar values (SPEC.md §13.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    /// 1-based line.
    pub line: u32,
    /// 1-based column in Unicode scalar values.
    pub column: u32,
}

impl Position {
    /// A position; `line` and `column` are 1-based.
    #[must_use]
    pub const fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}

/// A half-open source range with both position forms SPEC.md §13.1 requires: line and
/// column for people, UTF-8 byte offsets for tools. US-004 produces these from a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Where the finding starts.
    pub start: Position,
    /// Where it ends (exclusive).
    pub end: Position,
    /// Byte offset of `start` in the UTF-8 source.
    pub byte_start: usize,
    /// Byte offset of `end` in the UTF-8 source.
    pub byte_end: usize,
}

impl Span {
    /// A span from `start` to `end` with the matching byte offsets.
    #[must_use]
    pub const fn new(start: Position, end: Position, byte_start: usize, byte_end: usize) -> Self {
        Self {
            start,
            end,
            byte_start,
            byte_end,
        }
    }
}

/// A second location that explains a finding (the other call site, the declaration).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Related {
    /// Path of the file, as the reporter prints it.
    pub path: String,
    /// Where in that file.
    pub span: Span,
    /// What the location contributes, e.g. `"first call site: string"`.
    pub note: String,
}

/// A style finding's replacement was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixError {
    /// The replacement equals the current text, so applying it would change nothing.
    NoOp {
        /// The text that would have been both current and replacement.
        text: String,
    },
}

impl fmt::Display for FixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FixError::NoOp { text } => write!(f, "replacement equals the current text {text:?}"),
        }
    }
}

impl std::error::Error for FixError {}

/// One finding. Field order is the sort order: path, position, code, then the rest so
/// that ordering is total (SPEC.md §13.3 lists findings by file, line, column).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Diagnostic {
    /// Path of the file, as the reporter prints it.
    pub path: String,
    /// Where the finding is.
    pub span: Span,
    /// Which rule fired.
    pub code: Code,
    /// What is wrong, in the words of the rule's spec section.
    pub message: String,
    /// For style findings: the text as written.
    pub current: Option<String>,
    /// For style findings: the text the formatter would write.
    pub replacement: Option<String>,
    /// Other locations that explain the finding.
    pub related: Vec<Related>,
}

impl Diagnostic {
    /// A finding with no replacement and no related locations.
    #[must_use]
    pub fn new(
        code: Code,
        path: impl Into<String>,
        span: Span,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            span,
            code,
            message: message.into(),
            current: None,
            replacement: None,
            related: Vec::new(),
        }
    }

    /// Attaches the formatter's before/after text (SPEC.md §8.5).
    ///
    /// # Errors
    ///
    /// A replacement equal to the current text is a no-op finding and is rejected.
    pub fn with_fix(
        mut self,
        current: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Result<Self, FixError> {
        let current = current.into();
        let replacement = replacement.into();
        if current == replacement {
            return Err(FixError::NoOp { text: current });
        }
        self.current = Some(current);
        self.replacement = Some(replacement);
        Ok(self)
    }

    /// Adds a related location.
    #[must_use]
    pub fn with_related(
        mut self,
        path: impl Into<String>,
        span: Span,
        note: impl Into<String>,
    ) -> Self {
        self.related.push(Related {
            path: path.into(),
            span,
            note: note.into(),
        });
        self
    }

    /// The code's severity.
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.code.severity()
    }

    /// True for S020, S021 and S030, which tools report but never apply.
    #[must_use]
    pub fn is_hand_edit(&self) -> bool {
        self.code.is_hand_edit()
    }
}

/// Findings from every pass, kept sorted by (path, line, column, code, …) with identical
/// findings collapsed, so the reporter prints one deterministic list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diagnostics {
    items: BTreeSet<Diagnostic>,
}

impl Diagnostics {
    /// An empty collector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a finding; an identical one already present is kept once.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.insert(diagnostic);
    }

    /// Adds every finding of another collector or iterator.
    pub fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.items.extend(diagnostics);
    }

    /// The findings in reporting order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    /// How many distinct findings there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when nothing was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// True when any finding is an E-code; style findings never fail a build (SPEC.md §8).
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity() == Severity::Error)
    }

    /// Number of E-code findings.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .count()
    }

    /// Number of S-code findings.
    #[must_use]
    pub fn style_count(&self) -> usize {
        self.items
            .iter()
            .filter(|d| d.severity() == Severity::Style)
            .count()
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::collections::btree_set::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Code, Diagnostic, Diagnostics, FixError, Position, Related, Severity, Span};

    fn at(line: u32, column: u32) -> Span {
        let offset = usize::try_from(line * 100 + column).expect("small test numbers fit");
        Span::new(
            Position::new(line, column),
            Position::new(line, column + 1),
            offset,
            offset + 1,
        )
    }

    fn finding(path: &str, line: u32, column: u32, code: Code) -> Diagnostic {
        Diagnostic::new(
            code,
            path,
            at(line, column),
            format!("{} here", code.name()),
        )
    }

    #[test]
    fn positions_and_spans_keep_their_fields() {
        let span = Span::new(Position::new(3, 7), Position::new(3, 9), 40, 42);
        assert_eq!((span.start.line, span.start.column), (3, 7));
        assert_eq!((span.end.line, span.end.column), (3, 9));
        assert_eq!((span.byte_start, span.byte_end), (40, 42));
        assert!(
            Position::new(1, 9) < Position::new(2, 1),
            "lines order before columns"
        );
        assert!(Position::new(2, 1) < Position::new(2, 2));
    }

    #[test]
    fn a_new_diagnostic_has_no_fix_and_no_related_locations() {
        let d = finding("src/a.js", 1, 1, Code::E103);
        assert_eq!(d.path, "src/a.js");
        assert_eq!(d.code, Code::E103);
        assert_eq!(d.message, "non-exhaustive here");
        assert_eq!(d.current, None);
        assert_eq!(d.replacement, None);
        assert_eq!(d.related, Vec::new());
        assert_eq!(d.severity(), Severity::Error);
        assert!(!d.is_hand_edit());
    }

    #[test]
    fn a_fix_records_before_and_after_text() {
        let d = finding("src/a.js", 1, 1, Code::S001)
            .with_fix("'a'", "\"a\"")
            .expect("different texts are a real fix");
        assert_eq!(d.current.as_deref(), Some("'a'"));
        assert_eq!(d.replacement.as_deref(), Some("\"a\""));
        assert_eq!(d.severity(), Severity::Style);
    }

    #[test]
    fn a_replacement_equal_to_the_current_text_is_rejected() {
        let err = finding("src/a.js", 1, 1, Code::S002)
            .with_fix(";", ";")
            .expect_err("a no-op fix is not a finding");
        assert_eq!(
            err,
            FixError::NoOp {
                text: ";".to_owned()
            }
        );
        assert_eq!(err.to_string(), "replacement equals the current text \";\"");
    }

    #[test]
    fn related_locations_are_kept_in_insertion_order() {
        let d = finding("src/a.js", 5, 1, Code::E138)
            .with_related("src/b.js", at(1, 1), "first call site: string")
            .with_related("src/c.js", at(2, 2), "second call site: number");
        assert_eq!(
            d.related,
            vec![
                Related {
                    path: "src/b.js".to_owned(),
                    span: at(1, 1),
                    note: "first call site: string".to_owned(),
                },
                Related {
                    path: "src/c.js".to_owned(),
                    span: at(2, 2),
                    note: "second call site: number".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn hand_edit_findings_are_flagged_through_the_diagnostic() {
        for code in [Code::S020, Code::S021, Code::S030] {
            assert!(finding("x.js", 1, 1, code).is_hand_edit(), "{code}");
        }
        for code in [Code::S001, Code::S012, Code::E000] {
            assert!(!finding("x.js", 1, 1, code).is_hand_edit(), "{code}");
        }
    }

    fn sample() -> Vec<Diagnostic> {
        vec![
            finding("src/b.js", 1, 1, Code::E000),
            finding("src/a.js", 2, 1, Code::E103),
            finding("src/a.js", 1, 5, Code::S001),
            finding("src/a.js", 1, 5, Code::E105),
            finding("src/a.js", 1, 2, Code::E117),
            finding("src/a.js", 1, 5, Code::S002),
        ]
    }

    #[test]
    fn findings_sort_by_path_then_line_then_column_then_code() {
        let diagnostics: Diagnostics = sample().into_iter().collect();
        let order: Vec<(String, u32, u32, Code)> = diagnostics
            .iter()
            .map(|d| {
                (
                    d.path.clone(),
                    d.span.start.line,
                    d.span.start.column,
                    d.code,
                )
            })
            .collect();
        assert_eq!(
            order,
            vec![
                ("src/a.js".to_owned(), 1, 2, Code::E117),
                ("src/a.js".to_owned(), 1, 5, Code::E105),
                ("src/a.js".to_owned(), 1, 5, Code::S001),
                ("src/a.js".to_owned(), 1, 5, Code::S002),
                ("src/a.js".to_owned(), 2, 1, Code::E103),
                ("src/b.js".to_owned(), 1, 1, Code::E000),
            ]
        );
    }

    /// Every permutation of the sample yields the same order. US-008 replaces this
    /// hand-rolled shuffle with `sjs-testkit::prop`.
    #[test]
    fn every_insertion_order_yields_the_same_report_order() {
        let expected: Diagnostics = sample().into_iter().collect();
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..256 {
            let mut items = sample();
            for i in (1..items.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let j = usize::try_from(state % (i as u64 + 1)).expect("index fits");
                items.swap(i, j);
            }
            let mut shuffled = Diagnostics::new();
            for item in items {
                shuffled.push(item);
            }
            assert_eq!(shuffled, expected);
        }
    }

    #[test]
    fn identical_findings_from_two_passes_collapse_to_one() {
        let mut diagnostics = Diagnostics::new();
        diagnostics.push(finding("src/a.js", 1, 1, Code::E115));
        diagnostics.push(finding("src/a.js", 1, 1, Code::E115));
        diagnostics.extend(vec![finding("src/a.js", 1, 1, Code::E115)]);
        assert_eq!(diagnostics.len(), 1);
        // `extend` adds what is new and skips what is already there.
        diagnostics.extend(vec![
            finding("src/a.js", 1, 1, Code::E115),
            finding("src/a.js", 1, 1, Code::E116),
            finding("src/b.js", 1, 1, Code::E116),
        ]);
        assert_eq!(diagnostics.len(), 3);
        let codes: Vec<Code> = diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [Code::E115, Code::E116, Code::E116]);
        // A different message at the same position is a different finding.
        diagnostics.push(Diagnostic::new(
            Code::E115,
            "src/a.js",
            at(1, 1),
            "another message",
        ));
        assert_eq!(diagnostics.len(), 4);
    }

    #[test]
    fn counts_and_has_errors_follow_severity() {
        let mut diagnostics = Diagnostics::new();
        assert!(diagnostics.is_empty());
        assert!(!diagnostics.has_errors());
        assert_eq!(
            (
                diagnostics.len(),
                diagnostics.error_count(),
                diagnostics.style_count()
            ),
            (0, 0, 0)
        );

        diagnostics.push(finding("src/a.js", 1, 1, Code::S001));
        diagnostics.push(finding("src/a.js", 2, 1, Code::S020));
        assert!(!diagnostics.is_empty());
        assert!(
            !diagnostics.has_errors(),
            "style findings never fail the build"
        );
        assert_eq!(
            (
                diagnostics.len(),
                diagnostics.error_count(),
                diagnostics.style_count()
            ),
            (2, 0, 2)
        );

        diagnostics.push(finding("src/a.js", 3, 1, Code::E000));
        assert!(diagnostics.has_errors());
        assert_eq!(
            (
                diagnostics.len(),
                diagnostics.error_count(),
                diagnostics.style_count()
            ),
            (3, 1, 2)
        );
    }

    #[test]
    fn into_iter_yields_the_sorted_findings_once() {
        let diagnostics: Diagnostics = sample().into_iter().collect();
        let codes: Vec<Code> = diagnostics.clone().into_iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            [
                Code::E117,
                Code::E105,
                Code::S001,
                Code::S002,
                Code::E103,
                Code::E000
            ]
        );
        assert_eq!(diagnostics.len(), 6);
    }
}
