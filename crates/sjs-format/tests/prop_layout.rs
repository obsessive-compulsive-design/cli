//! STORIES.md US-040: whatever the document, no laid-out line exceeds the width except
//! one holding a single over-long token, and no line ends in whitespace.

use sjs_format::{print, Doc, WIDTH};
use sjs_testkit::prng::Rng;
use sjs_testkit::prop::{check_with, Config, Gen};

/// Random documents: tokens separated by breaks, in groups and indents nested a few
/// deep, with hard breaks and trailing-comma conditionals; one token in twenty is wider
/// than the line.
struct Docs;

impl Gen for Docs {
    type Value = Vec<Doc>;

    fn generate(&self, rng: &mut Rng) -> Vec<Doc> {
        sequence(rng, 0)
    }

    fn shrink(&self, value: &Vec<Doc>) -> Vec<Vec<Doc>> {
        (0..value.len())
            .map(|at| {
                let mut shorter = value.clone();
                shorter.remove(at);
                shorter
            })
            .collect()
    }
}

fn sequence(rng: &mut Rng, depth: u32) -> Vec<Doc> {
    let count = rng.index(6);
    let mut docs = Vec::new();
    for at in 0..count {
        if at > 0 {
            docs.push(if rng.chance(1, 2) {
                Doc::Line
            } else {
                Doc::SoftLine
            });
        }
        docs.push(item(rng, depth));
    }
    docs
}

fn item(rng: &mut Rng, depth: u32) -> Doc {
    let choices = if depth < 4 { 6 } else { 2 };
    match rng.below(choices) {
        0 => Doc::text(token(rng)),
        1 => {
            if rng.chance(1, 8) {
                Doc::HardLine
            } else {
                Doc::text(token(rng))
            }
        }
        2 => Doc::indent(sequence(rng, depth + 1)),
        3 => Doc::if_break(vec![Doc::text(",")], Vec::new()),
        _ => Doc::group(sequence(rng, depth + 1)),
    }
}

/// A token: a run of one character, sometimes wider than the line, sometimes multibyte.
fn token(rng: &mut Rng) -> String {
    let len = if rng.chance(1, 20) {
        WIDTH + 1 + rng.index(30)
    } else {
        1 + rng.index(30)
    };
    let unit = if rng.chance(1, 4) { "é" } else { "x" };
    unit.repeat(len)
}

fn holds(docs: &[Doc]) -> Result<(), String> {
    let out = print(docs);
    for line in out.split('\n') {
        if line.ends_with(' ') {
            return Err(format!("trailing whitespace on {line:?}"));
        }
        let columns = line.chars().count();
        let tokens = line.trim_start().split(' ').count();
        if columns > WIDTH && tokens > 1 {
            return Err(format!(
                "{columns} columns across {tokens} tokens: {line:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn no_line_exceeds_the_width_except_one_holding_a_single_over_long_token() {
    check_with(&Config::from_env(), &Docs, &|docs: &Vec<Doc>| holds(docs))
        .expect("the layout invariant holds");
}

#[test]
fn the_generator_shrinks_by_dropping_one_document() {
    let docs = vec![Doc::text("a"), Doc::Line, Doc::text("b")];
    let shorter = Docs.shrink(&docs);
    assert_eq!(shorter.len(), 3);
    assert_eq!(shorter[0], vec![Doc::Line, Doc::text("b")]);
    assert_eq!(Docs.shrink(&Vec::new()), Vec::<Vec<Doc>>::new());
    assert_eq!(holds(&docs), Ok(()));
    let wide = Doc::text("x".repeat(WIDTH + 1));
    assert_eq!(
        holds(std::slice::from_ref(&wide)),
        Ok(()),
        "one over-long token may exceed"
    );
    assert!(
        holds(&[wide, Doc::text(" "), Doc::text("y")]).is_err(),
        "two tokens may not"
    );
    assert!(holds(&[Doc::text("a ")]).is_ok(), "the layout trims");
}
