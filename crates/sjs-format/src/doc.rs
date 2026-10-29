//! The document IR and its layout (STORIES.md US-040, SPEC.md §9.1): text; line breaks
//! that are a space, or nothing, unless their group is broken; groups that stay flat
//! when they fit in what remains of the line and otherwise break, outermost first;
//! indentation; and content chosen by whether the enclosing group is broken. The
//! algorithm is Wadler's prettier as Prettier runs it: a stack of commands, each a
//! document in a mode at an indentation level, and a look-ahead asking whether a
//! group's content, flat, fits before the line's next break. Width counts Unicode scalar
//! values, as diagnostics count columns (SPEC.md §13.1).

/// The line width: hard, in columns (SPEC.md §9.1).
pub const WIDTH: usize = 100;

/// Spaces per indentation level (SPEC.md §9.1).
pub const INDENT: usize = 2;

/// A piece of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doc {
    /// Text as written. A line break inside it (a template literal's) ends the line as
    /// a break would; nothing in it is reflowed.
    Text(String),
    /// A space when the enclosing group is flat, a line break when it is broken.
    Line,
    /// Nothing when the enclosing group is flat, a line break when it is broken.
    SoftLine,
    /// A line break always: no group holding one is flat.
    HardLine,
    /// Flat when its content fits in what remains of the line, broken otherwise.
    Group(Vec<Doc>),
    /// Its content, with the line breaks inside it indented one level more.
    Indent(Vec<Doc>),
    /// `broken` when the enclosing group is broken, `flat` when it is flat.
    IfBreak { broken: Vec<Doc>, flat: Vec<Doc> },
}

impl Doc {
    /// Text as written.
    pub fn text(text: impl Into<String>) -> Self {
        Doc::Text(text.into())
    }

    /// A group.
    #[must_use]
    pub fn group(content: Vec<Doc>) -> Self {
        Doc::Group(content)
    }

    /// Indented content.
    #[must_use]
    pub fn indent(content: Vec<Doc>) -> Self {
        Doc::Indent(content)
    }

    /// Content by whether the enclosing group is broken.
    #[must_use]
    pub fn if_break(broken: Vec<Doc>, flat: Vec<Doc>) -> Self {
        Doc::IfBreak { broken, flat }
    }
}

/// How a command is laid out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// A document to lay out, in a mode, at an indentation level.
#[derive(Debug, Clone, Copy)]
struct Cmd<'a> {
    indent: usize,
    mode: Mode,
    doc: &'a Doc,
}

/// Lays `docs` out at [`WIDTH`].
#[must_use]
pub fn print(docs: &[Doc]) -> String {
    layout(docs, WIDTH)
}

/// Lays `docs` out at `width` columns. No line ends in whitespace.
#[must_use]
pub fn layout(docs: &[Doc], width: usize) -> String {
    let mut out = String::new();
    let mut column = 0;
    let mut cmds: Vec<Cmd<'_>> = Vec::new();
    push(&mut cmds, docs, 0, Mode::Break);
    while let Some(Cmd { indent, mode, doc }) = cmds.pop() {
        match doc {
            Doc::Text(text) => {
                out.push_str(text);
                column = advance(column, text);
            }
            Doc::Line if mode == Mode::Flat => {
                out.push(' ');
                column += 1;
            }
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::Line | Doc::SoftLine | Doc::HardLine => {
                newline(&mut out, indent);
                column = indent * INDENT;
            }
            Doc::Group(content) => {
                let mode =
                    if mode == Mode::Flat || fits(content, &cmds, width.saturating_sub(column)) {
                        Mode::Flat
                    } else {
                        Mode::Break
                    };
                push(&mut cmds, content, indent, mode);
            }
            Doc::Indent(content) => push(&mut cmds, content, indent + 1, mode),
            Doc::IfBreak { broken, flat } => {
                let chosen = if mode == Mode::Break { broken } else { flat };
                push(&mut cmds, chosen, indent, mode);
            }
        }
    }
    trim_spaces(&mut out);
    out
}

/// Queues `content` so its first document comes off the stack next.
fn push<'a>(cmds: &mut Vec<Cmd<'a>>, content: &'a [Doc], indent: usize, mode: Mode) {
    for doc in content.iter().rev() {
        cmds.push(Cmd { indent, mode, doc });
    }
}

/// Whether `content`, laid out flat, followed by the commands still to come, fits in
/// `remaining` columns: measured up to the line's next break, which ends the line (a
/// break in the content to come, or inside a text). A hard break inside the flat
/// content means it cannot be flat.
fn fits(content: &[Doc], rest: &[Cmd<'_>], remaining: usize) -> bool {
    let mut remaining = remaining;
    let mut stack: Vec<(Mode, &Doc)> = content.iter().rev().map(|doc| (Mode::Flat, doc)).collect();
    let mut pending = rest.len();
    loop {
        let Some((mode, doc)) = stack.pop() else {
            if pending == 0 {
                return true;
            }
            pending -= 1;
            let next = rest[pending];
            stack.push((next.mode, next.doc));
            continue;
        };
        match doc {
            Doc::Text(text) => {
                if let Some((first, _)) = text.split_once('\n') {
                    return width(first) <= remaining;
                }
                match remaining.checked_sub(width(text)) {
                    Some(left) => remaining = left,
                    None => return false,
                }
            }
            Doc::Line if mode == Mode::Flat => match remaining.checked_sub(1) {
                Some(left) => remaining = left,
                None => return false,
            },
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::HardLine if mode == Mode::Flat => return false,
            Doc::Line | Doc::SoftLine | Doc::HardLine => return true,
            Doc::Group(content) | Doc::Indent(content) => {
                stack.extend(content.iter().rev().map(|doc| (mode, doc)));
            }
            Doc::IfBreak { broken, flat } => {
                let chosen = if mode == Mode::Break { broken } else { flat };
                stack.extend(chosen.iter().rev().map(|doc| (mode, doc)));
            }
        }
    }
}

/// The width of `text` in columns: its Unicode scalar values.
fn width(text: &str) -> usize {
    text.chars().count()
}

/// The column after `text` is written from `column`: past its last line break, if it
/// has one, or `column` plus its width.
fn advance(column: usize, text: &str) -> usize {
    match text.rsplit_once('\n') {
        Some((_, last)) => width(last),
        None => column + width(text),
    }
}

/// Ends the line, without trailing whitespace, and indents the next.
fn newline(out: &mut String, indent: usize) {
    trim_spaces(out);
    out.push('\n');
    for _ in 0..indent * INDENT {
        out.push(' ');
    }
}

fn trim_spaces(out: &mut String) {
    let trimmed = out.trim_end_matches(' ').len();
    out.truncate(trimmed);
}

#[cfg(test)]
mod tests {
    use super::{layout, print, Doc, INDENT, WIDTH};

    fn text(text: &str) -> Doc {
        Doc::text(text)
    }

    /// `f(a, b)` as a document: the arguments break one per line with a trailing comma.
    fn call(args: &[&str]) -> Doc {
        let mut inner = vec![Doc::SoftLine];
        for (at, arg) in args.iter().enumerate() {
            if at > 0 {
                inner.push(text(","));
                inner.push(Doc::Line);
            }
            inner.push(text(arg));
        }
        inner.push(Doc::if_break(vec![text(",")], Vec::new()));
        Doc::group(vec![
            text("f("),
            Doc::indent(inner),
            Doc::SoftLine,
            text(")"),
        ])
    }

    #[test]
    fn the_constructors_build_each_node() {
        assert_eq!(Doc::text("a"), Doc::Text("a".to_owned()));
        assert_eq!(Doc::group(vec![Doc::Line]), Doc::Group(vec![Doc::Line]));
        assert_eq!(
            Doc::indent(vec![Doc::SoftLine]),
            Doc::Indent(vec![Doc::SoftLine])
        );
        assert_eq!(
            Doc::if_break(vec![text("a")], vec![text("b")]),
            Doc::IfBreak {
                broken: vec![text("a")],
                flat: vec![text("b")]
            }
        );
        assert_eq!((WIDTH, INDENT), (100, 2));
    }

    #[test]
    fn text_is_written_as_is_and_counts_scalar_values() {
        assert_eq!(print(&[text("a"), text("é😀")]), "aé😀");
        let fits = "é".repeat(98);
        let docs = |tail: &str| vec![Doc::group(vec![text(&fits), Doc::Line, text(tail)])];
        assert_eq!(
            print(&docs("😀")),
            format!("{fits} 😀"),
            "98 + a space + 1 is 100: flat"
        );
        assert_eq!(
            print(&docs("😀😀")),
            format!("{fits}\n😀😀"),
            "one more column breaks"
        );
    }

    #[test]
    fn line_is_a_space_when_flat_and_a_break_when_broken() {
        let flat = vec![Doc::group(vec![text("a"), Doc::Line, text("b")])];
        assert_eq!(print(&flat), "a b");
        let long = "x".repeat(99);
        let broken = vec![Doc::group(vec![text(&long), Doc::Line, text("b")])];
        assert_eq!(print(&broken), format!("{long}\nb"));
    }

    #[test]
    fn soft_line_is_nothing_when_flat_and_a_break_when_broken() {
        assert_eq!(print(&[call(&["a", "b"])]), "f(a, b)");
        let long = "x".repeat(96);
        assert_eq!(
            print(&[call(&[&long, "b"])]),
            format!("f(\n  {long},\n  b,\n)")
        );
    }

    #[test]
    fn hard_line_always_breaks_and_breaks_its_group() {
        let docs = vec![Doc::group(vec![
            text("a"),
            Doc::HardLine,
            text("b"),
            Doc::Line,
            text("c"),
        ])];
        assert_eq!(print(&docs), "a\nb\nc", "the line after it breaks too");
        assert_eq!(print(&[text("a"), Doc::HardLine, text("b")]), "a\nb");
    }

    #[test]
    fn a_group_fits_at_exactly_the_width_and_breaks_one_past_it() {
        let left = "x".repeat(49);
        let right = "y".repeat(50);
        let docs = |right: &str| vec![Doc::group(vec![text(&left), Doc::Line, text(right)])];
        assert_eq!(
            print(&docs(&right)),
            format!("{left} {right}"),
            "100 columns fit"
        );
        let wider = format!("{right}y");
        assert_eq!(
            print(&docs(&wider)),
            format!("{left}\n{wider}"),
            "101 break"
        );
        assert_eq!(layout(&docs("y"), 51), format!("{left} y"));
        assert_eq!(layout(&docs("y"), 50), format!("{left}\ny"));
    }

    #[test]
    fn groups_break_outermost_first_and_an_inner_group_that_fits_stays_flat() {
        // 96 + 1 + 1 + 1 + 3 is 102: the outer breaks, and `a b` fits on its own line.
        let long = "x".repeat(96);
        let inner = Doc::group(vec![text("a"), Doc::Line, text("b")]);
        let docs = vec![Doc::group(vec![
            text(&long),
            Doc::Line,
            text("y"),
            Doc::Line,
            inner.clone(),
        ])];
        assert_eq!(
            print(&docs),
            format!("{long}\ny\na b"),
            "the outer breaks at its own lines; the inner fits on its line"
        );
        let longer = "x".repeat(99);
        let docs = vec![Doc::group(vec![
            text(&long),
            Doc::Line,
            Doc::group(vec![text(&longer), Doc::Line, text("b")]),
        ])];
        assert_eq!(
            print(&docs),
            format!("{long}\n{longer}\nb"),
            "an inner group that does not fit (99 + 1 + 1) breaks as well"
        );
        let docs = vec![Doc::group(vec![text("a"), Doc::Line, inner])];
        assert_eq!(print(&docs), "a a b", "a flat outer keeps the inner flat");
    }

    #[test]
    fn an_over_long_token_sits_on_its_own_line_without_breaking_what_follows() {
        let long = "x".repeat(120);
        let docs = vec![
            call(&[&long, "b"]),
            Doc::HardLine,
            Doc::group(vec![text("c"), Doc::Line, text("d")]),
        ];
        assert_eq!(
            print(&docs),
            format!("f(\n  {long},\n  b,\n)\nc d"),
            "the call breaks around the token; the next group is untouched"
        );
        assert_eq!(
            print(&[Doc::group(vec![text("x = "), text(&long)])]),
            format!("x = {long}"),
            "with no break to take, the token stays where it is"
        );
    }

    #[test]
    fn indent_adds_two_spaces_per_level_to_the_breaks_inside_it() {
        let docs = vec![Doc::group(vec![
            text("{"),
            Doc::indent(vec![
                Doc::HardLine,
                text("a"),
                Doc::indent(vec![Doc::HardLine, text("b")]),
                Doc::HardLine,
                text("c"),
            ]),
            Doc::HardLine,
            text("}"),
        ])];
        assert_eq!(print(&docs), "{\n  a\n    b\n  c\n}");
    }

    #[test]
    fn if_break_chooses_by_the_enclosing_group() {
        let docs = vec![Doc::group(vec![
            text("["),
            Doc::if_break(vec![text("broken")], vec![text("flat")]),
            text("]"),
        ])];
        assert_eq!(print(&docs), "[flat]");
        let docs = vec![Doc::group(vec![
            text("["),
            Doc::if_break(vec![text("broken")], vec![text("flat")]),
            Doc::HardLine,
            text("]"),
        ])];
        assert_eq!(print(&docs), "[broken\n]");
        assert_eq!(
            print(&[call(&["a"])]),
            "f(a)",
            "no trailing comma when flat"
        );
    }

    #[test]
    fn indentation_inside_a_broken_group_inside_a_flat_parent() {
        // `x = [ … ]`: the assignment's own text stays on one line, the array breaks.
        let long = "x".repeat(95);
        let array = Doc::group(vec![
            text("["),
            Doc::indent(vec![
                Doc::SoftLine,
                text(&long),
                text(","),
                Doc::Line,
                text("1"),
                Doc::if_break(vec![text(",")], Vec::new()),
            ]),
            Doc::SoftLine,
            text("]"),
        ]);
        let docs = vec![Doc::group(vec![text("x ="), text(" "), array])];
        assert_eq!(print(&docs), format!("x = [\n  {long},\n  1,\n]"));
    }

    #[test]
    fn an_empty_document_prints_nothing() {
        assert_eq!(print(&[]), "");
        assert_eq!(
            print(&[Doc::group(Vec::new()), Doc::indent(Vec::new())]),
            ""
        );
        assert_eq!(print(&[Doc::if_break(Vec::new(), Vec::new())]), "");
    }

    #[test]
    fn a_text_with_its_own_line_breaks_ends_the_line_where_it_breaks() {
        let template = "`a\nbb`";
        let docs = vec![Doc::group(vec![
            text(template),
            Doc::Line,
            text(&"y".repeat(96)),
        ])];
        assert_eq!(
            print(&docs),
            format!("{template} {}", "y".repeat(96)),
            "the column after it is 3, so 96 more fit"
        );
        let docs = vec![Doc::group(vec![
            text(&"x".repeat(99)),
            text(template),
            Doc::Line,
            text("z"),
        ])];
        assert_eq!(
            print(&docs),
            format!("{}{template}\nz", "x".repeat(99)),
            "its first line does not fit, so the group breaks"
        );
        let docs = vec![Doc::group(vec![
            text(&"x".repeat(98)),
            text(template),
            Doc::Line,
            text("z"),
        ])];
        assert_eq!(
            print(&docs),
            format!("{}{template} z", "x".repeat(98)),
            "its first line fits, and what follows its last starts afresh"
        );
    }

    #[test]
    fn what_follows_a_group_on_its_line_counts_and_a_break_ends_the_count() {
        let group = Doc::group(vec![text("a"), Doc::Line, text("b")]);
        let long = "x".repeat(98);
        assert_eq!(
            print(&[group.clone(), text(&long)]),
            format!("a\nb{long}"),
            "`a b` plus 98 is 101: the group breaks"
        );
        assert_eq!(
            print(&[group.clone(), Doc::HardLine, text(&long)]),
            format!("a b\n{long}"),
            "a break between them ends the line"
        );
        let outer = vec![Doc::group(vec![group.clone(), Doc::Line, text(&long)])];
        assert_eq!(
            print(&outer),
            format!("a b\n{long}"),
            "the outer breaks, the inner fits"
        );
        assert_eq!(
            print(&[group, Doc::SoftLine, text(&long)]),
            format!("a b\n{long}"),
            "a soft break at the top level is a break"
        );
    }

    #[test]
    fn an_if_break_in_the_rest_of_the_line_counts_as_its_own_group_is_laid_out() {
        // The outer group is broken (a hard break), so the `if_break` after the inner
        // group adds its broken text to the inner group's line.
        let docs = |tail: usize| {
            vec![Doc::group(vec![
                Doc::group(vec![text("a"), Doc::Line, text("b")]),
                Doc::if_break(vec![text(&"x".repeat(tail))], Vec::new()),
                Doc::HardLine,
            ])]
        };
        assert_eq!(
            print(&docs(97)),
            format!("a b{}\n", "x".repeat(97)),
            "3 + 97 fit"
        );
        assert_eq!(
            print(&docs(98)),
            format!("a\nb{}\n", "x".repeat(98)),
            "3 + 98 do not"
        );
    }

    #[test]
    fn the_space_of_a_flat_line_counts_toward_what_follows() {
        let docs = vec![
            Doc::group(vec![text("a"), Doc::Line, text("b")]),
            Doc::group(vec![Doc::Line, text(&"z".repeat(97))]),
        ];
        assert_eq!(
            print(&docs),
            format!("a b\n{}", "z".repeat(97)),
            "`a b` is 3 columns, so a space and 97 more do not fit"
        );
        let docs = vec![
            Doc::group(vec![text("a"), Doc::Line, text("b")]),
            Doc::group(vec![Doc::Line, text(&"z".repeat(96))]),
        ];
        assert_eq!(print(&docs), format!("a b {}", "z".repeat(96)), "96 do");
    }

    #[test]
    fn the_column_after_a_break_is_the_indentation() {
        let docs = vec![
            Doc::HardLine,
            Doc::group(vec![text("a"), Doc::Line, text(&"z".repeat(98))]),
        ];
        assert_eq!(
            print(&docs),
            format!("\na {}", "z".repeat(98)),
            "column 0: 100 fit"
        );
        let docs = vec![Doc::indent(vec![
            Doc::HardLine,
            Doc::group(vec![text("a"), Doc::Line, text(&"z".repeat(96))]),
        ])];
        assert_eq!(
            print(&docs),
            format!("\n  a {}", "z".repeat(96)),
            "column 2: 98 more fit"
        );
        let docs = vec![Doc::indent(vec![
            Doc::HardLine,
            Doc::group(vec![text("a"), Doc::Line, text(&"z".repeat(97))]),
        ])];
        assert_eq!(
            print(&docs),
            format!("\n  a\n  {}", "z".repeat(97)),
            "and 99 do not"
        );
    }

    #[test]
    fn no_line_ends_in_whitespace() {
        assert_eq!(print(&[text("a "), Doc::HardLine, text("b")]), "a\nb");
        assert_eq!(
            print(&[Doc::indent(vec![Doc::HardLine, Doc::HardLine, text("b")])]),
            "\n\n  b",
            "a blank line is empty"
        );
        assert_eq!(print(&[text("a"), Doc::indent(vec![Doc::HardLine])]), "a\n");
        assert_eq!(print(&[text("a  ")]), "a", "nor the last");
    }

    #[test]
    fn a_group_after_an_over_long_token_breaks_and_a_line_in_a_flat_group_stays_a_space() {
        let long = "x".repeat(120);
        let docs = vec![
            text(&long),
            Doc::group(vec![text("a"), Doc::Line, text("b")]),
        ];
        assert_eq!(
            print(&docs),
            format!("{long}a\nb"),
            "nothing remains of the line"
        );
        let docs = vec![Doc::group(vec![
            text("a"),
            Doc::Line,
            Doc::group(vec![Doc::HardLine, text("b")]),
        ])];
        assert_eq!(
            print(&docs),
            "a\n\nb",
            "a hard break inside breaks the outer too"
        );
    }
}
