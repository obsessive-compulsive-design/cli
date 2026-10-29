//! STORIES.md US-032: the parser never panics, never overflows the stack and always
//! reproduces its input, on arbitrary bytes (`fuzz_parse`, run for `SJS_FUZZ_SECONDS`),
//! on corpus programs with one token deleted, and on nesting a hundred thousand deep;
//! and it recovers from an error so the next one is still reported.

use sjs_diag::{Code, SourceFile};
use sjs_syntax::{lex, parse, Element, NodeKind, TokenKind};
use sjs_testkit::fuzz;
use sjs_testkit::prng::Rng;
use sjs_testkit::program::{Options, ProgramGen};
use sjs_testkit::prop::Gen;
use std::path::Path;
use std::time::{Duration, Instant};

/// Tokens worth splicing into random input: every construct the grammar treats
/// specially, keywords with the space that keeps them words.
const DICTIONARY: &[&[u8]] = &[
    b"function ",
    b"async ",
    b"class ",
    b"extends ",
    b"const ",
    b"let ",
    b"var ",
    b"if ",
    b"else ",
    b"for ",
    b"of ",
    b"while ",
    b"do ",
    b"switch ",
    b"case ",
    b"default:",
    b"try ",
    b"catch ",
    b"finally ",
    b"return ",
    b"break",
    b"continue",
    b"throw ",
    b"new ",
    b"import ",
    b"export ",
    b"from ",
    b"as ",
    b"await ",
    b"yield ",
    b"get ",
    b"set ",
    b"static ",
    b"super",
    b"this",
    b"typeof ",
    b"instanceof ",
    b"with ",
    b"debugger",
    b"in ",
    b"delete ",
    b"void ",
    b"=>",
    b"...",
    b"?.",
    b"??",
    b"??=",
    b"**",
    b"++",
    b"--",
    b"==",
    b"===",
    b"!=",
    b"||=",
    b"(",
    b")",
    b"[",
    b"]",
    b"{",
    b"}",
    b";",
    b",",
    b":",
    b"?",
    b"=",
    b"+=",
    b"#x",
    b"`",
    b"${",
    b"\"./x.js\"",
    b"\"",
    b"/a/v",
    b"/",
    b"[Symbol.iterator]",
    b"String.raw",
    b"__proto__",
    b"constructor",
    b"prototype",
    b"import(",
    b"import.meta",
    b"new.target",
    b"(a) => ",
    b"a => ",
    b"label:",
    b"undefined",
    b"null",
    b"true",
    b"1n",
    b"0x1",
    b"a",
    b"b",
    b"\n",
    b"// c\n",
    b"/* c */",
];

/// What every input must satisfy, whatever its bytes: the tree prints back to the
/// file under a `Module` root that ends in one `Eof`, its tokens cover the file
/// contiguously on character boundaries, and every diagnostic points inside it.
fn check(bytes: &[u8]) {
    let Ok(file) = SourceFile::new("fuzz.js", bytes.to_vec()) else {
        return;
    };
    let (tree, diagnostics) = parse(&file);
    assert_eq!(tree.print(&file), file.text(), "lossless");
    let root = tree.root();
    assert_eq!(root.kind(), NodeKind::Module);
    let last = root.children().last().expect("the Eof token");
    assert!(
        matches!(last, Element::Token(token) if token.token().kind == TokenKind::Eof),
        "the root ends in Eof"
    );
    let tokens = tree.tokens();
    let eofs = tokens
        .iter()
        .filter(|token| token.kind == TokenKind::Eof)
        .count();
    assert_eq!(eofs, 1, "one Eof");
    let mut end = 0;
    for token in tokens {
        for trivia in &token.leading {
            assert_eq!(trivia.span.byte_start, end, "trivia is contiguous");
            end = trivia.span.byte_end;
        }
        assert_eq!(token.span.byte_start, end, "tokens are contiguous");
        end = token.span.byte_end;
        assert!(file.text().is_char_boundary(end));
    }
    assert_eq!(end, bytes.len(), "every byte is covered");
    for diagnostic in diagnostics.iter() {
        let span = diagnostic.span;
        assert!(span.byte_start <= span.byte_end, "{diagnostic:?}");
        assert!(span.byte_end <= bytes.len(), "{diagnostic:?}");
    }
}

#[test]
fn fuzz_parse() {
    let regressions = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions/parse");
    let report = fuzz::run_or_panic(Some(&regressions), DICTIONARY, check);
    assert!(report.runs > 0);
}

/// A thousand corpus programs, each with one token (chosen per seed) deleted: the
/// parser neither panics nor loses a byte.
#[test]
fn corpus_programs_with_one_token_deleted_parse_without_panic_and_round_trip() {
    let gen = ProgramGen {
        options: Options {
            max_depth: 4,
            max_items: 6,
            doubles: true,
        },
    };
    for seed in 0..1_000u64 {
        let mut rng = Rng::from_seed(seed);
        let text = gen.generate(&mut rng).to_string();
        let file = SourceFile::new("corpus.js", text.clone().into_bytes()).expect("utf-8");
        let (stream, _) = lex(&file);
        let tokens = stream.tokens.len() - 1;
        assert!(tokens > 0, "seed {seed}: a program has tokens");
        let victim = &stream.tokens[rng.index(tokens)];
        let mut mutated = text.clone();
        mutated.replace_range(victim.span.byte_start..victim.span.byte_end, "");
        assert_ne!(mutated, text, "seed {seed}: a token was deleted");
        check(mutated.as_bytes());
    }
}

/// Nesting a hundred thousand deep, in every shape the grammar recurses on: E000 at
/// the bound, and the file printed back, with no stack overflow.
#[test]
fn nesting_a_hundred_thousand_deep_reports_e000_and_returns() {
    let depth = 100_000;
    for (name, prefix, open, close) in [
        ("parentheses", "x = ", "(", ")"),
        ("brackets", "x = ", "[", "]"),
        ("object braces", "x = ", "{a:", "}"),
        ("blocks", "", "{", "}"),
        ("unclosed parentheses", "x = ", "(", ""),
        ("calls", "x = ", "f(", ")"),
        ("negations", "x = ", "-", ""),
        ("arrows", "x = ", "() => ", ""),
        ("powers", "x = ", "a ** ", ""),
        ("constructions", "x = ", "new ", ""),
        ("ifs", "", "if (a) ", ""),
        ("templates", "x = ", "`${", "}`"),
    ] {
        let text = format!("{prefix}{}a{}", open.repeat(depth), close.repeat(depth));
        let file = SourceFile::new("deep.js", text.clone().into_bytes()).expect("utf-8");
        let start = Instant::now();
        let (tree, found) = parse(&file);
        let elapsed = start.elapsed();
        // Linear parsing takes a few seconds at this depth; a quadratic step took over a
        // minute.
        let size = text.len();
        assert!(
            elapsed < Duration::from_secs(30),
            "{name}: {size} bytes took {elapsed:?}"
        );
        assert_eq!(tree.print(&file), text, "{name}");
        let bound = found
            .iter()
            .filter(|d| d.code == Code::E000 && d.message.contains("nest deeper than"))
            .count();
        assert_eq!(bound, 1, "{name}: reported once: {}", found.len());
        check(text.as_bytes());
    }
}

/// `(line, column, message)` of every diagnostic of `text`.
fn diagnostics(text: &str) -> Vec<(u32, u32, String)> {
    let file = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
    check(text.as_bytes());
    parse(&file)
        .1
        .iter()
        .map(|d| (d.span.start.line, d.span.start.column, d.message.clone()))
        .collect()
}

#[test]
fn five_independent_errors_are_five_diagnostics_and_the_file_prints_back() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/conformance/04-statements/E000/five-errors.js");
    let bytes = std::fs::read(&path).expect("the fixture exists");
    check(&bytes);
    let file = SourceFile::new("five-errors.js", bytes).expect("utf-8");
    let (_, found) = parse(&file);
    assert_eq!(found.len(), 5, "{found:?}");
    let lines: Vec<u32> = found.iter().map(|d| d.span.start.line).collect();
    assert_eq!(lines, [8, 9, 10, 11, 12], "one per line, in order");
}

#[test]
fn recovery_leaves_one_report_per_fault_at_the_edges() {
    let found = diagnostics("{");
    assert_eq!(
        found.len(),
        1,
        "unclosed `{{` at the end of the file: {found:?}"
    );
    assert!(found[0].2.contains("expected `}`"), "{found:?}");
    let found = diagnostics(")");
    assert_eq!(found.len(), 1, "a `)` with no opener: {found:?}");
    assert!(found[0].2.contains("expected a statement"), "{found:?}");
    let found = diagnostics("}}}");
    assert_eq!(
        found.len(),
        3,
        "a file of only `}}`: one per brace: {found:?}"
    );
    assert!(
        found.iter().all(|d| d.2.contains("unmatched `}`")),
        "{found:?}"
    );
    let found = diagnostics("x = `${a +}b${}c`\nconst y = 1");
    assert_eq!(
        found.len(),
        2,
        "an error inside a substitution recovers at its end: {found:?}"
    );
    assert_eq!(
        (found[0].0, found[0].1),
        (1, 11),
        "the missing operand after `+`: {found:?}"
    );
    assert_eq!(
        (found[1].0, found[1].1),
        (1, 15),
        "the empty substitution: {found:?}"
    );
    let file = SourceFile::new("t.js", b"x = `${a +}b${}c`\nconst y = 1".to_vec()).expect("utf-8");
    let sexp = parse(&file).0.to_sexp(&file);
    assert!(
        sexp.ends_with("(ConstDecl const y (Initializer = 1)) <eof>)"),
        "{sexp}"
    );
}
