//! STORIES.md US-019: the lexer never panics, never hangs and always reproduces its
//! input, on arbitrary bytes (`fuzz_lex`, run for `SJS_FUZZ_SECONDS`) and on inputs
//! built to be hostile: megabytes of slashes and unterminated strings, template
//! substitutions nested ten thousand deep.

use sjs_diag::SourceFile;
use sjs_syntax::{lex, regex_allowed_after, Lexer, TokenKind};
use sjs_testkit::fuzz;
use std::path::Path;
use std::time::{Duration, Instant};

/// Tokens worth splicing into random input: every construct the lexer treats specially.
const DICTIONARY: &[&[u8]] = &[
    b"\"",
    b"'",
    b"`",
    b"${",
    b"}",
    b"{",
    b"/",
    b"/=",
    b"/*",
    b"*/",
    b"//",
    b"/**",
    b"\\",
    b"\\u{",
    b"\\u",
    b"\\x",
    b"#!",
    b"#",
    b"@",
    b"$",
    b"0x",
    b"0o",
    b"0b",
    b"1e",
    b"1n",
    b"_",
    b".",
    b"...",
    b"?.",
    b"??=",
    b">>>=",
    b"=>",
    b"var ",
    b"delete",
    b"in",
    b"null",
    b"true",
    b"/a/v",
    b"/[/",
    b"(?<n>",
    b"\\k<",
    b"\\p{",
    b"[[",
    b"--",
    b"&&",
    b"\\q{",
    b"\n",
    b"\r\n",
    b"\r",
    "\u{2028}".as_bytes(),
    "\u{FEFF}".as_bytes(),
    "é".as_bytes(),
    "😀".as_bytes(),
    &[0xFF],
    &[0xC0, 0x80],
];

/// What every input must satisfy, whatever its bytes.
fn check(bytes: &[u8]) {
    let Ok(file) = SourceFile::new("fuzz.js", bytes.to_vec()) else {
        return;
    };
    let (stream, _) = lex(&file);
    let printed = stream.print(&file);
    assert_eq!(printed, file.text(), "lossless");
    let last = stream.tokens.len() - 1;
    assert_eq!(stream.tokens[last].kind, TokenKind::Eof);
    assert!(stream.tokens[..last]
        .iter()
        .all(|t| t.kind != TokenKind::Eof));
    let mut end = 0;
    for token in &stream.tokens {
        for trivia in &token.leading {
            assert_eq!(trivia.span.byte_start, end, "trivia is contiguous");
            end = trivia.span.byte_end;
        }
        assert_eq!(token.span.byte_start, end, "tokens are contiguous");
        end = token.span.byte_end;
        assert!(file.text().is_char_boundary(end));
    }
    assert_eq!(end, bytes.len(), "every byte is covered");
    // The incremental lexer agrees with `lex` under its own guess and never panics
    // under the opposite hint either.
    let mut lexer = Lexer::new(&file);
    let mut previous = TokenKind::Eof;
    for token in &stream.tokens {
        let next = lexer.next_token(regex_allowed_after(&previous));
        assert_eq!(next, *token);
        previous = next.kind;
    }
    let mut contrary = Lexer::new(&file);
    let mut previous = TokenKind::Eof;
    loop {
        let token = contrary.next_token(!regex_allowed_after(&previous));
        if token.kind == TokenKind::Eof {
            break;
        }
        previous = token.kind;
    }
    let _ = contrary.finish();
}

#[test]
fn fuzz_lex() {
    let regressions = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions");
    let report = fuzz::run_or_panic(Some(&regressions), DICTIONARY, check);
    assert!(report.runs > 0);
}

#[test]
fn template_substitutions_nested_ten_thousand_deep_do_not_overflow_the_stack() {
    let depth = 10_000;
    let text = format!("{}a{}", "`${".repeat(depth), "}`".repeat(depth));
    check(text.as_bytes());
    let file = SourceFile::new("deep.js", text.clone().into_bytes()).expect("utf-8");
    let (stream, found) = lex(&file);
    assert!(found.is_empty());
    assert_eq!(
        stream.tokens.len(),
        2 * depth + 2,
        "head × depth, `a`, tail × depth, Eof"
    );
    let unclosed = "`${".repeat(depth);
    check(unclosed.as_bytes());
    let file = SourceFile::new("open.js", unclosed.into_bytes()).expect("utf-8");
    let (_, found) = lex(&file);
    assert_eq!(found.len(), depth, "one E000 per `${{` left open");
}

fn lex_time(text: &str) -> Duration {
    let file = SourceFile::new("big.js", text.as_bytes().to_vec()).expect("utf-8");
    let start = Instant::now();
    let (stream, _) = lex(&file);
    let elapsed = start.elapsed();
    assert_eq!(stream.eof().kind, TokenKind::Eof);
    elapsed
}

/// A hostile input lexes in time linear in its length: ten times the bytes take at most
/// twenty times as long (plus a fixed allowance for timer noise), and never more than a
/// fixed bound. The large size is 10 MB when `SJS_FUZZ_SECONDS` is set (the fuzz job and
/// `scripts/gates.sh`) and 1 MB otherwise, so the routine test run stays quick.
#[test]
fn lexing_time_is_linear_in_the_input_length() {
    let full = std::env::var_os("SJS_FUZZ_SECONDS").is_some();
    let megabyte = if full { 1 << 20 } else { 1 << 20 >> 3 };
    let large_bound = Duration::from_secs(if full { 120 } else { 30 });
    let shapes: [(&str, &str); 4] = [
        ("slashes", "/ "),
        ("unterminated strings", "\"\n"),
        ("unterminated regexes", "/a\n"),
        ("open substitutions", "`${"),
    ];
    for (name, unit) in shapes {
        let small = unit.repeat(megabyte / unit.len());
        let large = unit.repeat(10 * megabyte / unit.len());
        let size = large.len();
        let t_small = lex_time(&small);
        let t_large = lex_time(&large);
        let bound = t_small * 20 + Duration::from_millis(500);
        assert!(
            t_large <= bound,
            "{name}: {t_small:?} for a tenth of {size} bytes, {t_large:?} for all of them, more than {bound:?}"
        );
        assert!(
            t_large < large_bound,
            "{name}: {size} bytes took {t_large:?}"
        );
    }
}
