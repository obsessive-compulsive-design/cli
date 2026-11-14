//! The S-code reporter (US-048, SPEC.md §8.5, §9.6): every way the canonical text
//! departs from the source, as a finding with the current text and the text the layout
//! writes. The formatter's passes report what they judge (S003, S004, S005, S009, S010,
//! S011, S006 for a comment too long, S007 and S008 for the text's whitespace) and the
//! parser S001, S002 and S012. This module diffs the source against the canonical text
//! token by token and gives every change nobody claimed its code: S006 for a change on a
//! line longer than the width, S007 for a line's indentation, S008 for any other change
//! of whitespace or of a comment. A change of tokens nobody claimed is a fault in a
//! pass: it is reported as S008 and listed apart, so that a test can fail on it.

use std::ops::Range;

use sjs_diag::{Code, Diagnostic, Diagnostics, Severity, SourceFile, Span};
use sjs_syntax::{parse, NodeKind, NodeRef, Token, TokenKind};

use crate::comments::comments_of;
use crate::diff::{edits, Edit};
use crate::doc::WIDTH;
use crate::expr::Formatter;
use crate::imports::run_ranges;
use crate::lists::in_tree_order;

/// What `sjs lint` reports for a file's layout: the findings, and among them those for
/// a change of tokens no pass claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Every S-code finding of the formatter, in no particular order.
    pub findings: Vec<Diagnostic>,
    /// The findings for changes of tokens no pass claims: a fault in a pass.
    pub unclassified: Vec<Diagnostic>,
}

/// A token, as the diff compares them: by text, with its place. Comments are not
/// pieces: the layout moves one past a token (a `{`, a `,`) without the tokens changing,
/// so a comment is part of what lies between two tokens, like whitespace.
struct Piece<'a> {
    text: &'a str,
    range: Range<usize>,
}

impl PartialEq for Piece<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

/// A stretch of the source and the stretch of the canonical text standing in its place,
/// which differ: what lies between two pieces both have.
struct Unit {
    a: Range<usize>,
    b: Range<usize>,
    /// The pieces of the source that are gone: each with the run of pieces spelled like
    /// it that it stands in (of `)))` the diff may drop any one, so a finding on one
    /// claims whichever it chose).
    deleted: Vec<(Range<usize>, Range<usize>)>,
    /// The pieces of the canonical text that are new: where each would stand in the
    /// source, and its place in the canonical text.
    inserted: Vec<(usize, Range<usize>)>,
}

/// What a unit comes to once the known findings have claimed their pieces.
enum Verdict {
    /// Every piece is claimed, or there is none: the whitespace and the comments of the
    /// stretches.
    Whitespace(Range<usize>, Range<usize>),
    /// Tokens nobody claims: a fault in a pass.
    Tokens(Range<usize>, Range<usize>),
}

impl Unit {
    fn has_pieces(&self) -> bool {
        !(self.deleted.is_empty() && self.inserted.is_empty())
    }

    /// Where a new piece stands in the source: from its point to the stretch's end.
    fn place_of(&self, at: usize) -> Range<usize> {
        at..self.a.end
    }

    /// Whether a known finding claims the unit: it touches each piece gone or new, or,
    /// for a unit without pieces, the stretch.
    fn explained(&self, known: &[Span]) -> bool {
        if !self.has_pieces() {
            return claimed(known, &self.a);
        }
        self.deleted.iter().all(|(_, run)| claimed(known, run))
            && self
                .inserted
                .iter()
                .all(|(at, _)| claimed(known, &self.place_of(*at)))
    }

    /// The unit judged against the known findings.
    fn verdict(&self, known: &[Span]) -> Verdict {
        let free_deleted: Vec<&(Range<usize>, Range<usize>)> = self
            .deleted
            .iter()
            .filter(|(_, run)| !claimed(known, run))
            .collect();
        let free_inserted: Vec<&(usize, Range<usize>)> = self
            .inserted
            .iter()
            .filter(|(at, _)| !claimed(known, &self.place_of(*at)))
            .collect();
        if free_deleted.is_empty() && free_inserted.is_empty() {
            return Verdict::Whitespace(self.a.clone(), self.b.clone());
        }
        let a_extent = free_deleted.first().zip(free_deleted.last()).map_or_else(
            || {
                let at = free_inserted[0].0;
                at..at
            },
            |((first, _), (last, _))| first.start..last.end,
        );
        let b_extent = free_inserted
            .first()
            .zip(free_inserted.last())
            .map_or(self.b.clone(), |((_, first), (_, last))| {
                first.start..last.end
            });
        let claimed_a: Vec<Range<usize>> = self
            .deleted
            .iter()
            .filter(|(_, run)| claimed(known, run))
            .map(|(range, _)| range.clone())
            .collect();
        let claimed_b: Vec<Range<usize>> = self
            .inserted
            .iter()
            .filter(|(at, _)| claimed(known, &self.place_of(*at)))
            .map(|(_, range)| range.clone())
            .collect();
        let a = widen(&a_extent, &self.a, &claimed_a);
        let b = widen(&b_extent, &self.b, &claimed_b);
        Verdict::Tokens(a, b)
    }
}

fn touches(span: &Span, range: &Range<usize>) -> bool {
    span.byte_start <= range.end && range.start <= span.byte_end
}

fn claimed(known: &[Span], range: &Range<usize>) -> bool {
    known.iter().any(|span| touches(span, range))
}

/// The ranges of `all` that lie inside `stretch`.
fn within(all: &[Range<usize>], stretch: &Range<usize>) -> Vec<Range<usize>> {
    all.iter()
        .filter(|range| stretch.start <= range.start && range.end <= stretch.end)
        .cloned()
        .collect()
}

/// The comments of a stretch of the source and of the canonical text, each given with
/// its text and where it starts, that are not the same on both sides: from the first
/// that differs to the last, the comments both begin and end with alike left out.
/// `None` when every comment is as it was.
fn changed_comments(
    (in_a, text_a, start_a): (&[Range<usize>], &str, usize),
    (in_b, text_b, start_b): (&[Range<usize>], &str, usize),
) -> Option<(Range<usize>, Range<usize>)> {
    let same =
        |(x, y): &(&Range<usize>, &Range<usize>)| text_a[(*x).clone()] == text_b[(*y).clone()];
    let front = in_a.iter().zip(in_b).take_while(same).count();
    let back = in_a[front..]
        .iter()
        .rev()
        .zip(in_b[front..].iter().rev())
        .take_while(same)
        .count();
    let (rest_a, rest_b) = (
        &in_a[front..in_a.len() - back],
        &in_b[front..in_b.len() - back],
    );
    // The layout keeps every comment, so both sides hold as many.
    if rest_a.is_empty() {
        return None;
    }
    let extent = |all: &[Range<usize>], rest: &[Range<usize>], start: usize| {
        let at = front.checked_sub(1).map_or(start, |before| all[before].end);
        rest.first()
            .zip(rest.last())
            .map_or(at..at, |(first, last)| first.start..last.end)
    };
    Some((extent(in_a, rest_a, start_a), extent(in_b, rest_b, start_b)))
}

/// `extent` widened to `bounds`, but not past the `claimed` ranges beside it.
fn widen(extent: &Range<usize>, bounds: &Range<usize>, claimed: &[Range<usize>]) -> Range<usize> {
    let mut out = bounds.clone();
    for range in claimed {
        if range.end <= extent.start {
            out.start = out.start.max(range.end);
        } else if range.start >= extent.end {
            out.end = out.end.min(range.start);
        }
    }
    out
}

impl Formatter<'_> {
    /// Every S-code finding of the formatter for the file, given the parser's findings:
    /// the passes' own, then one for each change of the canonical text no pass claims
    /// (none for a file the grammar rejects, whose layout is a best effort).
    #[must_use]
    pub fn report(&self, parsed: &Diagnostics) -> Report {
        let mut findings = Vec::new();
        findings.extend(self.redundant_parentheses());
        findings.extend(self.whitespace());
        findings.extend(self.trailing_commas());
        findings.extend(self.arrow_returns());
        findings.extend(self.method_chains());
        findings.extend(self.import_order());
        findings.extend(self.long_lines());
        findings.extend(self.member_order());
        if parsed.has_errors() {
            return Report {
                findings,
                unclassified: Vec::new(),
            };
        }
        let known: Vec<Span> = parsed
            .iter()
            .filter(|d| d.severity() == Severity::Style)
            .chain(findings.iter())
            .map(|d| d.span)
            .collect();
        let reordered: Vec<Span> = findings
            .iter()
            .filter(|d| matches!(d.code, Code::S003 | Code::S009))
            .map(|d| d.span)
            .collect();
        let (classified, unclassified) = self.changes(&known, &reordered);
        findings.extend(classified);
        Report {
            findings,
            unclassified,
        }
    }

    /// The findings for the changes between the source and the canonical text that no
    /// finding in `known` claims, and among them those that change tokens.
    fn changes(&self, known: &[Span], reordered: &[Span]) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
        SourceFile::new(self.file.path(), self.module().into_bytes())
            .ok()
            .map_or_else(Default::default, |formatted| {
                self.changes_against(&formatted, known, reordered)
            })
    }

    fn changes_against(
        &self,
        formatted: &SourceFile,
        known: &[Span],
        reordered: &[Span],
    ) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
        let (tree, _) = parse(formatted);
        // What a reordering spans is cut from both texts before they are compared: its
        // pieces, in another order, would match whatever is spelled like them nearby.
        let mut source_bodies = Vec::new();
        self.in_layout_order(self.tree.root(), &[NodeKind::ClassBody], &mut source_bodies);
        let mut laid_bodies = Vec::new();
        in_tree_order(tree.root(), &[NodeKind::ClassBody], &mut laid_bodies);
        let range_of = |node: &NodeRef<'_>| node.span().byte_start..node.span().byte_end;
        let pairs = source_bodies
            .iter()
            .map(range_of)
            .zip(laid_bodies.iter().map(range_of))
            .chain(
                run_ranges(self.tree.root())
                    .into_iter()
                    .zip(run_ranges(tree.root())),
            );
        let (cut_a, cut_b): (Vec<Range<usize>>, Vec<Range<usize>>) = pairs
            .filter(|(source, _)| {
                reordered
                    .iter()
                    .any(|span| (span.byte_start..span.byte_end) == *source)
            })
            .unzip();
        let outside = |cut: &[Range<usize>], piece: &Piece<'_>| {
            !cut.iter()
                .any(|range| range.start <= piece.range.start && piece.range.end <= range.end)
        };
        let mut a = pieces(self.tree.tokens(), self.file.text());
        a.retain(|piece| outside(&cut_a, piece));
        let mut b = pieces(tree.tokens(), formatted.text());
        b.retain(|piece| outside(&cut_b, piece));
        let ranges = |tokens: &[Token]| -> Vec<Range<usize>> {
            comments_of(tokens.iter())
                .iter()
                .map(|span| span.byte_start..span.byte_end)
                .collect()
        };
        let (comments_a, comments_b) = (ranges(self.tree.tokens()), ranges(tree.tokens()));
        let mut classified = Vec::new();
        let mut unclassified = Vec::new();
        // A change of whitespace joins the one before it when the same code names both
        // and, joined, they lie on one line of the source.
        let mut pending: Option<(Code, Range<usize>, Range<usize>)> = None;
        for unit in units(&a, &b, self.file.text(), formatted.text()) {
            // A finding that reorders what it spans (S003, S009) claims every unit that
            // touches its span.
            if reordered.iter().any(|span| touches(span, &unit.a)) {
                continue;
            }
            // A comment the layout respaces or reflows is a change of its own, whatever
            // else its stretch holds; a unit a finding claims says nothing more.
            let comments = changed_comments(
                (
                    &within(&comments_a, &unit.a),
                    self.file.text(),
                    unit.a.start,
                ),
                (
                    &within(&comments_b, &unit.b),
                    formatted.text(),
                    unit.b.start,
                ),
            );
            if comments.is_some() || unit.explained(known) {
                if let Some((a, b)) = comments {
                    let (code, a, b) = self.placed(&a, &b, formatted.text());
                    classified.extend(
                        pending
                            .replace((code, a, b))
                            .and_then(|(c, a, b)| self.finding(c, &a, &b, formatted)),
                    );
                }
                continue;
            }
            let (code, a, b) = match unit.verdict(known) {
                Verdict::Tokens(a, b) => {
                    classified.extend(
                        pending
                            .take()
                            .and_then(|(c, a, b)| self.finding(c, &a, &b, formatted)),
                    );
                    let (a, b) = self.narrowed(&a, &b, formatted.text());
                    let finding = self.finding(Code::S008, &a, &b, formatted);
                    classified.extend(finding.clone());
                    unclassified.extend(finding);
                    continue;
                }
                Verdict::Whitespace(a, b) => self.placed(&a, &b, formatted.text()),
            };
            pending = Some(match pending.take() {
                Some((before, a0, b0))
                    if before == code && !self.file.text()[a0.start..a.end].contains('\n') =>
                {
                    (code, a0.start..a.end, b0.start..b.end)
                }
                other => {
                    classified
                        .extend(other.and_then(|(c, a, b)| self.finding(c, &a, &b, formatted)));
                    (code, a, b)
                }
            });
        }
        classified.extend(pending.and_then(|(c, a, b)| self.finding(c, &a, &b, formatted)));
        (classified, unclassified)
    }

    /// The code of a change of whitespace or of comments, and its stretches: S006 on a
    /// line longer than the width, S007 for a line's indentation (the stretches then
    /// begin at the line), S008 otherwise. A change of comments is narrowed to what
    /// differs, the whitespace both sides share around it left out.
    fn placed(
        &self,
        a: &Range<usize>,
        b: &Range<usize>,
        formatted: &str,
    ) -> (Code, Range<usize>, Range<usize>) {
        let has_comment = |text: &str| text.contains("//") || text.contains("/*");
        // The layout keeps every comment: what the source's stretch holds, the other does.
        let comments = has_comment(&self.file.text()[a.clone()]);
        let (a, b) = if comments {
            self.narrowed(a, b, formatted)
        } else {
            (a.clone(), b.clone())
        };
        let a_text = &self.file.text()[a.clone()];
        let b_text = &formatted[b.clone()];
        // The lines the stretch lies on: a stretch ending at a line's start is not on
        // that line.
        let last = a.end.saturating_sub(1).max(a.start);
        let long = self.file.span(a.start, last).is_some_and(|span| {
            (span.start.line..=span.end.line).any(|line| {
                self.file
                    .line(line)
                    .is_some_and(|line| line.text.chars().count() > WIDTH)
            })
        });
        if long {
            return (Code::S006, a, b);
        }
        if !comments {
            if let (Some((head_a, _)), Some((head_b, _))) =
                (a_text.rsplit_once('\n'), b_text.rsplit_once('\n'))
            {
                if head_a == head_b {
                    let a = a.start + head_a.len() + 1..a.end;
                    let b = b.start + head_b.len() + 1..b.end;
                    return (Code::S007, a, b);
                }
            }
        }
        (Code::S008, a, b)
    }

    /// The stretches `a` of the source and `b` of the canonical text without the
    /// whitespace both begin and end with alike.
    fn narrowed(
        &self,
        a: &Range<usize>,
        b: &Range<usize>,
        formatted: &str,
    ) -> (Range<usize>, Range<usize>) {
        let (a_text, b_text) = (&self.file.text()[a.clone()], &formatted[b.clone()]);
        let front = shared_whitespace(a_text.chars(), b_text.chars());
        let back = shared_whitespace(a_text[front..].chars().rev(), b_text[front..].chars().rev());
        (a.start + front..a.end - back, b.start + front..b.end - back)
    }

    /// A finding for `code` over the stretch `a` of the source, with the stretch `b` of
    /// the canonical text as the replacement.
    fn finding(
        &self,
        code: Code,
        a: &Range<usize>,
        b: &Range<usize>,
        formatted: &SourceFile,
    ) -> Option<Diagnostic> {
        let message = match code {
            Code::S006 => "a line longer than 100 columns: the layout breaks it (SPEC.md §9.1)",
            Code::S007 => {
                "indentation that is not the layout's: two spaces per level (SPEC.md §9.1)"
            }
            _ => "layout that is not the canonical one (SPEC.md §9)",
        };
        self.file.span(a.start, a.end).and_then(|span| {
            Diagnostic::new(code, self.file.path(), span, message)
                .with_fix(&self.file.text()[a.clone()], &formatted.text()[b.clone()])
                .ok()
        })
    }
}

/// The bytes of whitespace two texts begin with alike.
fn shared_whitespace(a: impl Iterator<Item = char>, b: impl Iterator<Item = char>) -> usize {
    a.zip(b)
        .take_while(|(p, q)| p == q && p.is_whitespace())
        .map(|(p, _)| p.len_utf8())
        .sum()
}

/// The pieces of a token stream: every token but the end, in order.
fn pieces<'a>(tokens: &'a [Token], text: &'a str) -> Vec<Piece<'a>> {
    tokens
        .iter()
        .filter(|token| token.kind != TokenKind::Eof)
        .map(|token| {
            let range = token.span.byte_start..token.span.byte_end;
            Piece {
                text: &text[range.clone()],
                range,
            }
        })
        .collect()
}

/// The units of change between the pieces `a` of `source` and the pieces `b` of
/// `formatted`: each stretch between two shared pieces (and after the last) whose text
/// differs, with the pieces gone and new inside it.
fn units(a: &[Piece<'_>], b: &[Piece<'_>], source: &str, formatted: &str) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut from_a = 0;
    let mut from_b = 0;
    let mut cursor = 0;
    let mut deleted = Vec::new();
    let mut inserted = Vec::new();
    let mut close = |a_range: Range<usize>,
                     b_range: Range<usize>,
                     deleted: &mut Vec<(Range<usize>, Range<usize>)>,
                     inserted: &mut Vec<(usize, Range<usize>)>| {
        if source[a_range.clone()] != formatted[b_range.clone()] {
            units.push(Unit {
                a: a_range,
                b: b_range,
                deleted: std::mem::take(deleted),
                inserted: std::mem::take(inserted),
            });
        }
        deleted.clear();
        inserted.clear();
    };
    for edit in edits(a, b) {
        match edit {
            Edit::Delete(i) => {
                let alike = |other: &Piece<'_>| *other == a[i];
                let before = a[..i].iter().rev().take_while(|p| alike(p)).count();
                let after = a[i + 1..].iter().take_while(|p| alike(p)).count();
                let run = a[i - before].range.start..a[i + after].range.end;
                deleted.push((a[i].range.clone(), run));
                cursor = a[i].range.end;
            }
            Edit::Insert(j) => inserted.push((cursor, b[j].range.clone())),
            Edit::Equal(i, j) => {
                close(
                    from_a..a[i].range.start,
                    from_b..b[j].range.start,
                    &mut deleted,
                    &mut inserted,
                );
                from_a = a[i].range.end;
                from_b = b[j].range.end;
                cursor = from_a;
            }
        }
    }
    close(
        from_a..source.len(),
        from_b..formatted.len(),
        &mut deleted,
        &mut inserted,
    );
    units
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    /// A finding: its code, line, column and fix.
    type Found = (Code, u32, u32, Option<(String, String)>);

    fn view(d: &sjs_diag::Diagnostic) -> Found {
        let fix = d.current.clone().zip(d.replacement.clone());
        (d.code, d.span.start.line, d.span.start.column, fix)
    }

    /// Every S-code of the file, the parser's and the formatter's, in order.
    fn found(text: &str) -> Vec<Found> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, parsed) = parse(&f);
        let report = Formatter::new(&f, &tree).report(&parsed);
        assert!(
            report.unclassified.is_empty(),
            "a change no pass claims: {:?}",
            report.unclassified
        );
        let mut all: Vec<Found> = parsed
            .iter()
            .filter(|d| d.code.severity() == sjs_diag::Severity::Style)
            .chain(report.findings.iter())
            .map(view)
            .collect();
        all.sort_by_key(|(code, line, column, _)| (*line, *column, *code));
        all
    }

    /// The findings for every change, no pass having claimed any.
    fn unclaimed(text: &str) -> (Vec<Found>, Vec<Found>) {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        let (classified, unclassified) = Formatter::new(&f, &tree).changes(&[], &[]);
        (
            classified.iter().map(view).collect(),
            unclassified.iter().map(view).collect(),
        )
    }

    fn fix(current: &str, replacement: &str) -> (String, String) {
        (current.to_owned(), replacement.to_owned())
    }

    #[test]
    fn a_canonical_file_has_no_findings_and_a_missing_newline_one() {
        assert_eq!(found("const a = 1\n\nf(a)\n"), Vec::new());
        assert_eq!(found(""), Vec::new());
        assert_eq!(
            found("const a = 1"),
            vec![(Code::S008, 1, 12, Some(fix("", "\n")))],
            "only the final newline"
        );
    }

    #[test]
    fn whitespace_the_layout_changes_is_s007_or_s008() {
        assert_eq!(
            found("function f() {\n    a()\n}\n"),
            vec![(Code::S007, 2, 1, Some(fix("    ", "  ")))],
            "an indentation of the wrong depth"
        );
        assert_eq!(
            found("function f() {\n  a()\n\n    b()\n}\n"),
            vec![(Code::S007, 4, 1, Some(fix("    ", "  ")))],
            "the blank line before it stays out of the finding"
        );
        assert_eq!(
            found("a()\nb()\n"),
            vec![(Code::S008, 1, 4, Some(fix("\n", "\n\n")))],
            "a blank line between items"
        );
        assert_eq!(
            found("const x = a+b\n"),
            vec![(Code::S008, 1, 12, Some(fix("+", " + ")))],
            "spaces around an operator, one finding for the line"
        );
        assert_eq!(
            found("f( a )\n"),
            vec![(Code::S008, 1, 3, Some(fix(" a ", "a")))]
        );
        assert_eq!(
            found("//x\nf() //y\n"),
            vec![
                (Code::S008, 1, 1, Some(fix("//x", "// x"))),
                (Code::S008, 2, 5, Some(fix("//y", "// y")))
            ],
            "comments as the layout writes them"
        );
        assert_eq!(
            found("const  a = 1\n\n\n\nf(a)\n"),
            vec![
                (Code::S008, 1, 6, Some(fix("  ", " "))),
                (Code::S008, 1, 13, Some(fix("\n\n\n\n", "\n\n")))
            ]
        );
    }

    #[test]
    fn a_long_line_the_layout_breaks_is_s006() {
        let args = (0..12)
            .map(|n| format!("argument{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let text = format!("f({args})\n");
        let out = found(&text);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[1].0, Code::S004, "the comma the broken list takes");
        let (code, line, column, fixed) = &out[0];
        assert_eq!((*code, *line, *column), (Code::S006, 1, 3));
        let (current, replacement) = fixed.clone().expect("a replacement");
        assert!(text.contains(&current));
        assert!(
            replacement.starts_with("\n  argument0,\n  argument1,"),
            "{replacement}"
        );
        assert_eq!(
            found(&format!("f(a)\n{text}")),
            vec![
                (Code::S008, 1, 5, Some(fix("\n", "\n\n"))),
                (out[0].0, 2, out[0].2, out[0].3.clone()),
                (out[1].0, 2, out[1].2, out[1].3.clone()),
            ],
            "the blank line before the long line is not on it"
        );
    }

    #[test]
    fn a_change_two_passes_claim_is_their_two_findings() {
        assert_eq!(
            found("f('a',)\n"),
            vec![
                (Code::S001, 1, 3, Some(fix("'a'", "\"a\""))),
                (Code::S004, 1, 6, Some(fix(",", "")))
            ]
        );
        assert_eq!(
            found("const x = (a);\n"),
            vec![
                (Code::S005, 1, 11, Some(fix("(a)", "a"))),
                (Code::S002, 1, 14, Some(fix(";", "")))
            ]
        );
    }

    #[test]
    fn a_comment_beside_a_claimed_change_is_judged_on_its_own() {
        assert_eq!(
            found("x = (a)\n//c\nf()\n"),
            vec![
                (Code::S005, 1, 5, Some(fix("(a)", "a"))),
                (Code::S008, 2, 1, Some(fix("//c", "// c")))
            ],
            "a comment after a dropped pair"
        );
        assert_eq!(
            found("//c\n(a).b()\n"),
            vec![
                (Code::S008, 1, 1, Some(fix("//c", "// c"))),
                (Code::S005, 2, 1, Some(fix("(a)", "a")))
            ],
            "a comment before one"
        );
        assert_eq!(
            found("import { b } from './b.js' //x\nimport { a } from './a.js'\n"),
            vec![
                (
                    Code::S003,
                    1,
                    1,
                    Some(fix(
                        "import { b } from './b.js' //x\nimport { a } from './a.js'",
                        "import { a } from \"./a.js\"\nimport { b } from \"./b.js\"  // x"
                    ))
                ),
                (Code::S001, 1, 19, Some(fix("'./b.js'", "\"./b.js\""))),
                (Code::S001, 2, 19, Some(fix("'./a.js'", "\"./a.js\""))),
            ],
            "a reordering claims every change it touches"
        );
    }

    #[test]
    fn a_claimed_piece_between_unclaimed_comments_stays_in_their_stretch() {
        assert_eq!(
            found("//a\n;\n//b\nf()\n"),
            vec![
                (Code::S008, 1, 1, Some(fix("//a\n;\n//b", "// a\n\n// b"))),
                (Code::S002, 2, 1, Some(fix(";", "")))
            ],
            "the dropped `;` lies between the two comments the layout respaces"
        );
    }

    #[test]
    fn a_known_span_claims_a_piece_only_when_it_touches_the_piece() {
        let f = SourceFile::new("a.js", b"x = (a)\n".to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        let beside = f.span(3, 3).expect("a span");
        let (_, unclassified) = Formatter::new(&f, &tree).changes(&[beside], &[]);
        assert_eq!(
            unclassified.iter().map(view).collect::<Vec<_>>(),
            vec![
                (Code::S008, 1, 5, Some(fix("(", ""))),
                (Code::S008, 1, 7, Some(fix(")", "")))
            ],
            "a span at the stretch's edge, not on the `(`, claims nothing"
        );
    }

    #[test]
    fn an_unclaimed_piece_stops_at_the_claimed_ones_beside_it() {
        let f = SourceFile::new("a.js", b"f(('a'),)\ng(('b'))\n".to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        let spans: Vec<_> = [(2, 3), (7, 9), (13, 14)]
            .iter()
            .map(|&(from, to)| f.span(from, to).expect("a span"))
            .collect();
        let (_, unclassified) = Formatter::new(&f, &tree).changes(&spans, &[]);
        assert_eq!(
            unclassified.iter().map(view).collect::<Vec<_>>(),
            vec![
                (Code::S008, 1, 7, Some(fix("", "\"a\""))),
                (Code::S008, 2, 7, Some(fix(")", "\"b\"")))
            ],
            "what is claimed (a span claims what it touches) bounds what is not"
        );
        let f = SourceFile::new("a.js", b"f('a' ,)\n".to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        let comma = f.span(6, 7).expect("a span");
        let (_, unclassified) = Formatter::new(&f, &tree).changes(&[comma], &[]);
        assert_eq!(
            unclassified.iter().map(view).collect::<Vec<_>>(),
            vec![(Code::S008, 1, 3, Some(fix("'a' ", "\"a\"")))],
            "a claimed piece after the unclaimed one ends it"
        );
    }

    #[test]
    fn the_width_itself_is_not_too_long_and_messages_name_their_rule() {
        let exactly = format!("const {} = a+b\n", "x".repeat(88));
        assert_eq!(exactly.trim_end().chars().count(), 100);
        let out = found(&exactly);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].0, Code::S008, "a line of the width is not over it");
        let f = SourceFile::new(
            "a.js",
            format!(
                "function f() {{\n    a()\n}}\nf({})\n",
                "argument, ".repeat(12)
            )
            .into_bytes(),
        )
        .expect("utf-8");
        let (tree, parsed) = parse(&f);
        let report = Formatter::new(&f, &tree).report(&parsed);
        let message = |code: Code| {
            report
                .findings
                .iter()
                .find(|d| d.code == code)
                .map(|d| d.message.clone())
                .unwrap_or_default()
        };
        let (long, indentation) = (message(Code::S006), message(Code::S007));
        assert!(long.contains("longer than 100"), "{long}");
        assert!(indentation.contains("indentation"), "{indentation}");
    }

    #[test]
    fn what_a_reordering_spans_is_not_matched_with_what_is_spelled_like_it() {
        let text = "class A {\n  async m() {}\n  x = 1\n}\n\nasync function m() {}\n\nf( a )\n";
        assert_eq!(
            found(text),
            vec![
                (
                    Code::S009,
                    1,
                    9,
                    Some(fix(
                        "{\n  async m() {}\n  x = 1\n}",
                        "{\n  x = 1\n  async m() {}\n}"
                    ))
                ),
                (Code::S008, 8, 3, Some(fix(" a ", "a")))
            ],
            "the function after the class is spelled like the method that moved"
        );
        let imports =
            "import { b } from \"./b.js\"\nimport { a } from \"./a.js\"\n\nimport2( \"./b.js\")\n";
        let out = found(imports);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!((out[0].0, out[1].0, out[1].1), (Code::S003, Code::S008, 4));
    }

    #[test]
    fn a_reordered_run_of_imports_is_cut_from_what_is_compared() {
        let text = "import { s as g, l } from \"./util.js\"\n\n//////\nimport r from \"./config.json\" with { type: \"json\" }\n\n{\n  while ((u)) {\n  }\n  f()\n}\n";
        let out = found(text);
        assert_eq!(
            out.iter().map(|f| (f.0, f.1, f.2)).collect::<Vec<_>>(),
            vec![(Code::S003, 1, 1), (Code::S005, 7, 10), (Code::S008, 7, 16)],
            "{out:?}"
        );
    }

    #[test]
    fn a_comment_of_either_kind_is_narrowed_to_what_differs() {
        assert_eq!(
            found("count /= 1n // a\n"),
            vec![(Code::S008, 1, 13, Some(fix("// a", " // a")))]
        );
    }

    #[test]
    fn a_dropped_token_among_its_like_is_claimed_by_a_finding_on_any() {
        assert_eq!(
            found("for (const x of f(1, (0))) {}\n"),
            vec![(Code::S005, 1, 22, Some(fix("(0)", "0")))],
            "of `)))` the diff drops the last, the finding names the first"
        );
    }

    #[test]
    fn a_change_of_tokens_nobody_claims_is_listed_apart() {
        let (classified, unclassified) = unclaimed("f('a')\n");
        assert_eq!(
            classified,
            vec![(Code::S008, 1, 3, Some(fix("'a'", "\"a\"")))]
        );
        assert_eq!(unclassified, classified);
        let (_, unclassified) = unclaimed("const x = (a)\n");
        assert_eq!(
            unclassified,
            vec![
                (Code::S008, 1, 11, Some(fix("(", ""))),
                (Code::S008, 1, 13, Some(fix(")", "")))
            ],
            "tokens gone, none new"
        );
        let long = format!(
            "f({})\n",
            (0..12)
                .map(|n| format!("argument{n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let (_, unclassified) = unclaimed(&long);
        assert_eq!(unclassified.len(), 1, "{unclassified:?}");
        assert_eq!(unclassified[0].0, Code::S008);
        assert!(
            unclassified[0].3.as_ref().is_some_and(
                |(current, replacement)| current.is_empty() && replacement.starts_with(',')
            ),
            "tokens new, none gone: {unclassified:?}"
        );
    }

    #[test]
    fn a_file_the_grammar_rejects_gets_the_passes_findings_alone() {
        let f = SourceFile::new("a.js", b"export default  (a)\nconst x = (b)\n".to_vec())
            .expect("utf-8");
        let (tree, parsed) = parse(&f);
        let report = Formatter::new(&f, &tree).report(&parsed);
        assert!(parsed.has_errors());
        assert_eq!(report.unclassified, Vec::new());
        assert_eq!(
            report.findings.iter().map(view).collect::<Vec<_>>(),
            vec![(Code::S005, 2, 11, Some(fix("(b)", "b")))],
            "the pass's finding alone, not the spaces after `default`"
        );
    }
}
