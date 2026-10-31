//! STORIES.md US-046: generated programs with comments inserted (US-009's mode) keep
//! every comment through the layout (an import's moving with it, US-045), lay out to the
//! same program as without them, and format to a fixed point. `SJS_CORPUS_PROGRAMS` sets how many; a routine run
//! takes three hundred.

mod common;

use common::{canonical, file, module};
use sjs_syntax::{parse, TriviaKind};
use sjs_testkit::prng::Rng;
use sjs_testkit::program::{Options, Program, ProgramGen};
use sjs_testkit::prop::Gen;

/// How many programs to run: the variable's value, or three hundred.
fn programs(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(300)
}

/// Every comment of `text`, sorted, as its characters without layout: `//x` and `// x`
/// read the same, and so do a doc comment and its reflow.
fn comments(text: &str) -> Vec<String> {
    let f = file(text);
    let (tree, _) = parse(&f);
    let mut comments: Vec<String> = tree
        .tokens()
        .iter()
        .flat_map(|token| token.leading.iter())
        .filter(|trivia| {
            !matches!(
                trivia.kind,
                TriviaKind::Whitespace | TriviaKind::LineTerminator(_)
            )
        })
        .map(|trivia| gist(f.slice(&trivia.span).unwrap_or("")))
        .collect();
    comments.sort();
    comments
}

fn gist(comment: &str) -> String {
    comment
        .lines()
        .map(|line| {
            let line = line.trim_start();
            let body = if line.starts_with("*/") {
                line
            } else {
                line.strip_prefix('*').unwrap_or(line)
            };
            body.split_whitespace().collect::<String>()
        })
        .collect()
}

#[test]
fn the_program_count_comes_from_the_variable() {
    assert_eq!(programs(None), 300);
    assert_eq!(programs(Some(" 10 ")), 10);
    assert_eq!(programs(Some("many")), 300);
}

#[test]
fn the_gist_of_a_comment_ignores_its_layout() {
    assert_eq!(gist("//x"), gist("// x"));
    assert_eq!(
        gist("/**\n * one two\n * three\n */"),
        gist("/**\n * one two three\n */")
    );
    assert_ne!(gist("// a"), gist("// b"));
}

#[test]
fn generated_programs_with_comments_keep_them_and_reach_a_fixed_point() {
    let gen = ProgramGen {
        options: Options {
            comments: true,
            ..Options::default()
        },
    };
    let count = programs(std::env::var("SJS_CORPUS_PROGRAMS").ok().as_deref());
    for seed in 0..count {
        let program = gen.generate(&mut Rng::from_seed(seed));
        let text = program.to_string();
        let plain = Program {
            items: program.items,
            comments: Vec::new(),
        }
        .to_string();
        let out = module(&text);
        let (_, diagnostics) = parse(&file(&out));
        assert!(
            diagnostics.is_empty(),
            "seed {seed}: the layout parses clean: {diagnostics:?}\n{out}"
        );
        assert_eq!(
            canonical(&out),
            canonical(&module(&plain)),
            "seed {seed}: the same program as without comments\n{text}\n---\n{out}"
        );
        assert_eq!(
            comments(&out),
            comments(&text),
            "seed {seed}: every comment\n{text}\n---\n{out}"
        );
        assert_eq!(module(&out), out, "seed {seed}: a fixed point\n{out}");
    }
}
