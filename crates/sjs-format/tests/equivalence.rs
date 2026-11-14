//! STORIES.md US-050: the layout is idempotent, `fmt(fmt(x)) == fmt(x)`, and changes
//! nothing but layout, `strip(parse(fmt(x))) == strip(parse(x))`, where `strip`
//! (`common::essence`) drops trivia, parentheses and what the layout may change. Both
//! hold on the pinned corpus with comments (seeds `0..SJS_CORPUS_PROGRAMS`, ten thousand
//! in CI, pinned by the testkit's `program-corpus-comments-manifest`), on every
//! conformance fixture, and on whatever `fuzz_format` generates that has no E000; the
//! layout never panics on anything.

mod common;

use common::{essence, file, module};
use sjs_diag::SourceFile;
use sjs_syntax::lex;
use sjs_testkit::conformance::discover;
use sjs_testkit::fuzz;
use sjs_testkit::prng::Rng;
use sjs_testkit::program::{Options, ProgramGen};
use sjs_testkit::prop::Gen;
use std::path::Path;

/// Lays `text` out, which must not panic; when it has no E000, both properties.
fn holds(text: &str, name: &str) {
    let out = module(text);
    let Some(before) = essence(text) else {
        return;
    };
    assert_eq!(
        module(&out),
        out,
        "{name}: the layout of the layout differs\n{text}"
    );
    assert_eq!(
        essence(&out),
        Some(before),
        "{name}: the layout changed the program\n{text}\n---\n{out}"
    );
}

fn programs(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(300)
}

fn generator() -> ProgramGen {
    ProgramGen {
        options: Options {
            comments: true,
            ..Options::default()
        },
    }
}

#[test]
fn the_essence_ignores_layout_and_nothing_else() {
    let same = |a: &str, b: &str| assert_eq!(essence(a), essence(b), "{a:?} / {b:?}");
    same("f((a), 'b',);\n;", "f(a, \"b\")");
    same("const f = () => { return 1; }", "const f = () => 1");
    same(
        "import { b, a } from './b.js'\nimport { c } from './a.js'\nf()",
        "import { c } from \"./a.js\"\nimport { a, b } from \"./b.js\"\n\nf() // c",
    );
    same(
        "class A { m() {} a = 1; [Symbol.iterator]() {} }",
        "class A { a = 1;\n[Symbol.iterator]() {}\nm() {} }",
    );
    same(
        "for (let i = 0; i < n; i++) {}",
        "for (let i = 0;i < n;i++) {}",
    );
    for (a, b) in [
        ("f(a, b)", "f(b, a)"),
        ("x = 'a'", "x = 'b'"),
        ("const f = () => { g() }", "const f = () => g()"),
        (
            "f()\nimport { a } from './a.js'",
            "import { a } from './a.js'\nf()",
        ),
        ("a + b * c", "(a + b) * c"),
        (
            "for (let i = 0; i < n; i++) {}",
            "for (let i = 0; i < n; i++) {}
;x",
        ),
        ("[a, , b]", "[a, b]"),
    ] {
        assert_ne!(essence(a), essence(b), "{a:?} / {b:?}");
    }
    assert_eq!(essence("let x = ;"), None, "E000: nothing is claimed");
    assert_eq!(programs(None), 300);
    assert_eq!(programs(Some(" 7 ")), 7);
    assert_eq!(programs(Some("x")), 300);
}

#[test]
fn the_corpus_with_comments_is_laid_out_idempotently_and_unchanged() {
    let gen = generator();
    let count = programs(std::env::var("SJS_CORPUS_PROGRAMS").ok().as_deref());
    for seed in 0..count {
        let text = gen.generate(&mut Rng::from_seed(seed)).to_string();
        assert!(essence(&text).is_some(), "seed {seed} parses clean");
        holds(&text, &format!("seed {seed}"));
    }
}

#[test]
fn every_fixture_is_laid_out_idempotently_and_unchanged() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance");
    let mut clean = 0;
    for fixture in discover(&root).expect("the fixtures") {
        for source in &fixture.files {
            if SourceFile::new("a.js", source.bytes.clone()).is_err() {
                continue;
            }
            let text = &*String::from_utf8_lossy(&source.bytes);
            clean += usize::from(essence(text).is_some());
            holds(text, &format!("{} {}", fixture.name, source.relative));
        }
    }
    assert!(clean > 100, "{clean} fixtures without E000");
}

/// A thousand corpus programs, each with one token (chosen per seed) deleted: the
/// layout does not panic, and both properties hold on those that still parse.
#[test]
fn corpus_programs_with_one_token_deleted_keep_both_properties() {
    let gen = generator();
    for seed in 0..1_000u64 {
        let mut rng = Rng::from_seed(seed);
        let mut text = gen.generate(&mut rng).to_string();
        let (stream, _) = lex(&file(&text));
        let victim = stream.tokens[rng.index(stream.tokens.len() - 1)].span;
        text.replace_range(victim.byte_start..victim.byte_end, "");
        holds(&text, &format!("seed {seed} less a token"));
    }
}

/// Tokens worth splicing into random input: what the layout treats specially.
const DICTIONARY: &[&[u8]] = &[
    b"function f(a) {",
    b"async ",
    b"class A {",
    b"extends ",
    b"const ",
    b"let ",
    b"if (a) {",
    b"} else {",
    b"for (const x of xs) {",
    b"while (a) {",
    b"switch (a) {",
    b"case 1: {",
    b"try {",
    b"} catch {",
    b"return ",
    b"throw ",
    b"new ",
    b"import { a } from \"./a.js\"\n",
    b"import * as n from \"../n.js\"\n",
    b"export ",
    b"await ",
    b"yield ",
    b"get ",
    b"set ",
    b"static ",
    b"constructor() {",
    b"=>",
    b"...",
    b"?.",
    b"??",
    b"**",
    b"++",
    b"===",
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
    b"+",
    b"-",
    b".",
    b"#x",
    b"`",
    b"${",
    b"'s'",
    b"\"d\"",
    b"/a/v",
    b"[Symbol.iterator]() {",
    b"import(",
    b"(a) => ",
    b"a.b().c().d()",
    b"x",
    b"y",
    b"1",
    b"\n",
    b"\n\n",
    b"  ",
    b"// c\n",
    b"//c",
    b"/* c */",
    b"/** d */\n",
    b"/**\n * d\n * @param a  b\n */\n",
];

#[test]
fn fuzz_format() {
    let regressions = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fuzz-regressions/format");
    let report = fuzz::run_or_panic(Some(&regressions), DICTIONARY, |bytes| {
        // A file that is not UTF-8, or begins with a byte-order mark, never reaches the
        // layout.
        if SourceFile::new("a.js", bytes.to_vec()).is_ok() {
            holds(&String::from_utf8_lossy(bytes), "fuzz");
        }
    });
    assert!(report.runs > 0);
}
