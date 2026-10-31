//! Diffs (US-048, SPEC.md §9.6, §13.4): the shortest edit script between two sequences
//! (Myers 1986), and the unified diff of a file against its canonical text, which `sjs
//! format` prints and `patch -p0` applies.

use std::fmt::Write as _;
use std::ops::Range;

/// One step of an edit script: an element both sides share, one only the first has, or
/// one only the second has, by their indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edit {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// The shortest edit script from `before` to `after`, in order: a shared prefix and
/// suffix are taken as they are, and Myers' search runs on what lies between.
pub(crate) fn edits<T: PartialEq>(before: &[T], after: &[T]) -> Vec<Edit> {
    let prefix = before.iter().zip(after).take_while(|(x, y)| x == y).count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let mut out: Vec<Edit> = (0..prefix).map(|at| Edit::Equal(at, at)).collect();
    let middle = myers(
        &before[prefix..before.len() - suffix],
        &after[prefix..after.len() - suffix],
    );
    out.extend(middle.into_iter().map(|edit| match edit {
        Edit::Equal(x, y) => Edit::Equal(x + prefix, y + prefix),
        Edit::Delete(x) => Edit::Delete(x + prefix),
        Edit::Insert(y) => Edit::Insert(y + prefix),
    }));
    let (len_before, len_after) = (before.len(), after.len());
    out.extend(
        (0..suffix).map(|at| Edit::Equal(len_before - suffix + at, len_after - suffix + at)),
    );
    out
}

/// Myers' greedy search: for each edit distance, the furthest point reached on each
/// diagonal, kept round by round to walk the shortest script back. A diagonal `k` is
/// stored at `k + max`, so every index is a `usize`; the diagonal past the top one of a
/// round holds zero, so comparing with it chooses the diagonal below, as it should.
fn myers<T: PartialEq>(before: &[T], after: &[T]) -> Vec<Edit> {
    let (len_before, len_after) = (before.len(), after.len());
    let max = len_before + len_after;
    let mut furthest = vec![0usize; 2 * max + 2];
    let mut rounds: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut distance = 0;
    'search: for round in 0..=max {
        let low = max - round;
        rounds.push((low, furthest[low..=max + round + 1].to_vec()));
        let mut kk = low;
        loop {
            let from = if kk == low || furthest[kk - 1] < furthest[kk + 1] {
                furthest[kk + 1]
            } else {
                furthest[kk - 1] + 1
            };
            let (mut x, mut y) = (from, from + max - kk);
            while x < len_before && y < len_after && before[x] == after[y] {
                x += 1;
                y += 1;
            }
            furthest[kk] = x;
            if x >= len_before && y >= len_after {
                distance = round;
                break 'search;
            }
            if kk >= max + round {
                break;
            }
            kk += 2;
        }
    }
    backtrack(&rounds, distance, max, (len_before, len_after))
}

/// The script, walked back from the end over the rounds of the search.
fn backtrack(
    rounds: &[(usize, Vec<usize>)],
    distance: usize,
    max: usize,
    (len_before, len_after): (usize, usize),
) -> Vec<Edit> {
    let mut edits = Vec::new();
    let (mut x, mut y) = (len_before, len_after);
    for round in (1..=distance).rev() {
        let (low, furthest) = &rounds[round];
        let at = |kk: usize| furthest[kk - low];
        let kk = x + max - y;
        let previous_kk = if kk == *low || at(kk - 1) < at(kk + 1) {
            kk + 1
        } else {
            kk - 1
        };
        let previous_x = at(previous_kk);
        let previous_y = previous_x + max - previous_kk;
        while x > previous_x && y > previous_y {
            edits.push(Edit::Equal(x - 1, y - 1));
            x -= 1;
            y -= 1;
        }
        edits.push(if x == previous_x {
            Edit::Insert(previous_y)
        } else {
            Edit::Delete(previous_x)
        });
        x = previous_x;
        y = previous_y;
    }
    // Round zero would be the snake from the origin, which is empty: `edits` has taken
    // the shared prefix before the search.
    edits.reverse();
    edits
}

/// The unified diff from `before` to `after`, both the file at `path`, with three lines
/// of context: what `patch -p0` applies to `before` to get `after`. Empty when they are
/// the same.
#[must_use]
pub fn unified_diff(path: &str, before: &str, after: &str) -> String {
    if before == after {
        return String::new();
    }
    let a: Vec<&str> = before.split_inclusive('\n').collect();
    let b: Vec<&str> = after.split_inclusive('\n').collect();
    let script = edits(&a, &b);
    let mut out = format!("--- {path}\n+++ {path}\n");
    let mut old_before = 0;
    let mut new_before = 0;
    let mut written = 0;
    for hunk in hunks(&script, 3) {
        // Between hunks every line is shared: a hunk holds every change.
        old_before += hunk.start - written;
        new_before += hunk.start - written;
        let body = &script[hunk.clone()];
        let old_count = body
            .iter()
            .filter(|edit| !matches!(edit, Edit::Insert(_)))
            .count();
        let new_count = body
            .iter()
            .filter(|edit| !matches!(edit, Edit::Delete(_)))
            .count();
        let _ = writeln!(
            out,
            "@@ -{} +{} @@",
            range(old_before, old_count),
            range(new_before, new_count)
        );
        for edit in body {
            let (sign, line) = match *edit {
                Edit::Equal(i, _) => (' ', a[i]),
                Edit::Delete(i) => ('-', a[i]),
                Edit::Insert(j) => ('+', b[j]),
            };
            out.push(sign);
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
        old_before += old_count;
        new_before += new_count;
        written = hunk.end;
    }
    out
}

/// A hunk header's range: the first line (1-based, or the line before for none) and
/// the count, left out when it is one.
fn range(before: usize, count: usize) -> String {
    let start = if count == 0 { before } else { before + 1 };
    if count == 1 {
        start.to_string()
    } else {
        format!("{start},{count}")
    }
}

/// The hunks of a script as ranges of its edits: every change with `context` shared
/// lines on each side, hunks that would touch or overlap joined.
fn hunks(script: &[Edit], context: usize) -> Vec<Range<usize>> {
    let mut out: Vec<Range<usize>> = Vec::new();
    for (at, edit) in script.iter().enumerate() {
        if matches!(edit, Edit::Equal(..)) {
            continue;
        }
        let start = at.saturating_sub(context);
        let end = (at + context + 1).min(script.len());
        match out.last_mut() {
            Some(last) if start <= last.end => last.end = end,
            _ => out.push(start..end),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::{edits, unified_diff, Edit};
    use sjs_testkit::prng::Rng;
    use sjs_testkit::prop::{check_with, Config, Gen};

    fn apply(a: &[char], b: &[char], script: &[Edit]) -> Vec<char> {
        let mut out = Vec::new();
        for edit in script {
            match *edit {
                Edit::Equal(i, j) => {
                    assert_eq!(a[i], b[j], "an equal step pairs equal elements");
                    out.push(a[i]);
                }
                Edit::Delete(_) => {}
                Edit::Insert(j) => out.push(b[j]),
            }
        }
        out
    }

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    #[test]
    fn the_script_turns_the_first_sequence_into_the_second_and_is_shortest() {
        let (a, b) = (chars("abcabba"), chars("cbabac"));
        let script = edits(&a, &b);
        assert_eq!(apply(&a, &b, &script), b);
        let changes = script
            .iter()
            .filter(|edit| !matches!(edit, Edit::Equal(..)))
            .count();
        assert_eq!(changes, 5, "Myers' example has distance five");
        assert_eq!(edits(&chars(""), &chars("")), Vec::new());
        assert_eq!(
            edits(&chars("ab"), &chars("ab")),
            vec![Edit::Equal(0, 0), Edit::Equal(1, 1)]
        );
        assert_eq!(
            edits(&chars("ab"), &chars("")),
            vec![Edit::Delete(0), Edit::Delete(1)]
        );
        assert_eq!(
            edits(&chars(""), &chars("ab")),
            vec![Edit::Insert(0), Edit::Insert(1)]
        );
        assert_eq!(
            edits(&chars("ac"), &chars("abc")),
            vec![Edit::Equal(0, 0), Edit::Insert(1), Edit::Equal(1, 2)]
        );
    }

    /// Two short texts over a small alphabet, so that scripts need real searching.
    struct Pair;

    impl Gen for Pair {
        type Value = (Vec<char>, Vec<char>);

        fn generate(&self, rng: &mut Rng) -> Self::Value {
            let text = |rng: &mut Rng| {
                (0..rng.index(12))
                    .map(|_| ['a', 'b', 'c'][rng.index(3)])
                    .collect::<Vec<char>>()
            };
            (text(rng), text(rng))
        }

        fn shrink(&self, value: &Self::Value) -> Vec<Self::Value> {
            let mut out = Vec::new();
            for at in 0..value.0.len() {
                let mut shorter = value.0.clone();
                shorter.remove(at);
                out.push((shorter, value.1.clone()));
            }
            out
        }
    }

    #[test]
    fn every_script_replays_its_second_sequence() {
        check_with(&Config::from_env(), &Pair, &|(a, b): &(
            Vec<char>,
            Vec<char>,
        )| {
            assert_eq!(apply(a, b, &edits(a, b)), *b, "{a:?} -> {b:?}");
            Ok(())
        })
        .expect("the script replays");
        let shorter = Pair.shrink(&(chars("ab"), chars("c")));
        assert_eq!(
            shorter,
            vec![(chars("b"), chars("c")), (chars("a"), chars("c"))]
        );
        assert_eq!(Pair.shrink(&(chars(""), chars("c"))), Vec::new());
    }

    #[test]
    fn a_unified_diff_has_headers_hunks_and_context() {
        assert_eq!(
            unified_diff("a.js", "x\n", "x\n"),
            "",
            "the same text: no diff"
        );
        assert_eq!(
            unified_diff("a.js", "a\nb\nc\n", "a\nB\nc\n"),
            "--- a.js\n+++ a.js\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
        );
        let mut before = String::new();
        for n in 1..=12 {
            let _ = writeln!(before, "l{n}");
        }
        let after = before.replace("l2\n", "L2\n").replace("l11\n", "L11\n");
        assert_eq!(
            unified_diff("a.js", &before, &after),
            "--- a.js\n+++ a.js\n@@ -1,5 +1,5 @@\n l1\n-l2\n+L2\n l3\n l4\n l5\n@@ -8,5 +8,5 @@\n l8\n l9\n l10\n-l11\n+L11\n l12\n",
            "two hunks far apart, three lines of context each"
        );
        let joined = before.replace("l5\n", "L5\n").replace("l11\n", "L11\n");
        assert!(
            unified_diff("a.js", &before, &joined).matches("@@").count() == 2,
            "changes within six shared lines share a hunk"
        );
        assert_eq!(
            unified_diff("a.js", "a", "a\n"),
            "--- a.js\n+++ a.js\n@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+a\n",
            "a missing final newline is marked"
        );
        assert_eq!(
            unified_diff("a.js", "", "a\n"),
            "--- a.js\n+++ a.js\n@@ -0,0 +1 @@\n+a\n",
            "an empty file gains a line"
        );
        assert_eq!(
            unified_diff("a.js", "a\nb\n", ""),
            "--- a.js\n+++ a.js\n@@ -1,2 +0,0 @@\n-a\n-b\n",
            "a file loses every line"
        );
    }
}
