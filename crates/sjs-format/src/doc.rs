//! The document IR and its layout (STORIES.md US-040, SPEC.md §9.1): text; line breaks
//! that are a space, or nothing, unless their group is broken; groups that stay flat
//! when they fit in what remains of the line and otherwise break, outermost first;
//! indentation; and content chosen by whether the enclosing group is broken. The
//! algorithm is Wadler's prettier as Prettier runs it: a stack of commands, each a
//! document in a mode at an indentation level, and a look-ahead asking whether a
//! group's content, flat, fits before the line's next break. Width counts Unicode scalar
//! values, as diagnostics count columns (SPEC.md §13.1). Comments (US-046) add text that
//! goes to the end of its line, a break unless the line is still empty, and a paragraph
//! of words filled to the width.

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
    /// Text written at the end of a line: the line before when nothing but indentation
    /// has been written on the current one, else the current line when it ends (a `//`
    /// comment after what ends a line). No group holding one is flat.
    LineSuffix(String),
    /// A line break, unless nothing but indentation has been written on the line (a
    /// comment that must begin a line). No group holding one is flat.
    LineStart,
    /// Words filled into lines of the width, a single space between them, each line
    /// beginning with `prefix` after the indentation (a doc comment's paragraph). No
    /// group holding one is flat.
    Paragraph { prefix: String, words: Vec<String> },
    /// The suffixes waiting for the end of the line, each on a line of its own here
    /// instead (a comment after a block's `{` goes above the block's first item).
    HangSuffixes,
    /// A line break when suffixes were just hung, and nothing otherwise (an empty block
    /// closes on its own line only when a comment hangs inside it).
    HangEnd,
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

/// The text being written: the column it is at, the suffixes waiting for the end of
/// the line, and the comments that lost a line's end and wait to begin the next line.
struct Printer {
    out: String,
    column: usize,
    /// Text waiting for the end of the line.
    suffixes: Vec<String>,
    /// Comments a `//` comment kept from ending their line, each with that line's
    /// indentation, waiting to begin the next line that holds text.
    leading: Vec<(String, String)>,
    /// Whether a `//` comment is on the current line.
    line_comment: bool,
    /// Whether a `//` comment ended the line before.
    ended_by_comment: bool,
    /// Whether suffixes were just hung on lines of their own.
    hung: bool,
}

impl Printer {
    fn new() -> Self {
        Printer {
            out: String::new(),
            column: 0,
            suffixes: Vec::new(),
            leading: Vec::new(),
            line_comment: false,
            ended_by_comment: false,
            hung: false,
        }
    }

    /// `text`, after the lines of the comments waiting to begin this one.
    fn text(&mut self, text: &str) {
        if !self.leading.is_empty() {
            let at = self.out.rfind('\n').map_or(0, |at| at + 1);
            let mut lines = String::new();
            for (indent, comment) in self.leading.drain(..) {
                lines.push_str(&indent);
                lines.push_str(&comment);
                lines.push('\n');
            }
            self.out.insert_str(at, &lines);
        }
        self.line_comment |= is_line_comment(text);
        self.out.push_str(text);
        self.column = advance(self.column, text);
    }

    /// Ends the line, without trailing whitespace and with the suffixes waiting for it,
    /// and indents the next.
    fn newline(&mut self, indent: usize) {
        trim_spaces(&mut self.out);
        self.flush();
        self.out.push('\n');
        for _ in 0..indent * INDENT {
            self.out.push(' ');
        }
        self.column = indent * INDENT;
        self.hung = false;
    }

    /// The suffixes waiting, at the end of the line. Once a `//` comment ends it, the
    /// rest wait to begin the next line that holds text, indented like this one.
    fn flush(&mut self) {
        let indent = self.indentation(self.out.len());
        let mut ended = self.line_comment;
        for suffix in std::mem::take(&mut self.suffixes) {
            if ended {
                self.leading
                    .push((indent.clone(), suffix.trim_start().to_owned()));
            } else {
                ended = is_line_comment(&suffix);
                self.out.push_str(&suffix);
            }
        }
        self.ended_by_comment = ended;
        self.line_comment = false;
    }

    /// The indentation of the line that `end` is on.
    fn indentation(&self, end: usize) -> String {
        let from = self.out[..end].rfind('\n').map_or(0, |at| at + 1);
        self.out[from..end]
            .chars()
            .take_while(|ch| *ch == ' ')
            .collect()
    }

    /// Whether nothing but indentation has been written on the current line.
    fn at_line_start(&self) -> bool {
        let from = self.out.rfind('\n').map_or(0, |at| at + 1);
        self.out[from..].chars().all(|ch| ch == ' ')
    }

    /// `text` at the end of the line before when the current one is still empty (or,
    /// when a `//` comment already ends that line, waiting to begin this one), else at
    /// the end of the current line once it ends.
    fn suffix(&mut self, text: &str) {
        if self.at_line_start() {
            if let Some(at) = self.out.rfind('\n') {
                if self.ended_by_comment {
                    let indent = self.indentation(at);
                    self.leading.push((indent, text.trim_start().to_owned()));
                } else {
                    self.out.insert_str(at, text);
                    self.ended_by_comment = is_line_comment(text);
                }
                return;
            }
        }
        self.suffixes.push(text.to_owned());
    }

    /// A line break, unless the line is still empty.
    fn line_start(&mut self, indent: usize) {
        if !self.at_line_start() {
            self.newline(indent);
        }
    }

    /// Each suffix waiting for the end of the line on a line of its own, without the
    /// spaces that set it off.
    fn hang(&mut self, indent: usize) {
        let pending = std::mem::take(&mut self.suffixes);
        let any = !pending.is_empty();
        for suffix in pending {
            self.newline(indent);
            self.text(suffix.trim_start());
        }
        self.hung = any;
    }

    /// A line break after hung suffixes, and nothing otherwise.
    fn hang_end(&mut self, indent: usize) {
        if self.hung {
            self.newline(indent);
        }
    }

    /// `prefix`, then `words` a space apart while they fit in `width`, each further line
    /// beginning with the indentation and `prefix`; a word too wide for a line of its
    /// own exceeds it.
    fn paragraph(&mut self, indent: usize, prefix: &str, words: &[String], width: usize) {
        self.text(prefix);
        for (at, word) in words.iter().enumerate() {
            if at > 0 {
                if self.column + 1 + self::width(word) > width {
                    self.newline(indent);
                    self.text(prefix);
                } else {
                    self.text(" ");
                }
            }
            self.text(word);
        }
    }

    /// The text, its last line ended by the suffixes waiting and followed by the lines of
    /// the comments still waiting, without trailing whitespace.
    fn finish(mut self) -> String {
        trim_spaces(&mut self.out);
        self.flush();
        for (indent, comment) in std::mem::take(&mut self.leading) {
            self.out.push('\n');
            self.out.push_str(&indent);
            self.out.push_str(&comment);
        }
        trim_spaces(&mut self.out);
        self.out
    }
}

/// Whether `text` is a `//` comment, which runs to the end of its line.
fn is_line_comment(text: &str) -> bool {
    text.trim_start().starts_with("//")
}

/// Lays `docs` out at [`WIDTH`].
#[must_use]
pub fn print(docs: &[Doc]) -> String {
    layout(docs, WIDTH)
}

/// Lays `docs` out at `width` columns. No line ends in whitespace.
#[must_use]
pub fn layout(docs: &[Doc], width: usize) -> String {
    let mut printer = Printer::new();
    let mut cmds: Vec<Cmd<'_>> = Vec::new();
    push(&mut cmds, docs, 0, Mode::Break);
    while let Some(Cmd { indent, mode, doc }) = cmds.pop() {
        match doc {
            Doc::Text(text) => printer.text(text),
            Doc::Line if mode == Mode::Flat => printer.text(" "),
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::Line | Doc::SoftLine | Doc::HardLine => printer.newline(indent),
            Doc::Group(content) => {
                let remaining = width.saturating_sub(printer.column);
                let mode = if mode == Mode::Flat || fits(content, &cmds, remaining) {
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
            Doc::LineSuffix(text) => printer.suffix(text),
            Doc::LineStart => printer.line_start(indent),
            Doc::Paragraph { prefix, words } => printer.paragraph(indent, prefix, words, width),
            Doc::HangSuffixes => printer.hang(indent),
            Doc::HangEnd => printer.hang_end(indent),
        }
    }
    printer.finish()
}

/// Queues `content` so its first document comes off the stack next.
fn push<'a>(cmds: &mut Vec<Cmd<'a>>, content: &'a [Doc], indent: usize, mode: Mode) {
    for doc in content.iter().rev() {
        cmds.push(Cmd { indent, mode, doc });
    }
}

/// Whether `content`, laid out flat, followed by the commands still to come, fits in
/// `remaining` columns: measured up to the line's next break, which ends the line (a
/// break in the content to come, or inside a text). A hard break, a line start or a
/// paragraph inside the flat content means it cannot be flat, and so does a line
/// suffix, which otherwise counts toward the line it ends.
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
                if !take(&mut remaining, text) {
                    return false;
                }
            }
            Doc::Line if mode == Mode::Flat => {
                if !take(&mut remaining, " ") {
                    return false;
                }
            }
            Doc::SoftLine if mode == Mode::Flat => {}
            Doc::HangSuffixes | Doc::HangEnd => {}
            Doc::HardLine | Doc::LineStart | Doc::Paragraph { .. } | Doc::LineSuffix(_)
                if mode == Mode::Flat =>
            {
                return false;
            }
            Doc::Line | Doc::SoftLine | Doc::HardLine | Doc::LineStart | Doc::Paragraph { .. } => {
                return true;
            }
            Doc::LineSuffix(text) => {
                if !take(&mut remaining, text) {
                    return false;
                }
            }
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

/// Takes `text`'s width out of `remaining`; false when it does not fit.
fn take(remaining: &mut usize, text: &str) -> bool {
    match remaining.checked_sub(width(text)) {
        Some(left) => {
            *remaining = left;
            true
        }
        None => false,
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

    fn suffix(text: &str) -> Doc {
        Doc::LineSuffix(text.to_owned())
    }

    #[test]
    fn a_line_suffix_ends_its_line_and_breaks_its_group() {
        let list = Doc::group(vec![
            text("f("),
            Doc::indent(vec![
                Doc::SoftLine,
                text("a"),
                text(","),
                Doc::Line,
                suffix("  // x"),
                text("b"),
            ]),
            Doc::SoftLine,
            text(")"),
        ]);
        assert_eq!(
            print(&[list]),
            "f(\n  a,  // x\n  b\n)",
            "met at the start of a line, it ends the line before"
        );
        assert_eq!(
            print(&[
                text("a"),
                suffix("  // x"),
                text(" = 1"),
                Doc::HardLine,
                text("b")
            ]),
            "a = 1  // x\nb",
            "met inside a line, it waits for the line's end"
        );
        assert_eq!(
            print(&[text("a "), suffix("  // x")]),
            "a  // x",
            "or for the end of the document, trailing spaces dropped"
        );
        assert_eq!(
            print(&[suffix("// x"), text("a")]),
            "a// x",
            "first in the document, there is no line before"
        );
        let short = Doc::group(vec![text("a"), Doc::Line, suffix("  // x"), text("b")]);
        assert_eq!(
            print(&[short]),
            "a  // x\nb",
            "no group holding one is flat"
        );
        let rest = [
            Doc::group(vec![text("aaaa"), Doc::Line, text("bbbb")]),
            suffix("  // x"),
        ];
        assert_eq!(layout(&rest, 15), "aaaa bbbb  // x");
        assert_eq!(
            layout(&rest, 14),
            "aaaa\nbbbb  // x",
            "after a group, its width counts toward the group's line"
        );
    }

    #[test]
    fn a_line_start_breaks_unless_the_line_is_empty() {
        assert_eq!(print(&[text("a"), Doc::LineStart, text("b")]), "a\nb");
        assert_eq!(
            print(&[text("a"), Doc::HardLine, Doc::LineStart, text("b")]),
            "a\nb"
        );
        assert_eq!(print(&[Doc::LineStart, text("b")]), "b");
        assert_eq!(
            print(&[text("a "), Doc::LineStart, text("b")]),
            "a\nb",
            "no trailing whitespace"
        );
        let short = Doc::group(vec![text("a"), Doc::Line, Doc::LineStart, text("b")]);
        assert_eq!(print(&[short]), "a\nb", "no group holding one is flat");
        let rest = [
            Doc::group(vec![text("aaaa"), Doc::Line, text("bbbb")]),
            Doc::LineStart,
            text(&"x".repeat(20)),
        ];
        assert_eq!(
            layout(&rest, 9),
            "aaaa bbbb\nxxxxxxxxxxxxxxxxxxxx",
            "after a group, it ends the group's line"
        );
        let block = [
            text("{"),
            Doc::indent(vec![
                text(" x"),
                Doc::LineStart,
                text("// c"),
                Doc::HardLine,
                text("y"),
            ]),
            Doc::HardLine,
            text("}"),
        ];
        assert_eq!(
            print(&block),
            "{ x\n  // c\n  y\n}",
            "indented like its level"
        );
    }

    #[test]
    fn hanging_suffixes_take_lines_of_their_own() {
        let block = |suffixed: Doc| {
            vec![
                suffixed,
                text(" {"),
                Doc::indent(vec![Doc::HangSuffixes, Doc::HardLine, text("a")]),
                Doc::HardLine,
                text("}"),
            ]
        };
        assert_eq!(
            print(&block(Doc::group(vec![suffix("  // x"), text("f()")]))),
            "f() {\n  // x\n  a\n}"
        );
        assert_eq!(
            print(&block(text("f()"))),
            "f() {\n  a\n}",
            "nothing waiting, nothing written"
        );
        assert_eq!(
            print(&[
                suffix("  // x"),
                suffix("  /* y */"),
                text("{"),
                Doc::HangSuffixes,
                text("}")
            ]),
            "{\n// x\n/* y */}",
            "each on its line, in order"
        );
        let flat = Doc::group(vec![text("a"), Doc::HangSuffixes, Doc::Line, text("b")]);
        assert_eq!(print(&[flat]), "a b", "it does not break a group");
        let empty = |suffixed: Doc| {
            vec![
                suffixed,
                text(" {"),
                Doc::indent(vec![Doc::HangSuffixes]),
                Doc::HangEnd,
                text("}"),
            ]
        };
        assert_eq!(
            print(&empty(Doc::group(vec![suffix("  // x"), text("f()")]))),
            "f() {\n  // x\n}",
            "an empty block closes on its own line when a comment hangs in it"
        );
        assert_eq!(print(&empty(text("f()"))), "f() {}");
    }

    #[test]
    fn a_second_line_comment_at_a_line_end_begins_the_next_line_of_text() {
        assert_eq!(
            print(&[
                text("a"),
                suffix("  // x"),
                suffix("  // y"),
                Doc::HardLine,
                text("b")
            ]),
            "a  // x\n// y\nb"
        );
        assert_eq!(
            print(&[
                text("a"),
                suffix("  // x"),
                suffix("  // y"),
                Doc::HardLine,
                Doc::HardLine,
                text("b"),
            ]),
            "a  // x\n\n// y\nb",
            "after the blank line, where the layout of items puts a comment above an item"
        );
        assert_eq!(
            print(&[
                text("{"),
                Doc::indent(vec![
                    Doc::HardLine,
                    text("a"),
                    suffix("  // x"),
                    suffix("  // y")
                ]),
                Doc::HardLine,
                text("}"),
            ]),
            "{\n  a  // x\n  // y\n}",
            "indented like the line it could not end"
        );
        assert_eq!(
            print(&[
                text("a"),
                suffix("  // x"),
                suffix(" /* y */"),
                Doc::HardLine,
                text("b")
            ]),
            "a  // x\n/* y */\nb"
        );
        assert_eq!(
            print(&[text("a"), suffix(" /* y */"), suffix("  // x")]),
            "a /* y */  // x",
            "a block comment lets a line comment follow it"
        );
        assert_eq!(
            print(&[text("a"), suffix("  // x"), suffix("  // y")]),
            "a  // x\n// y",
            "at the end of the document"
        );
        assert_eq!(
            print(&[
                text("a,"),
                suffix("  // p"),
                Doc::HardLine,
                suffix("  // x"),
                text("b")
            ]),
            "a,  // p\n// x\nb",
            "met at the start of a line whose line before a comment ends"
        );
        assert_eq!(
            print(&[text("// own"), Doc::HardLine, suffix("  // x"), text("b")]),
            "// own\n// x\nb",
            "or that is a comment"
        );
    }

    fn paragraph(text: &str) -> Doc {
        Doc::Paragraph {
            prefix: " * ".to_owned(),
            words: text.split(' ').map(str::to_owned).collect(),
        }
    }

    #[test]
    fn a_paragraph_fills_its_words_to_the_width_behind_its_prefix() {
        let docs = [
            text("/**"),
            Doc::HardLine,
            paragraph("one two three four"),
            Doc::HardLine,
            text(" */"),
        ];
        assert_eq!(layout(&docs, 12), "/**\n * one two\n * three\n * four\n */");
        assert_eq!(layout(&[paragraph("exactly fits")], 15), " * exactly fits");
        assert_eq!(
            layout(&[paragraph("exactly fits")], 14),
            " * exactly\n * fits"
        );
        assert_eq!(
            layout(
                &[Doc::indent(vec![
                    Doc::HardLine,
                    paragraph("a verylongword b")
                ])],
                10
            ),
            "\n   * a\n   * verylongword\n   * b",
            "indented, and a word too wide stands alone"
        );
        let short = Doc::group(vec![text("a"), Doc::Line, paragraph("p")]);
        assert_eq!(print(&[short]), "a\n * p", "no group holding one is flat");
        let rest = [
            Doc::group(vec![text("aaaa"), Doc::Line, text("bbbb")]),
            paragraph("x"),
        ];
        assert_eq!(
            layout(&rest, 9),
            "aaaa bbbb * x",
            "after a group, it ends the group's line"
        );
    }
}
