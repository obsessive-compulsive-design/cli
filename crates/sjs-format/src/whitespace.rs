//! S007 and S008 (US-042, SPEC.md §9.1): what the text itself gets wrong about
//! whitespace, judged line by line without the tree's help: indentation with a tab or
//! of a width that is not a multiple of two (S007), a line ending other than `\n`,
//! trailing whitespace and the final newline, missing or followed by blank lines
//! (S008). A line that begins inside a template literal, a string continued over a
//! line break or a block comment is content: its indentation is left alone, and so is
//! the end of a line that ends inside one; line endings are judged everywhere.

use sjs_diag::{Code, Diagnostic, LineTerminator};
use sjs_syntax::TriviaKind;

use crate::expr::Formatter;

impl Formatter<'_> {
    /// S007 and S008 for every line of the file, in order.
    #[must_use]
    pub fn whitespace(&self) -> Vec<Diagnostic> {
        let inner = self.inner_terminators();
        let mut findings = Vec::new();
        let mut start = 0;
        // The offset of the terminator ending the line before.
        let mut previous_end: Option<usize> = None;
        // Where the run of empty lines ending the file began.
        let mut blank_tail: Option<usize> = None;
        let mut number = 1;
        while let Some(line) = self.file.line(number) {
            let end = start + line.text.len();
            let blank = line.text.trim_matches(is_blank).is_empty();
            let last = line.terminator.is_none();
            if let Some(terminator) = line.terminator {
                findings.extend(self.ending(terminator, end));
            }
            let inside = previous_end.is_some_and(|at| inner.binary_search(&at).is_ok());
            let run = (start + line.text.trim_end_matches(is_blank).len())..end;
            if !run.is_empty() && inner.binary_search(&end).is_err() {
                findings.extend(self.trailing(run.start, run.end));
            }
            if !blank && !inside {
                findings.extend(self.indentation(line.text, start));
            }
            if last && !blank {
                findings.extend(self.final_newline(end));
            }
            blank_tail = match (line.text.is_empty(), last) {
                (true, false) => Some(blank_tail.unwrap_or(start)),
                (true, true) => blank_tail,
                (false, _) => None,
            };
            match line.terminator {
                None => break,
                Some(terminator) => start = end + terminator_len(terminator),
            }
            previous_end = Some(end);
            number += 1;
        }
        if let Some(from) = blank_tail {
            findings.extend(self.blank_tail(from, start));
        }
        findings
    }

    /// The offsets of the line terminators inside a template literal, a string continued
    /// over a line break or a block comment, in order: the line each ends and the line
    /// each begins are content.
    fn inner_terminators(&self) -> Vec<usize> {
        let mut offsets = Vec::new();
        for token in self.tree.tokens() {
            for trivia in &token.leading {
                if matches!(
                    trivia.kind,
                    TriviaKind::BlockComment { .. } | TriviaKind::DocComment { .. }
                ) {
                    push_terminators(&mut offsets, self.text(trivia.span), trivia.span.byte_start);
                }
            }
            push_terminators(&mut offsets, self.text(token.span), token.span.byte_start);
        }
        offsets
    }

    /// S008 for a line ending that is not `\n`, with `\n` as the replacement.
    fn ending(&self, terminator: LineTerminator, at: usize) -> Option<Diagnostic> {
        let (name, text) = match terminator {
            LineTerminator::Lf => return None,
            LineTerminator::CrLf => ("`\\r\\n`", "\r\n"),
            LineTerminator::Cr => ("`\\r`", "\r"),
            LineTerminator::LineSeparator => ("U+2028", "\u{2028}"),
            LineTerminator::ParagraphSeparator => ("U+2029", "\u{2029}"),
        };
        self.file.span(at, at + text.len()).and_then(|span| {
            Diagnostic::new(
                Code::S008,
                self.file.path(),
                span,
                format!("a {name} line ending: lines end in `\\n` (SPEC.md §9.1)"),
            )
            .with_fix(text, "\n")
            .ok()
        })
    }

    /// S008 for whitespace ending a line, dropped.
    fn trailing(&self, from: usize, to: usize) -> Option<Diagnostic> {
        self.file.span(from, to).and_then(|span| {
            Diagnostic::new(
                Code::S008,
                self.file.path(),
                span,
                "trailing whitespace: nothing follows the last character of a line (SPEC.md §9.1)",
            )
            .with_fix(self.text(span).to_owned(), "")
            .ok()
        })
    }

    /// S007 for an indentation with a tab in it or of an odd width.
    fn indentation(&self, text: &str, start: usize) -> Option<Diagnostic> {
        let width = text.len() - text.trim_start_matches(is_blank).len();
        let indent = &text[..width];
        let message = if indent.contains('\t') {
            "a tab in the indentation: indentation is spaces, two per level (SPEC.md §9.1)"
                .to_owned()
        } else if width % 2 == 1 {
            format!("an indentation of {width} spaces: two per level (SPEC.md §9.1)")
        } else {
            return None;
        };
        self.file
            .span(start, start + width)
            .map(|span| Diagnostic::new(Code::S007, self.file.path(), span, message))
    }

    /// S008 for a last line that nothing ends.
    fn final_newline(&self, at: usize) -> Option<Diagnostic> {
        self.file.span(at, at).and_then(|span| {
            Diagnostic::new(
                Code::S008,
                self.file.path(),
                span,
                "no final newline: the file ends in exactly one `\\n` (SPEC.md §9.1)",
            )
            .with_fix("", "\n")
            .ok()
        })
    }

    /// S008 for the empty lines ending the file, dropped.
    fn blank_tail(&self, from: usize, to: usize) -> Option<Diagnostic> {
        self.file.span(from, to).and_then(|span| {
            Diagnostic::new(
                Code::S008,
                self.file.path(),
                span,
                "blank lines at the end: the file ends in exactly one `\\n` (SPEC.md §9.1)",
            )
            .with_fix(self.text(span).to_owned(), "")
            .ok()
        })
    }
}

/// A space or a tab: what indentation and trailing whitespace are made of.
fn is_blank(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

/// The offsets, from `start`, of the line terminators in `text` (both bytes of `\r\n`,
/// of which only the first ends a line).
fn push_terminators(offsets: &mut Vec<usize>, text: &str, start: usize) {
    offsets.extend(
        text.match_indices(['\n', '\r', '\u{2028}', '\u{2029}'])
            .map(|(at, _)| start + at),
    );
}

/// The bytes of a line's terminator.
fn terminator_len(terminator: LineTerminator) -> usize {
    match terminator {
        LineTerminator::Lf | LineTerminator::Cr => 1,
        LineTerminator::CrLf => 2,
        LineTerminator::LineSeparator | LineTerminator::ParagraphSeparator => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    /// A finding: its code, line, column and fix.
    type Found = (Code, u32, u32, Option<(String, String)>);

    fn found(text: &str) -> Vec<Found> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .whitespace()
            .into_iter()
            .map(|d| {
                let fix = d.current.clone().zip(d.replacement.clone());
                (d.code, d.span.start.line, d.span.start.column, fix)
            })
            .collect()
    }

    fn fix(current: &str, replacement: &str) -> (String, String) {
        (current.to_owned(), replacement.to_owned())
    }

    #[test]
    fn clean_files_and_the_empty_file_produce_nothing() {
        assert_eq!(found("const a = 1\n"), Vec::new());
        assert_eq!(found(""), Vec::new());
        assert_eq!(found("function f() {\n  return 1\n}\n"), Vec::new());
    }

    #[test]
    fn line_endings_other_than_lf() {
        assert_eq!(
            found("a\r\nb\rc\u{2028}d\u{2029}e\n"),
            vec![
                (Code::S008, 1, 2, Some(fix("\r\n", "\n"))),
                (Code::S008, 2, 2, Some(fix("\r", "\n"))),
                (Code::S008, 3, 2, Some(fix("\u{2028}", "\n"))),
                (Code::S008, 4, 2, Some(fix("\u{2029}", "\n"))),
            ]
        );
    }

    #[test]
    fn trailing_whitespace_and_whitespace_only_lines() {
        assert_eq!(
            found("a  \n\t\nb\n"),
            vec![
                (Code::S008, 1, 2, Some(fix("  ", ""))),
                (Code::S008, 2, 1, Some(fix("\t", ""))),
            ]
        );
        assert_eq!(
            found("a\n  "),
            vec![(Code::S008, 2, 1, Some(fix("  ", "")))],
            "a last line of spaces is trailing whitespace, not a missing newline"
        );
    }

    #[test]
    fn indentation_with_a_tab_or_an_odd_width() {
        let findings = found("f(\n\ta,\n   b,\n  c,\n \t d\n)\n");
        assert_eq!(
            findings
                .iter()
                .map(|(code, line, column, _)| (*code, *line, *column))
                .collect::<Vec<_>>(),
            vec![(Code::S007, 2, 1), (Code::S007, 3, 1), (Code::S007, 5, 1)]
        );
        assert!(findings.iter().all(|(_, _, _, fix)| fix.is_none()));
    }

    #[test]
    fn the_final_newline_missing_or_doubled() {
        assert_eq!(found("a"), vec![(Code::S008, 1, 2, Some(fix("", "\n")))]);
        assert_eq!(
            found("a\n\n"),
            vec![(Code::S008, 2, 1, Some(fix("\n", "")))]
        );
        assert_eq!(
            found("a\n\n\n\n"),
            vec![(Code::S008, 2, 1, Some(fix("\n\n\n", "")))],
            "one finding for the run"
        );
        assert_eq!(found("\n"), vec![(Code::S008, 1, 1, Some(fix("\n", "")))]);
        assert_eq!(
            found("a\n\nb\n"),
            Vec::new(),
            "a blank line in the middle is not the tail"
        );
    }

    #[test]
    fn content_lines_keep_their_whitespace_but_not_their_endings() {
        assert_eq!(found("const t = `a  \n   b  \n`\n"), Vec::new());
        assert_eq!(found("/* a  \n   b  \n */\n"), Vec::new());
        assert_eq!(found("/** a  \n   b */\n"), Vec::new());
        assert_eq!(found("const s = \"a  \\\n   b\"\n"), Vec::new());
        assert_eq!(
            found("const t = `a\r\n b`\r\n"),
            vec![
                (Code::S008, 1, 13, Some(fix("\r\n", "\n"))),
                (Code::S008, 2, 4, Some(fix("\r\n", "\n"))),
            ]
        );
        assert_eq!(
            found("const t = `a\n b`  \n   f(`x${\n   y\n}`)\n"),
            vec![
                (Code::S008, 2, 4, Some(fix("  ", ""))),
                (Code::S007, 3, 1, None),
                (Code::S007, 4, 1, None)
            ],
            "after the closing backtick the line is code again; a substitution is code"
        );
        assert_eq!(
            found("`a\n b`\n"),
            Vec::new(),
            "a template at a line's start"
        );
    }
}
