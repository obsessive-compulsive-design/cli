//! String literals (SPEC.md §2, §8.5): `"…"` with the ES2025 escapes, cooked to their
//! value; `'…'` is the same literal reported as S001 with a double-quoted replacement.
//! Legacy octal escapes, `\8`, `\9`, malformed `\x`/`\u` and lone surrogates are E000.

/// An E000 inside a literal: the escape at `start..end`.
pub(crate) struct Problem {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// What scanning a literal found.
pub(crate) struct Scan {
    /// After the closing quote; at the line terminator or the end of the file when
    /// unterminated.
    pub end: usize,
    pub terminated: bool,
    /// The string value, with escapes decoded and line continuations removed.
    pub cooked: String,
    pub problems: Vec<Problem>,
}

/// Scans the literal whose opening quote is at `start`.
pub(crate) fn scan(text: &str, start: usize) -> Scan {
    let quote = text[start..].chars().next().unwrap_or('"');
    let mut scanner = Scanner {
        text,
        pos: start + 1,
        cooked: String::new(),
        problems: Vec::new(),
    };
    let terminated = scanner.body(quote);
    Scan {
        end: scanner.pos,
        terminated,
        cooked: scanner.cooked,
        problems: scanner.problems,
    }
}

/// The double-quoted spelling of a single-quoted literal `raw`, changing only what the
/// quotes force: `\'` loses its backslash and a bare `"` gains one (SPEC.md §8.5).
pub(crate) fn double_quoted(raw: &str) -> String {
    let mut out = String::from("\"");
    let mut chars = raw[1..raw.len() - 1].chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                // A terminated literal never ends in a lone backslash.
                let next = chars.next().unwrap_or('\\');
                if next == '\'' {
                    out.push('\'');
                } else {
                    out.push('\\');
                    out.push(next);
                }
            }
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
    cooked: String,
    problems: Vec<Problem>,
}

impl Scanner<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    /// Everything after the opening quote. True when the closing quote was found.
    fn body(&mut self, quote: char) -> bool {
        loop {
            let Some(ch) = self.peek() else {
                return false;
            };
            match ch {
                '\n' | '\r' => return false,
                _ if ch == quote => {
                    self.pos += 1;
                    return true;
                }
                '\\' => self.escape(),
                _ => {
                    self.cooked.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn problem(&mut self, start: usize, message: String) {
        self.problems.push(Problem {
            start,
            end: self.pos,
            message,
        });
    }

    /// The escape whose backslash is at `self.pos`.
    fn escape(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let Some(ch) = self.peek() else {
            return;
        };
        self.pos += ch.len_utf8();
        let simple = match ch {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'b' => '\u{8}',
            'f' => '\u{C}',
            'v' => '\u{B}',
            '0' if !self.peek().is_some_and(|c| c.is_ascii_digit()) => '\0',
            '0'..='7' => return self.legacy_octal(start),
            '8' | '9' => {
                return self.problem(start, format!("`\\{ch}` is not an escape sequence"));
            }
            'x' => return self.hex_escape(start),
            'u' => return self.unicode_escape(start),
            '\r' => {
                if self.peek() == Some('\n') {
                    self.pos += 1;
                }
                return;
            }
            '\n' | '\u{2028}' | '\u{2029}' => return,
            other => other,
        };
        self.cooked.push(simple);
    }

    fn legacy_octal(&mut self, start: usize) {
        while self.peek().is_some_and(|c| matches!(c, '0'..='7')) {
            self.pos += 1;
        }
        let raw = &self.text[start..self.pos];
        let value = u32::from_str_radix(&raw[1..], 8).unwrap_or(0);
        self.problem(
            start,
            format!("`{raw}` is a legacy octal escape; write `\\x{value:02X}`"),
        );
    }

    /// `\xHH`.
    fn hex_escape(&mut self, start: usize) {
        match self.hex_digits(2) {
            Some(value) => self.cooked.push(char::from_u32(value).unwrap_or('\0')),
            None => self.problem(
                start,
                "`\\x` must be followed by exactly two hexadecimal digits".to_owned(),
            ),
        }
    }

    /// `\uHHHH` or `\u{H…}`; a surrogate pair written as two escapes is one character.
    fn unicode_escape(&mut self, start: usize) {
        let Some(unit) = self.unicode_unit() else {
            return self.problem(
                start,
                "`\\u` must be followed by four hexadecimal digits or `{…}` holding a code point up to 10FFFF".to_owned(),
            );
        };
        let value = if (0xD800..0xDC00).contains(&unit) {
            let before_low = self.pos;
            let low = if self.text[self.pos..].starts_with("\\u") {
                self.pos += 2;
                self.unicode_unit()
            } else {
                None
            };
            match low {
                Some(low) if (0xDC00..0xE000).contains(&low) => {
                    0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00)
                }
                _ => {
                    self.pos = before_low;
                    unit
                }
            }
        } else {
            unit
        };
        match char::from_u32(value) {
            Some(ch) => self.cooked.push(ch),
            None => self.problem(
                start,
                format!("`\\u{{{value:X}}}` is a lone surrogate, not a character"),
            ),
        }
    }

    /// The code unit or code point after `\u`, when well formed.
    fn unicode_unit(&mut self) -> Option<u32> {
        if self.peek() == Some('{') {
            self.pos += 1;
            let digits_start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            let digits = &self.text[digits_start..self.pos];
            if self.peek() != Some('}') || digits.is_empty() || digits.len() > 6 {
                return None;
            }
            self.pos += 1;
            let value = u32::from_str_radix(digits, 16).unwrap_or(u32::MAX);
            (value <= 0x10_FFFF).then_some(value)
        } else {
            self.hex_digits(4)
        }
    }

    /// Exactly `count` hexadecimal digits.
    fn hex_digits(&mut self, count: usize) -> Option<u32> {
        let start = self.pos;
        for _ in 0..count {
            if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            self.pos += 1;
        }
        u32::from_str_radix(&self.text[start..self.pos], 16).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{double_quoted, scan};

    fn cooked(literal: &str) -> String {
        let scan = scan(literal, 0);
        assert!(scan.terminated, "{literal}");
        assert!(scan.problems.is_empty(), "{literal}");
        assert_eq!(scan.end, literal.len(), "{literal}");
        scan.cooked
    }

    fn problems(literal: &str) -> Vec<(usize, usize, String)> {
        scan(literal, 0)
            .problems
            .into_iter()
            .map(|p| (p.start, p.end, p.message))
            .collect()
    }

    #[test]
    fn plain_and_escaped_text_cooks_to_its_value() {
        assert_eq!(cooked(r#""""#), "");
        assert_eq!(cooked(r#""abc""#), "abc");
        assert_eq!(
            cooked(r#""a\n\t\"\\A\u{1F600}\x41""#),
            "a\n\t\"\\A\u{1F600}A"
        );
        assert_eq!(cooked(r#""\r\b\f\v\0""#), "\r\u{8}\u{C}\u{B}\0");
        assert_eq!(cooked(r#""\0a""#), "\0a", "`\\0` not followed by a digit");
        assert_eq!(cooked(r#""\'\"\\""#), "'\"\\");
        assert_eq!(
            cooked(r#""\a\q\é""#),
            "aqé",
            "a non-escape character is itself"
        );
        assert_eq!(cooked(r#""\xff\x00""#), "\u{FF}\0");
        assert_eq!(cooked(r#""\u0041\u{41}\u{000041}""#), "AAA");
        assert_eq!(cooked(r#""\u{10FFFF}""#), "\u{10FFFF}");
        assert_eq!(cooked("\"é😀\u{2028}\u{2029}\""), "é😀\u{2028}\u{2029}");
        assert_eq!(cooked(r"'it\'s'"), "it's");
        assert_eq!(cooked(r#"'say "hi"'"#), "say \"hi\"");
    }

    #[test]
    fn surrogate_pairs_combine_and_lone_surrogates_are_problems() {
        assert_eq!(cooked(r#""\uD83D\uDE00""#), "😀");
        assert_eq!(cooked(r#""\u{D83D}\u{DE00}""#), "😀");
        assert_eq!(cooked(r#""\uD83D\u{DE00}x""#), "😀x");
        assert_eq!(
            problems(r#""\uD83D""#),
            [(
                1,
                7,
                "`\\u{D83D}` is a lone surrogate, not a character".to_owned()
            )]
        );
        assert_eq!(
            problems(r#""\uDE00""#),
            [(
                1,
                7,
                "`\\u{DE00}` is a lone surrogate, not a character".to_owned()
            )]
        );
        assert_eq!(
            problems(r#""\uD83Dx\uDE00""#),
            [
                (
                    1,
                    7,
                    "`\\u{D83D}` is a lone surrogate, not a character".to_owned()
                ),
                (
                    8,
                    14,
                    "`\\u{DE00}` is a lone surrogate, not a character".to_owned()
                ),
            ],
            "a high surrogate followed by a non-escape stays lone; the later low one too"
        );
        assert_eq!(
            problems(r#""\uD83D\u0041""#),
            [(
                1,
                7,
                "`\\u{D83D}` is a lone surrogate, not a character".to_owned()
            )],
            "a high surrogate followed by a non-surrogate escape"
        );
        assert_eq!(
            scan(r#""\uD83D\u0041""#, 0).cooked,
            "A",
            "the second escape still cooks"
        );
        assert_eq!(
            problems(r#""\uD83D\uZZZZ""#),
            [
                (1, 7, "`\\u{D83D}` is a lone surrogate, not a character".to_owned()),
                (7, 9, "`\\u` must be followed by four hexadecimal digits or `{…}` holding a code point up to 10FFFF".to_owned()),
            ],
            "a malformed second escape is its own problem"
        );
    }

    #[test]
    fn line_continuations_are_removed() {
        assert_eq!(cooked("\"a\\\nb\""), "ab");
        assert_eq!(cooked("\"a\\\r\nb\""), "ab");
        assert_eq!(cooked("\"a\\\rb\""), "ab");
        assert_eq!(cooked("\"a\\\u{2028}b\\\u{2029}c\""), "abc");
    }

    #[test]
    fn bad_escapes_are_problems_at_the_backslash() {
        let hex = "`\\x` must be followed by exactly two hexadecimal digits".to_owned();
        let uni = "`\\u` must be followed by four hexadecimal digits or `{…}` holding a code point up to 10FFFF".to_owned();
        let cases: [(&str, usize, usize, String); 18] = [
            (
                r#""\1""#,
                1,
                3,
                "`\\1` is a legacy octal escape; write `\\x01`".to_owned(),
            ),
            (
                r#""\01""#,
                1,
                4,
                "`\\01` is a legacy octal escape; write `\\x01`".to_owned(),
            ),
            (
                r#""\00""#,
                1,
                4,
                "`\\00` is a legacy octal escape; write `\\x00`".to_owned(),
            ),
            (
                r#""\377""#,
                1,
                5,
                "`\\377` is a legacy octal escape; write `\\xFF`".to_owned(),
            ),
            (
                r#""\7777""#,
                1,
                6,
                "`\\7777` is a legacy octal escape; write `\\xFFF`".to_owned(),
            ),
            (
                r#""\8""#,
                1,
                3,
                "`\\8` is not an escape sequence".to_owned(),
            ),
            (
                r#""\9""#,
                1,
                3,
                "`\\9` is not an escape sequence".to_owned(),
            ),
            (r#""\x""#, 1, 3, hex.clone()),
            (r#""\x4""#, 1, 4, hex.clone()),
            (r#""\x4G""#, 1, 4, hex.clone()),
            (r#""\u""#, 1, 3, uni.clone()),
            (r#""\u123""#, 1, 6, uni.clone()),
            (r#""\u{}""#, 1, 4, uni.clone()),
            (r#""\u{41""#, 1, 6, uni.clone()),
            (r#""\u{110000}""#, 1, 11, uni.clone()),
            (r#""\u{0000041}""#, 1, 11, uni.clone()),
            (r#""\u{FFFFFF}""#, 1, 11, uni.clone()),
            (
                r#""a\1b\8""#,
                2,
                4,
                "`\\1` is a legacy octal escape; write `\\x01`".to_owned(),
            ),
        ];
        for (literal, start, end, message) in cases {
            let found = problems(literal);
            assert_eq!(found[0], (start, end, message), "{literal}");
            let scan = scan(literal, 0);
            assert!(scan.terminated, "{literal} still finds its closing quote");
            assert_eq!(scan.end, literal.len(), "{literal}");
        }
        assert_eq!(
            problems(r#""a\1b\8""#).len(),
            2,
            "every bad escape is reported"
        );
        assert_eq!(
            scan(r#""\x4G""#, 0).cooked,
            "G",
            "the digit the bad `\\x` consumed is part of the escape; the text continues after it"
        );
    }

    #[test]
    fn unterminated_literals_stop_at_the_line_end_or_the_file_end() {
        for (text, end) in [
            ("\"abc", 4),
            ("\"abc\n", 4),
            ("\"abc\r\n", 4),
            ("\"", 1),
            ("\"\\", 2),
            ("\"a\\", 3),
            ("'abc\nx", 4),
        ] {
            let scan = scan(text, 0);
            assert!(!scan.terminated, "{text:?}");
            assert_eq!(scan.end, end, "{text:?}");
            assert!(scan.problems.is_empty(), "{text:?}");
        }
        assert_eq!(
            scan("\"a\\", 0).cooked,
            "a",
            "a trailing backslash cooks nothing"
        );
        assert_eq!(
            scan("\"a\\\nb", 0).cooked,
            "ab",
            "a continuation then no close"
        );
    }

    #[test]
    fn a_literal_scans_from_its_own_offset() {
        let text = "x = 'a\"b' + 1";
        let scan = scan(text, 4);
        assert!(scan.terminated);
        assert_eq!((scan.end, scan.cooked.as_str()), (9, "a\"b"));
    }

    #[test]
    fn double_quoting_changes_only_what_the_quotes_force() {
        assert_eq!(double_quoted("'a'"), "\"a\"");
        assert_eq!(double_quoted("''"), "\"\"");
        assert_eq!(double_quoted(r#"'say "hi"'"#), r#""say \"hi\"""#);
        assert_eq!(double_quoted(r"'it\'s'"), r#""it's""#);
        assert_eq!(
            double_quoted(r#"'a\'b"c'"#),
            r#""a'b\"c""#,
            "both quote kinds"
        );
        assert_eq!(
            double_quoted(r"'\n\t\\\x41\u{41}'"),
            r#""\n\t\\\x41\u{41}""#,
            "other escapes stay raw"
        );
        assert_eq!(
            double_quoted(r"'a\\'"),
            r#""a\\""#,
            "an escaped backslash is not an escaped quote"
        );
        assert_eq!(
            double_quoted("'a\\\nb'"),
            "\"a\\\nb\"",
            "a line continuation stays raw"
        );
    }
}
