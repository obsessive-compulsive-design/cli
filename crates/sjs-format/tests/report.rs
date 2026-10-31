//! STORIES.md US-048: over generated programs with comments, every change the layout
//! makes is claimed by a finding, and the unified diff of a file applies with `patch -p0`
//! to give the layout's text. `SJS_CORPUS_PROGRAMS` sets how many programs; a routine
//! run takes three hundred for the findings and a hundred for `patch`.

mod common;

use common::{file, module};
use sjs_format::{unified_diff, Formatter};
use sjs_syntax::parse;
use sjs_testkit::fs::TempDir;
use sjs_testkit::prng::Rng;
use sjs_testkit::process::run_with_stdin;
use sjs_testkit::program::{Options, ProgramGen};
use sjs_testkit::prop::Gen;

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

/// Applies the unified diff of `text` to it with `patch -p0` and returns the result.
fn patched(text: &str) -> String {
    let dir = TempDir::new("us048-patch").expect("a temporary directory");
    dir.write("a.js", text).expect("the file");
    let diff = unified_diff("a.js", text, &module(text));
    if diff.is_empty() {
        return text.to_owned();
    }
    let output = run_with_stdin("patch", &["-p0"], Some(dir.path()), Some(diff.as_bytes()))
        .expect("patch runs");
    assert!(
        output.success(),
        "patch failed: {}\n{}\n{diff}",
        output.stdout,
        output.stderr
    );
    std::fs::read_to_string(dir.path().join("a.js")).expect("the patched file")
}

#[test]
fn every_change_the_layout_makes_is_claimed_by_a_finding() {
    let gen = generator();
    let count = programs(std::env::var("SJS_CORPUS_PROGRAMS").ok().as_deref());
    for seed in 0..count {
        let text = gen.generate(&mut Rng::from_seed(seed)).to_string();
        let f = file(&text);
        let (tree, parsed) = parse(&f);
        let report = Formatter::new(&f, &tree).report(&parsed);
        assert!(
            report.unclassified.is_empty(),
            "seed {seed}: a change no pass claims: {:?}\n{text}",
            report.unclassified
        );
        let canonical = module(&text);
        let g = file(&canonical);
        let (tree, parsed) = parse(&g);
        let again = Formatter::new(&g, &tree).report(&parsed);
        let fixable: Vec<_> = again
            .findings
            .iter()
            .filter(|d| d.replacement.is_some())
            .collect();
        assert!(
            fixable.is_empty() && parsed.is_empty(),
            "seed {seed}: the canonical text has findings with a replacement: {fixable:?}\n{canonical}"
        );
    }
}

#[test]
fn the_unified_diff_applies_with_patch_to_give_the_layout() {
    let gen = generator();
    let count = programs(std::env::var("SJS_CORPUS_PROGRAMS").ok().as_deref()).min(100);
    for seed in 0..count {
        let text = gen.generate(&mut Rng::from_seed(seed)).to_string();
        assert_eq!(patched(&text), module(&text), "seed {seed}");
    }
    for text in [
        "f()",
        "const a = 1\n",
        "",
        "f('a',)\n\n\n",
        "import { b } from './b.js'\nimport { a } from './a.js'\nclass A {\n  m() {}\n  a = 1\n}\n",
    ] {
        assert_eq!(patched(text), module(text), "{text:?}");
    }
}

#[test]
fn a_canonical_file_has_no_diff_and_no_findings() {
    let text = "const a = 1\n\nf(a)\n";
    assert_eq!(unified_diff("a.js", text, &module(text)), "");
    let f = file(text);
    let (tree, parsed) = parse(&f);
    assert_eq!(
        Formatter::new(&f, &tree).report(&parsed).findings,
        Vec::new()
    );
}
