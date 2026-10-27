//! Regex literal tokenization (SPEC.md §2, E002): the body between the slashes, where a
//! class `[…]` hides `/` and a backslash escapes the next character, and the flags after
//! it. Whether a `/` starts a regex at all is the lexer's caller's decision (US-020).
//! Parsing the body under `v`-mode syntax is US-016.

/// What scanning found after the opening `/`.
pub(crate) struct Scan {
    /// After the flags; at the line terminator or the end of the file when unterminated.
    pub end: usize,
    pub kind: ScanKind,
}

pub(crate) enum ScanKind {
    /// The closing `/` was found.
    Closed {
        body: String,
        flags: String,
        /// Byte offset of the first flag (or of whatever follows the closing `/`).
        flags_at: usize,
    },
    /// A line terminator or the end of the file came first.
    Unterminated,
}

/// Scans the literal whose opening `/` is at `start`.
pub(crate) fn scan(text: &str, start: usize) -> Scan {
    let mut pos = start + 1;
    let mut in_class = false;
    loop {
        let Some(ch) = text[pos..].chars().next() else {
            return unterminated(pos);
        };
        match ch {
            '\n' | '\r' | '\u{2028}' | '\u{2029}' => return unterminated(pos),
            '\\' => {
                pos += 1;
                match text[pos..].chars().next() {
                    Some(escaped) if !is_terminator(escaped) => pos += escaped.len_utf8(),
                    _ => return unterminated(pos),
                }
            }
            '[' => {
                in_class = true;
                pos += 1;
            }
            ']' => {
                in_class = false;
                pos += 1;
            }
            '/' if !in_class => {
                let body = text[start + 1..pos].to_owned();
                pos += 1;
                let flags_at = pos;
                while text[pos..]
                    .chars()
                    .next()
                    .is_some_and(|c| c == '$' || c == '_' || c.is_alphanumeric())
                {
                    pos += text[pos..].chars().next().map_or(0, char::len_utf8);
                }
                return Scan {
                    end: pos,
                    kind: ScanKind::Closed {
                        body,
                        flags: text[flags_at..pos].to_owned(),
                        flags_at,
                    },
                };
            }
            _ => pos += ch.len_utf8(),
        }
    }
}

fn unterminated(end: usize) -> Scan {
    Scan {
        end,
        kind: ScanKind::Unterminated,
    }
}

fn is_terminator(ch: char) -> bool {
    matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// The verdict on a flag set.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Flags {
    /// Well formed and including `v`.
    Ok,
    /// E000: the flag at this byte offset within the flags is not acceptable.
    Syntax { offset: usize, message: String },
    /// E002: well formed but `v` is missing.
    MissingV(String),
}

/// Checks `flags` (the text after the closing `/`): each of `d g i m s v y` at most once,
/// `u` never alongside `v`, and `v` present (SPEC.md §2).
pub(crate) fn check_flags(flags: &str) -> Flags {
    let mut seen = String::new();
    for (offset, flag) in flags.char_indices() {
        if !"dgimsuvy".contains(flag) {
            return Flags::Syntax {
                offset,
                message: format!(
                    "`{flag}` is not a regex flag: the flags are `d`, `g`, `i`, `m`, `s`, `v` and `y`"
                ),
            };
        }
        if seen.contains(flag) {
            return Flags::Syntax {
                offset,
                message: format!("the regex flag `{flag}` is repeated"),
            };
        }
        if (flag == 'u' && seen.contains('v')) || (flag == 'v' && seen.contains('u')) {
            return Flags::Syntax {
                offset,
                message: "`u` and `v` cannot be combined: `v` alone is the Unicode mode".to_owned(),
            };
        }
        seen.push(flag);
    }
    if seen.contains('v') {
        Flags::Ok
    } else if flags.is_empty() {
        Flags::MissingV("regex literal without flags: write the flags, including `v`".to_owned())
    } else if seen.contains('u') {
        let fixed = flags.replace('u', "v");
        Flags::MissingV(format!(
            "regex flag `u` is the legacy Unicode mode: write `{fixed}` with `v` instead"
        ))
    } else {
        Flags::MissingV(format!("regex flags `{flags}` lack `v`: write `{flags}v`"))
    }
}

#[cfg(test)]
mod tests {
    use super::{check_flags, scan, Flags, ScanKind};

    fn unterminated_at(text: &str) -> usize {
        try_closed(text, 0).expect_err("unterminated")
    }

    #[test]
    fn the_body_runs_to_the_first_unescaped_slash_outside_a_class() {
        assert_eq!(closed("/a/v", 0), (4, "a".to_owned(), "v".to_owned(), 3));
        assert_eq!(closed("/a/", 0), (3, "a".to_owned(), String::new(), 3));
        assert_eq!(closed("//", 0), (2, String::new(), String::new(), 2));
        assert_eq!(closed("/=/v", 0), (4, "=".to_owned(), "v".to_owned(), 3));
        assert_eq!(
            closed("/[/]/v", 0),
            (6, "[/]".to_owned(), "v".to_owned(), 5)
        );
        assert_eq!(
            closed(r"/\//v", 0),
            (5, r"\/".to_owned(), "v".to_owned(), 4)
        );
        assert_eq!(
            closed(r"/[\]/]/v", 0),
            (8, r"[\]/]".to_owned(), "v".to_owned(), 7)
        );
        assert_eq!(
            closed("/a]b/v", 0),
            (6, "a]b".to_owned(), "v".to_owned(), 5),
            "a stray `]`"
        );
        assert_eq!(
            closed("/é😀/v", 0),
            (9, "é😀".to_owned(), "v".to_owned(), 8)
        );
        assert_eq!(
            closed("/a/dgimsyv.x", 0),
            (10, "a".to_owned(), "dgimsyv".to_owned(), 3)
        );
        assert_eq!(
            closed("/a/v$_1é x", 0),
            (9, "a".to_owned(), "v$_1é".to_owned(), 3)
        );
        assert_eq!(closed("x/a/v", 1).0, 5, "from an offset");
        assert_eq!(scan("x/a/v", 1).end, 5);
    }

    fn closed(text: &str, start: usize) -> (usize, String, String, usize) {
        try_closed(text, start).expect("closed")
    }

    fn try_closed(text: &str, start: usize) -> Result<(usize, String, String, usize), usize> {
        let scan = scan(text, start);
        match scan.kind {
            ScanKind::Closed {
                body,
                flags,
                flags_at,
            } => Ok((scan.end, body, flags, flags_at)),
            ScanKind::Unterminated => Err(scan.end),
        }
    }

    #[test]
    fn a_line_terminator_or_the_file_end_leaves_the_literal_unterminated() {
        assert_eq!(unterminated_at("/a"), 2);
        assert_eq!(unterminated_at("/"), 1);
        assert_eq!(unterminated_at("/a\nb/v"), 2);
        assert_eq!(unterminated_at("/a\r/v"), 2);
        assert_eq!(unterminated_at("/a\u{2028}/v"), 2);
        assert_eq!(unterminated_at("/a\u{2029}/v"), 2);
        assert_eq!(unterminated_at("/[a\n]/v"), 3, "inside a class too");
        assert_eq!(unterminated_at(r"/a\"), 3, "a backslash at the end");
        assert_eq!(
            unterminated_at("/a\\\n/v"),
            3,
            "a backslash before a terminator"
        );
        assert_eq!(
            unterminated_at("/[/"),
            3,
            "an unclosed class hides the slash"
        );
    }

    #[test]
    fn flags_are_each_of_the_seven_at_most_once_and_must_include_v() {
        assert_eq!(check_flags("v"), Flags::Ok);
        assert_eq!(check_flags("dgimsyv"), Flags::Ok);
        assert_eq!(check_flags("vgi"), Flags::Ok);
        assert_eq!(check_flags("ysmigdv"), Flags::Ok);
        assert_eq!(
            check_flags(""),
            Flags::MissingV(
                "regex literal without flags: write the flags, including `v`".to_owned()
            )
        );
        assert_eq!(
            check_flags("u"),
            Flags::MissingV(
                "regex flag `u` is the legacy Unicode mode: write `v` with `v` instead".to_owned()
            )
        );
        assert_eq!(
            check_flags("gu"),
            Flags::MissingV(
                "regex flag `u` is the legacy Unicode mode: write `gv` with `v` instead".to_owned()
            )
        );
        assert_eq!(
            check_flags("gi"),
            Flags::MissingV("regex flags `gi` lack `v`: write `giv`".to_owned())
        );
    }

    #[test]
    fn a_bad_flag_is_a_syntax_problem_at_its_offset() {
        let not_a_flag = |flag: char| {
            format!(
                "`{flag}` is not a regex flag: the flags are `d`, `g`, `i`, `m`, `s`, `v` and `y`"
            )
        };
        assert_eq!(
            check_flags("vx"),
            Flags::Syntax {
                offset: 1,
                message: not_a_flag('x')
            }
        );
        assert_eq!(
            check_flags("xv"),
            Flags::Syntax {
                offset: 0,
                message: not_a_flag('x')
            }
        );
        assert_eq!(
            check_flags("v$"),
            Flags::Syntax {
                offset: 1,
                message: not_a_flag('$')
            }
        );
        assert_eq!(
            check_flags("vé"),
            Flags::Syntax {
                offset: 1,
                message: not_a_flag('é')
            }
        );
        assert_eq!(
            check_flags("vv"),
            Flags::Syntax {
                offset: 1,
                message: "the regex flag `v` is repeated".to_owned()
            }
        );
        assert_eq!(
            check_flags("gvg"),
            Flags::Syntax {
                offset: 2,
                message: "the regex flag `g` is repeated".to_owned()
            }
        );
        let combined = "`u` and `v` cannot be combined: `v` alone is the Unicode mode".to_owned();
        assert_eq!(
            check_flags("vu"),
            Flags::Syntax {
                offset: 1,
                message: combined.clone()
            }
        );
        assert_eq!(
            check_flags("uv"),
            Flags::Syntax {
                offset: 1,
                message: combined.clone()
            }
        );
        assert_eq!(
            check_flags("ugv"),
            Flags::Syntax {
                offset: 2,
                message: combined
            }
        );
        assert_eq!(
            check_flags("uu"),
            Flags::Syntax {
                offset: 1,
                message: "the regex flag `u` is repeated".to_owned()
            },
            "a repeat is reported before the combination"
        );
    }
}
