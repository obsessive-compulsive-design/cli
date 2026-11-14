//! Whether a file is JSON (RFC 8259). A JSON module's value is `unknown` (SPEC.md §5.1),
//! so the loader needs no more of it than that it is one.

/// Whether `bytes` are one JSON value, as UTF-8 without a byte order mark, with nothing
/// but whitespace around it. Nesting is followed with a stack, so its depth is bounded
/// by the length of the file alone.
#[must_use]
pub fn is_json(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok() && document(bytes)
}

fn document(b: &[u8]) -> bool {
    // The closing bracket of each container that is open.
    let mut open: Vec<u8> = Vec::new();
    let mut at = 0;
    loop {
        at = blank(b, at);
        let ended = match b.get(at) {
            Some(b'{') if b.get(blank(b, at + 1)) == Some(&b'}') => Some(blank(b, at + 1) + 1),
            Some(b'[') if b.get(blank(b, at + 1)) == Some(&b']') => Some(blank(b, at + 1) + 1),
            Some(b'{') => {
                open.push(b'}');
                key(b, at + 1)
            }
            Some(b'[') => {
                open.push(b']');
                at += 1;
                continue;
            }
            Some(b'"') => string(b, at),
            _ => scalar(b, at),
        };
        let Some(end) = ended else {
            return false;
        };
        at = end;
        if open.last() == Some(&b'}') && b.get(at.wrapping_sub(1)) == Some(&b':') {
            // After a key: its value follows.
            continue;
        }
        // A value has ended: close what it ends, or go on to the next member.
        loop {
            at = blank(b, at);
            let Some(&close) = open.last() else {
                return at == b.len();
            };
            match b.get(at) {
                Some(b',') if close == b'}' => {
                    let Some(end) = key(b, at + 1) else {
                        return false;
                    };
                    at = end;
                    break;
                }
                Some(b',') => {
                    at += 1;
                    break;
                }
                Some(&byte) if byte == close => {
                    open.pop();
                    at += 1;
                }
                _ => return false,
            }
        }
    }
}

/// The offset after the whitespace at `at`.
fn blank(b: &[u8], at: usize) -> usize {
    at + b[at..]
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .count()
}

/// The offset after `"key" :` at (the whitespace before) `at`.
fn key(b: &[u8], at: usize) -> Option<usize> {
    let colon = blank(b, string(b, blank(b, at))?);
    (b.get(colon) == Some(&b':')).then_some(colon + 1)
}

/// The offset after the string that begins at `at`, if one does.
fn string(b: &[u8], at: usize) -> Option<usize> {
    if b.get(at) != Some(&b'"') {
        return None;
    }
    let mut at = at + 1;
    loop {
        match *b.get(at)? {
            b'"' => return Some(at + 1),
            b'\\' => match *b.get(at + 1)? {
                b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => at += 2,
                b'u' if b.get(at + 2..at + 6)?.iter().all(u8::is_ascii_hexdigit) => at += 6,
                _ => return None,
            },
            0..=0x1f => return None,
            _ => at += 1,
        }
    }
}

/// The offset after the `true`, `false`, `null` or number at `at`, if one is there.
fn scalar(b: &[u8], at: usize) -> Option<usize> {
    let rest = &b[at..];
    if let Some(word) = [&b"true"[..], b"false", b"null"]
        .into_iter()
        .find(|word| rest.starts_with(word))
    {
        return Some(at + word.len());
    }
    let digits = |from: usize| {
        rest[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = usize::from(rest.first() == Some(&b'-'));
    let whole = digits(end);
    if whole == 0 || (whole > 1 && rest[end] == b'0') {
        return None;
    }
    end += whole;
    if rest.get(end) == Some(&b'.') {
        let fraction = digits(end + 1);
        if fraction == 0 {
            return None;
        }
        end += 1 + fraction;
    }
    if matches!(rest.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(rest.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(end + 1 + sign);
        if exponent == 0 {
            return None;
        }
        end += 1 + sign + exponent;
    }
    Some(at + end)
}

#[cfg(test)]
mod tests {
    use super::is_json;

    #[test]
    fn every_kind_of_value_is_json() {
        for text in [
            "null",
            "true",
            "false",
            "0",
            "-0",
            "10",
            "-12.50",
            "1e9",
            "1E+9",
            "2.5e-3",
            "\"\"",
            "\"\\\\\"",
            "\"\\n\"",
            "\"a\\\"b\\\\c\\/d\\b\\f\\n\\r\\t\\u00e9\\uABCD\"",
            "\"\u{e9}\"",
            "[]",
            "[ ]",
            "{}",
            "{ }",
            "[1]",
            "[1, \"a\", [true, [null]], {}]",
            "{\"a\": 1}",
            "{\"a\": {\"b\": [1, {\"c\": []}]}, \"d\" : \"e\"}",
            " \t\r\n{\"a\":[{\"b\":1},2]}\n",
            "[[], {}, [{}]]",
            "{\"a\": [], \"b\": {}}",
        ] {
            assert!(is_json(text.as_bytes()), "{text}");
        }
    }

    #[test]
    fn everything_else_is_not() {
        for text in [
            "",
            " ",
            "nul",
            "True",
            "truex",
            "1 2",
            "01",
            "-",
            "-01",
            "+1",
            ".5",
            "1.",
            "1.e2",
            "1e",
            "1e+",
            "0x10",
            "'a'",
            "\"a",
            "\"a\\\"",
            "\"\\x\"",
            "\"\\u12\"",
            "\"\\u12g4\"",
            "\"\\",
            "\"a\nb\"",
            "\"a\u{1f}\"",
            "[",
            "]",
            "[1",
            "[1,]",
            "[,1]",
            "[1 2]",
            "[1}",
            "{",
            "}",
            "{\"a\"}",
            "{\"a\":}",
            "{\"a\":1,}",
            "{\"a\":1 \"b\":2}",
            "{a:1}",
            "{1:1}",
            "{\"a\":1]",
            "{\"a\" 1}",
            "{,}",
            "{\"a\":1,\"b\"}",
            "{\"a\":1,2}",
            "[1,\"a\":2]",
            "[\"a\":1]",
            "{} {}",
            "[] x",
            "{}x",
            "[]]",
            "// c\n1",
            "\u{feff}1",
            "{\"a\":",
            "[\"a\",",
            "tru",
            "-e",
            "1e-",
            "1.5.2",
        ] {
            assert!(!is_json(text.as_bytes()), "{text}");
        }
        assert!(!is_json(b"\"\xff\""), "not UTF-8");
    }

    #[test]
    fn nesting_is_not_bounded_by_the_stack() {
        let deep = format!("{}{}", "[".repeat(200_000), "]".repeat(200_000));
        assert!(is_json(deep.as_bytes()));
        assert!(!is_json(&deep.as_bytes()[1..]));
        let objects = format!("{}1{}", "{\"a\":".repeat(100_000), "}".repeat(100_000));
        assert!(is_json(objects.as_bytes()));
    }
}
