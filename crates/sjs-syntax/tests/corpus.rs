//! STORIES.md US-033: the pinned corpus (sjs-testkit's generated programs, seeds
//! `0..10 000` with the default options, pinned by the testkit's manifest snapshot)
//! round-trips through the parser byte for byte and parses clean. CI runs all of it
//! (`SJS_CORPUS_PROGRAMS=10000`, the `test` job and `scripts/gates.sh`); a routine run
//! takes the first thousand.

use sjs_diag::SourceFile;
use sjs_syntax::{parse, NodeKind};
use sjs_testkit::prng::Rng;
use sjs_testkit::program::{Options, ProgramGen};
use sjs_testkit::prop::Gen;

/// How many programs to run: the variable's value, or a thousand.
fn programs(value: Option<&str>) -> u64 {
    match value {
        Some(value) => value.trim().parse().unwrap_or(1_000),
        None => 1_000,
    }
}

#[test]
fn the_program_count_comes_from_the_variable() {
    assert_eq!(programs(None), 1_000);
    assert_eq!(programs(Some(" 10000 ")), 10_000);
    assert_eq!(programs(Some("many")), 1_000);
}

#[test]
fn the_pinned_corpus_round_trips_and_parses_clean() {
    let gen = ProgramGen {
        options: Options::default(),
    };
    let count = programs(std::env::var("SJS_CORPUS_PROGRAMS").ok().as_deref());
    for seed in 0..count {
        let text = gen.generate(&mut Rng::from_seed(seed)).to_string();
        let file = SourceFile::new("corpus.js", text.clone().into_bytes()).expect("utf-8");
        let (tree, diagnostics) = parse(&file);
        assert_eq!(tree.print(&file), text, "seed {seed}");
        assert_eq!(tree.root().kind(), NodeKind::Module);
        assert_eq!(
            diagnostics.len(),
            0,
            "seed {seed}: a corpus program parses clean: {diagnostics:?}\n{text}"
        );
    }
}
