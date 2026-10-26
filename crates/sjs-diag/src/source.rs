//! A source file with the line index every position in every diagnostic comes from
//! (SPEC.md §13.1: 1-based lines, columns in Unicode scalar values, byte offsets alongside).

use crate::code::Code;
use crate::diagnostic::{Diagnostic, Position, Span};

/// One of the ES2025 line terminators, remembered per line for the layout rules (S008).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineTerminator {
    /// `\n`
    Lf,
    /// `\r\n`
    CrLf,
    /// `\r`
    Cr,
    /// U+2028
    LineSeparator,
    /// U+2029
    ParagraphSeparator,
}

impl LineTerminator {
    /// The terminator's text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            LineTerminator::Lf => "\n",
            LineTerminator::CrLf => "\r\n",
            LineTerminator::Cr => "\r",
            LineTerminator::LineSeparator => "\u{2028}",
            LineTerminator::ParagraphSeparator => "\u{2029}",
        }
    }
}

/// One line of a file: its text without the terminator, and the terminator that ended
/// it (`None` only for the last line, which may be empty).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// The line's text, terminator excluded.
    pub text: &'a str,
    /// What ended the line; `None` for the final line.
    pub terminator: Option<LineTerminator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LineRecord {
    /// Byte offset of the first character of the line.
    start: usize,
    /// Byte offset just past the last character before the terminator.
    end: usize,
    terminator: Option<LineTerminator>,
}

/// A validated UTF-8 source file with its line index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    path: String,
    text: String,
    lines: Vec<LineRecord>,
}

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

/// Lines and columns are `u32` in [`Position`]; a count that does not fit saturates
/// rather than failing, so no position lookup has an error path for a 4-billion-line file.
fn saturating_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The inverse: a `u32` always fits a `usize` on every target this compiler builds for.
fn to_usize(number: u32) -> usize {
    usize::try_from(number).unwrap_or(usize::MAX)
}

impl SourceFile {
    /// Validates `bytes` as UTF-8 without a byte-order mark and indexes its lines.
    ///
    /// # Errors
    ///
    /// Invalid UTF-8 is `E000` at the first bad byte; a BOM is `E000` at 1:1 (SPEC.md §2
    /// says UTF-8, and a BOM would be copied into `dist/` byte for byte).
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Result<Self, Box<Diagnostic>> {
        let path = path.into();
        if bytes.starts_with(BOM) {
            let span = Span::new(Position::new(1, 1), Position::new(1, 2), 0, BOM.len());
            return Err(Box::new(Diagnostic::new(
                Code::E000,
                path,
                span,
                "byte-order mark: the subset is UTF-8 without a BOM",
            )));
        }
        match String::from_utf8(bytes) {
            Ok(text) => Ok(Self::from_text(path, text)),
            Err(error) => {
                let utf8 = error.utf8_error();
                let bad_start = utf8.valid_up_to();
                let bad_end = bad_start + utf8.error_len().unwrap_or(1);
                let mut bytes = error.into_bytes();
                bytes.truncate(bad_start);
                let prefix = String::from_utf8(bytes).unwrap_or_default();
                let valid = Self::from_text(path.clone(), prefix);
                let start = valid.position(bad_start).unwrap_or(Position::new(1, 1));
                let end = Position::new(start.line, start.column + 1);
                let span = Span::new(start, end, bad_start, bad_end);
                Err(Box::new(Diagnostic::new(
                    Code::E000,
                    path,
                    span,
                    format!("invalid UTF-8 at byte {bad_start}: the subset is UTF-8"),
                )))
            }
        }
    }

    fn from_text(path: String, text: String) -> Self {
        let mut lines = Vec::new();
        let mut start = 0;
        let mut chars = text.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            let terminator = match ch {
                '\n' => LineTerminator::Lf,
                '\r' => {
                    if chars.peek().is_some_and(|&(_, next)| next == '\n') {
                        chars.next();
                        LineTerminator::CrLf
                    } else {
                        LineTerminator::Cr
                    }
                }
                '\u{2028}' => LineTerminator::LineSeparator,
                '\u{2029}' => LineTerminator::ParagraphSeparator,
                _ => continue,
            };
            lines.push(LineRecord {
                start,
                end: index,
                terminator: Some(terminator),
            });
            start = index + terminator.as_str().len();
        }
        lines.push(LineRecord {
            start,
            end: text.len(),
            terminator: None,
        });
        Self { path, text, lines }
    }

    /// The path as the reporter prints it.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The whole text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// True for a zero-byte file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Number of lines; a file ending in a terminator has a final empty line, so this is
    /// one more than the number of terminators.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The 1-based line `number`, or `None` past the end.
    #[must_use]
    pub fn line(&self, number: u32) -> Option<Line<'_>> {
        let record = self.lines.get(to_usize(number).checked_sub(1)?)?;
        Some(Line {
            text: &self.text[record.start..record.end],
            terminator: record.terminator,
        })
    }

    /// The line/column of a byte offset, or `None` when the offset is past the end or
    /// not on a character boundary. Terminator characters have columns of their own, and
    /// the offset just after a terminator is column 1 of the next line.
    #[must_use]
    pub fn position(&self, offset: usize) -> Option<Position> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let index = self.lines.partition_point(|line| line.start <= offset) - 1;
        let line = saturating_u32(index + 1);
        let column = saturating_u32(self.text[self.lines[index].start..offset].chars().count() + 1);
        Some(Position::new(line, column))
    }

    /// The byte offset of a line/column, or `None` when no character boundary has that
    /// position (line past the end, column 0, or column past the line's terminator).
    #[must_use]
    pub fn offset(&self, position: Position) -> Option<usize> {
        let index = to_usize(position.line).checked_sub(1)?;
        let record = self.lines.get(index)?;
        let next_start = self
            .lines
            .get(index + 1)
            .map_or(self.text.len() + 1, |next| next.start);
        let steps = to_usize(position.column).checked_sub(1)?;
        let offset = self.text[record.start..]
            .char_indices()
            .map(|(i, _)| record.start + i)
            .chain(std::iter::once(self.text.len()))
            .nth(steps)?;
        (offset < next_start).then_some(offset)
    }

    /// A span over `byte_start..byte_end` with both position forms, or `None` when either
    /// offset is invalid or the range is reversed.
    #[must_use]
    pub fn span(&self, byte_start: usize, byte_end: usize) -> Option<Span> {
        if byte_start > byte_end {
            return None;
        }
        Some(Span::new(
            self.position(byte_start)?,
            self.position(byte_end)?,
            byte_start,
            byte_end,
        ))
    }

    /// The text a span covers, or `None` when its offsets are reversed, past the end or
    /// not on character boundaries (all of which `str::get` already rejects).
    #[must_use]
    pub fn slice(&self, span: &Span) -> Option<&str> {
        self.text.get(span.byte_start..span.byte_end)
    }
}

#[cfg(test)]
mod tests {
    use super::{Line, LineTerminator, SourceFile};
    use crate::code::Code;
    use crate::diagnostic::{Position, Span};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("src/a.js", text.as_bytes().to_vec()).expect("valid UTF-8 without a BOM")
    }

    /// The reference: walk the characters and count the way SPEC.md §13.1 describes,
    /// treating `\r\n` as one terminator.
    fn naive_positions(text: &str) -> Vec<(usize, Position)> {
        let mut out = Vec::new();
        let (mut line, mut column) = (1, 1);
        let mut chars = text.char_indices().peekable();
        while let Some((offset, ch)) = chars.next() {
            out.push((offset, Position::new(line, column)));
            let crlf = ch == '\r' && chars.peek().is_some_and(|&(_, next)| next == '\n');
            if crlf {
                let (offset, _) = chars.next().expect("peeked");
                out.push((offset, Position::new(line, column + 1)));
            }
            if matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        out.push((text.len(), Position::new(line, column)));
        out
    }

    const SAMPLES: &[&str] = &[
        "",
        "a",
        "\n",
        "a\n",
        "a\nb",
        "a\r\nb\rc\u{2028}d\u{2029}e\n",
        "\r",
        "\r\r\n\n\r",
        "héllo wörld\n😀😀\n",
        "const s = \"x\u{2028}y\"\n",
    ];

    #[test]
    fn positions_match_the_naive_scan_at_every_character_boundary() {
        for text in SAMPLES {
            let f = file(text);
            for (offset, expected) in naive_positions(text) {
                assert_eq!(
                    f.position(offset),
                    Some(expected),
                    "{text:?} at byte {offset}"
                );
                assert_eq!(f.offset(expected), Some(offset), "{text:?} at {expected:?}");
            }
        }
    }

    #[test]
    fn a_line_of_a_hundred_multibyte_characters_ends_at_column_one_hundred() {
        let text = "é".repeat(100);
        let f = file(&text);
        assert_eq!(f.len(), 200);
        assert_eq!(f.position(198), Some(Position::new(1, 100)));
        assert_eq!(f.position(200), Some(Position::new(1, 101)));
        assert_eq!(f.offset(Position::new(1, 100)), Some(198));
        assert_eq!(f.position(199), None, "inside a character");
    }

    #[test]
    fn lines_keep_their_text_and_terminator() {
        let f = file("a\r\nb\rc\u{2028}d\u{2029}e\n");
        assert_eq!(f.line_count(), 6);
        let expected = [
            ("a", Some(LineTerminator::CrLf)),
            ("b", Some(LineTerminator::Cr)),
            ("c", Some(LineTerminator::LineSeparator)),
            ("d", Some(LineTerminator::ParagraphSeparator)),
            ("e", Some(LineTerminator::Lf)),
            ("", None),
        ];
        for (number, (text, terminator)) in (1..).zip(expected) {
            assert_eq!(
                f.line(number),
                Some(Line { text, terminator }),
                "line {number}"
            );
        }
        assert_eq!(f.line(0), None);
        assert_eq!(f.line(7), None);
        assert_eq!(f.path(), "src/a.js");
        assert_eq!(f.text(), "a\r\nb\rc\u{2028}d\u{2029}e\n");
    }

    #[test]
    fn counts_that_do_not_fit_a_u32_saturate() {
        assert_eq!(super::saturating_u32(0), 0);
        assert_eq!(super::saturating_u32(4_294_967_295), u32::MAX);
        assert_eq!(super::saturating_u32(4_294_967_296), u32::MAX);
        assert_eq!(super::to_usize(0), 0);
        assert_eq!(super::to_usize(u32::MAX), 4_294_967_295);
    }

    #[test]
    fn terminator_text_is_the_terminator() {
        assert_eq!(LineTerminator::Lf.as_str(), "\n");
        assert_eq!(LineTerminator::CrLf.as_str(), "\r\n");
        assert_eq!(LineTerminator::Cr.as_str(), "\r");
        assert_eq!(LineTerminator::LineSeparator.as_str(), "\u{2028}");
        assert_eq!(LineTerminator::ParagraphSeparator.as_str(), "\u{2029}");
    }

    #[test]
    fn empty_and_unterminated_files() {
        let empty = file("");
        assert!(empty.is_empty());
        assert_eq!((empty.len(), empty.line_count()), (0, 1));
        assert_eq!(
            empty.line(1),
            Some(Line {
                text: "",
                terminator: None
            })
        );
        assert_eq!(empty.position(0), Some(Position::new(1, 1)));
        assert_eq!(empty.position(1), None);
        assert_eq!(empty.offset(Position::new(1, 1)), Some(0));
        assert_eq!(empty.offset(Position::new(1, 2)), None);

        let unterminated = file("ab\ncd");
        assert!(!unterminated.is_empty());
        assert_eq!(unterminated.line_count(), 2);
        assert_eq!(
            unterminated.line(2),
            Some(Line {
                text: "cd",
                terminator: None
            })
        );
        assert_eq!(unterminated.position(5), Some(Position::new(2, 3)));
        assert_eq!(unterminated.offset(Position::new(2, 3)), Some(5));
        assert_eq!(unterminated.offset(Position::new(2, 4)), None);
    }

    #[test]
    fn a_lone_carriage_return_is_a_terminator() {
        let f = file("a\rb");
        assert_eq!(f.line_count(), 2);
        assert_eq!(
            f.line(1),
            Some(Line {
                text: "a",
                terminator: Some(LineTerminator::Cr)
            })
        );
        assert_eq!(f.position(2), Some(Position::new(2, 1)));
    }

    #[test]
    fn a_line_separator_inside_a_string_literal_still_ends_a_line() {
        let f = file("const s = \"x\u{2028}y\"\n");
        assert_eq!(f.line_count(), 3);
        assert_eq!(f.line(1).map(|l| l.text), Some("const s = \"x"));
        assert_eq!(f.line(2).map(|l| l.text), Some("y\""));
        assert_eq!(f.position(15), Some(Position::new(2, 1)));
    }

    #[test]
    fn offsets_reject_positions_no_character_has() {
        let f = file("ab\ncd\n");
        assert_eq!(f.offset(Position::new(0, 1)), None, "line 0");
        assert_eq!(f.offset(Position::new(1, 0)), None, "column 0");
        assert_eq!(
            f.offset(Position::new(1, 3)),
            Some(2),
            "the terminator itself"
        );
        assert_eq!(
            f.offset(Position::new(1, 4)),
            None,
            "past the terminator is line 2"
        );
        assert_eq!(
            f.offset(Position::new(3, 1)),
            Some(6),
            "the final empty line"
        );
        assert_eq!(f.offset(Position::new(3, 2)), None);
        assert_eq!(f.offset(Position::new(4, 1)), None, "line past the end");
        let crlf = file("ab\r\ncd");
        assert_eq!(crlf.offset(Position::new(1, 3)), Some(2), "the \\r");
        assert_eq!(crlf.offset(Position::new(1, 4)), Some(3), "the \\n");
        assert_eq!(crlf.offset(Position::new(1, 5)), None);
        assert_eq!(crlf.offset(Position::new(2, 1)), Some(4));
    }

    #[test]
    fn spans_and_slices_use_both_position_forms() {
        let f = file("ab\ncd\n");
        let span = f.span(1, 4).expect("valid offsets");
        assert_eq!(
            span,
            Span::new(Position::new(1, 2), Position::new(2, 2), 1, 4)
        );
        assert_eq!(f.slice(&span), Some("b\nc"));
        assert_eq!(f.span(4, 1), None, "reversed");
        assert_eq!(f.span(0, 7), None, "end past the end");
        assert_eq!(f.span(7, 8), None, "start past the end");
        assert_eq!(
            f.span(6, 6),
            Some(Span::new(Position::new(3, 1), Position::new(3, 1), 6, 6))
        );
        assert_eq!(
            f.slice(&Span::new(Position::new(1, 1), Position::new(1, 1), 4, 1)),
            None
        );
        assert_eq!(
            f.slice(&Span::new(Position::new(1, 1), Position::new(1, 1), 0, 7)),
            None
        );
        let multibyte = file("é");
        assert_eq!(
            multibyte.slice(&Span::new(Position::new(1, 1), Position::new(1, 1), 0, 1)),
            None
        );
    }

    #[test]
    fn a_byte_order_mark_is_e000_at_one_one() {
        let err =
            SourceFile::new("src/a.js", b"\xEF\xBB\xBFconst a = 1\n".to_vec()).expect_err("BOM");
        assert_eq!(err.code, Code::E000);
        assert_eq!(err.path, "src/a.js");
        assert_eq!(
            err.span,
            Span::new(Position::new(1, 1), Position::new(1, 2), 0, 3)
        );
        assert!(err.message.contains("byte-order mark"), "{}", err.message);
    }

    #[test]
    fn invalid_utf8_is_e000_at_the_first_bad_byte() {
        let err = SourceFile::new("src/a.js", b"ab\nc\xFFd".to_vec()).expect_err("bad byte");
        assert_eq!(err.code, Code::E000);
        assert_eq!(
            err.span,
            Span::new(Position::new(2, 2), Position::new(2, 3), 4, 5)
        );
        assert_eq!(err.message, "invalid UTF-8 at byte 4: the subset is UTF-8");

        let truncated =
            SourceFile::new("src/a.js", b"\xE2\x82".to_vec()).expect_err("truncated sequence");
        assert_eq!(
            truncated.span,
            Span::new(Position::new(1, 1), Position::new(1, 2), 0, 1)
        );

        let only_bad = SourceFile::new("src/a.js", b"\x80".to_vec()).expect_err("bad first byte");
        assert_eq!(only_bad.span.start, Position::new(1, 1));
        assert_eq!(only_bad.span.byte_end, 1);
    }

    /// Random strings over an alphabet of terminators and multi-byte characters: every
    /// character boundary round-trips through position and back. US-008 replaces this
    /// generator with `sjs-testkit::prop`.
    #[test]
    fn every_character_boundary_round_trips_on_random_strings() {
        const ALPHABET: &[char] = &['a', 'é', '😀', ' ', '\n', '\r', '\u{2028}', '\u{2029}'];
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..512 {
            let len = usize::try_from(next() % 24).expect("small");
            let text: String = (0..len)
                .map(|_| ALPHABET[usize::try_from(next() % 8).expect("small")])
                .collect();
            let f = file(&text);
            let naive = naive_positions(&text);
            assert_eq!(
                f.line_count(),
                naive.last().map_or(0, |(_, p)| p.line as usize),
                "{text:?}"
            );
            for (offset, expected) in naive {
                assert_eq!(f.position(offset), Some(expected), "{text:?} at {offset}");
                assert_eq!(f.offset(expected), Some(offset), "{text:?} at {expected:?}");
            }
            for offset in 0..=text.len() {
                if !text.is_char_boundary(offset) {
                    assert_eq!(
                        f.position(offset),
                        None,
                        "{text:?} inside a character at {offset}"
                    );
                }
            }
        }
    }
}
