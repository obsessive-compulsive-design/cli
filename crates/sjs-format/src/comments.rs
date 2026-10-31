//! Comments (US-046, SPEC.md §9.5). A comment lives in the leading trivia of the token
//! after it. The layout of items ([`crate::stmt`]) places the comments above an item
//! and at the end of its line; this module places every other one: a `//` comment on
//! the line of the token before goes to the end of that line, a block comment stays
//! inline where it is, and a comment on a line of its own keeps a line of its own.
//! `//x` becomes `// x`, except for a comment beginning with `/` or `*` (a ruler, ASCII
//! art), and a `/** */` doc comment on a line of its own is reflowed to the width,
//! paragraph by paragraph, its blank lines, tag lines, fenced blocks and other Markdown
//! structure kept as written. `Formatter::long_lines()` reports S006 for a comment on a
//! line the layout cannot bring under the width.

use sjs_diag::{Code, Diagnostic, SourceFile, Span};
use sjs_syntax::{parse, Element, NodeKind, NodeRef, Token, TokenRef, TriviaKind};

use crate::doc::{Doc, WIDTH};
use crate::expr::Formatter;
use crate::imports::sort_imports;
use crate::stmt::{items_of, Body};

const MESSAGE: &str = "a comment on a line longer than 100 columns: the layout never breaks a \
                       comment (SPEC.md §9.1, §9.5)";

/// A comment in a token's leading trivia, as the layout places it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Where the comment is.
    span: Span,
    kind: Placement,
}

/// Where the layout writes a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// On the line of the token before, ending it: at the end of that line.
    Trailing,
    /// A block comment the token follows on its line, written where it is; `own_line`
    /// when a line break precedes it.
    Inline { own_line: bool },
    /// On a line of its own.
    Own,
}

/// A part of a doc comment's body.
enum Part {
    /// A blank line.
    Blank,
    /// A line kept as written: a tag, a fence or its content, a list item, a heading, a
    /// table row, a quote, an indented line.
    Verbatim(String),
    /// Prose, as words.
    Paragraph(Vec<String>),
}

impl Formatter<'_> {
    /// The documents for the comments before `token`, written before it.
    pub(crate) fn before(&self, token: TokenRef<'_>) -> Vec<Doc> {
        self.prefix(&placed(token))
    }

    /// The documents for the comments before `element`'s first token.
    pub(crate) fn before_element(&self, element: Element<'_>) -> Vec<Doc> {
        match element {
            Element::Token(token) => self.before(token),
            Element::Node(node) => self.before_node(node),
        }
    }

    /// The documents for the comments before a node the layout copies as written; none
    /// when the node begins an item.
    fn before_node(&self, node: NodeRef<'_>) -> Vec<Doc> {
        if leads_an_item(node) {
            return Vec::new();
        }
        node.tokens()
            .first()
            .map_or_else(Vec::new, |token| self.prefix(&placed_in(token)))
    }

    /// `placed` as what precedes an item: a trailing comment goes to the end of the line
    /// before, an inline one stands before the item with a space after it, one of its
    /// own takes a line.
    pub(crate) fn prefix(&self, placed: &[Placed]) -> Vec<Doc> {
        let mut docs = Vec::new();
        for comment in placed {
            match comment.kind {
                Placement::Trailing => docs.push(self.suffix(comment.span)),
                Placement::Inline { own_line } => {
                    if own_line {
                        docs.push(Doc::LineStart);
                    }
                    docs.push(Doc::text(format!("{} ", self.comment_text(comment.span))));
                }
                Placement::Own => {
                    docs.push(Doc::LineStart);
                    docs.extend(self.comment_block(comment.span));
                    docs.push(Doc::HardLine);
                }
            }
        }
        docs
    }

    /// The comments before a comma or a closing bracket, split into what ends the item
    /// before it (an inline comment on the same line) and what moves past it (the rest;
    /// a trailing comment reaches the end of its line from either side).
    pub(crate) fn split(&self, token: TokenRef<'_>) -> (Vec<Doc>, Vec<Placed>) {
        self.split_placed(&placed(token))
    }

    pub(crate) fn split_placed(&self, placed: &[Placed]) -> (Vec<Doc>, Vec<Placed>) {
        let mut suffix = Vec::new();
        let mut rest = Vec::new();
        for comment in placed {
            if comment.kind == (Placement::Inline { own_line: false }) {
                suffix.push(Doc::text(format!(" {}", self.comment_text(comment.span))));
            } else {
                rest.push(*comment);
            }
        }
        (suffix, rest)
    }

    /// `placed` before a closing bracket, hanging inside it: a trailing comment ends the
    /// line before, any other takes a line of its own.
    pub(crate) fn dangling(&self, placed: &[Placed]) -> Vec<Doc> {
        let mut docs = Vec::new();
        for comment in placed {
            match comment.kind {
                Placement::Trailing => docs.push(self.suffix(comment.span)),
                Placement::Inline { .. } => {
                    docs.push(Doc::LineStart);
                    docs.push(Doc::text(self.comment_text(comment.span)));
                }
                Placement::Own => {
                    docs.push(Doc::LineStart);
                    docs.extend(self.comment_block(comment.span));
                }
            }
        }
        docs
    }

    /// The comments before a token the layout drops (a redundant `)`, a final `;`), all
    /// at the end of the line: an inline one after what precedes it, any other at the
    /// line's end, where no code can follow it.
    pub(crate) fn suffixes(&self, token: TokenRef<'_>) -> Vec<Doc> {
        placed(token)
            .into_iter()
            .map(|comment| match comment.kind {
                Placement::Inline { own_line: false } => {
                    Doc::text(format!(" {}", self.comment_text(comment.span)))
                }
                _ => self.suffix(comment.span),
            })
            .collect()
    }

    fn suffix(&self, span: Span) -> Doc {
        Doc::LineSuffix(self.at_line_end(span))
    }

    /// A comment as it ends a line of code: two spaces before a `//` comment (SPEC.md
    /// §9.5), one before a block comment.
    pub(crate) fn at_line_end(&self, span: Span) -> String {
        let text = self.comment_text(span);
        let gap = if text.starts_with("//") { "  " } else { " " };
        format!("{gap}{text}")
    }

    /// A comment's text as the layout writes it: `// x` for `//x`, a block comment with
    /// its lines' trailing whitespace dropped.
    pub(crate) fn comment_text(&self, span: Span) -> String {
        let text = self.text(span);
        if text.starts_with("//") {
            line_comment(text)
        } else {
            block_comment(text)
        }
    }

    /// A comment on a line of its own: a doc comment reflowed, any other as
    /// [`Self::comment_text`].
    pub(crate) fn comment_block(&self, span: Span) -> Vec<Doc> {
        let text = self.text(span);
        if text.starts_with("/**") && text.len() > 4 {
            doc_comment(text)
        } else {
            vec![Doc::text(self.comment_text(span))]
        }
    }

    /// S006 for every comment on a line of the canonical text longer than the width: the
    /// layout never breaks a comment, so the line stays long. The canonical text is
    /// parsed again and its comments paired with the source's in order: the layout
    /// writes every comment of a parsed item, and the comment before a token the
    /// recovery left bare, which it drops, is left out of both.
    #[must_use]
    pub fn long_lines(&self) -> Vec<Diagnostic> {
        SourceFile::new(self.file.path(), self.module().into_bytes())
            .ok()
            .map_or_else(Vec::new, |formatted| self.compare_comments(&formatted))
    }

    fn compare_comments(&self, formatted: &SourceFile) -> Vec<Diagnostic> {
        let (tree, _) = parse(formatted);
        let source = self.written_comments();
        let written = comments_of(tree.tokens().iter());
        source
            .iter()
            .zip(&written)
            .filter(|(_, written)| {
                (written.start.line..=written.end.line).any(|line| {
                    formatted
                        .line(line)
                        .is_some_and(|line| line.text.chars().count() > WIDTH)
                })
            })
            .map(|(&span, _)| Diagnostic::new(Code::S006, self.file.path(), span, MESSAGE))
            .collect()
    }

    /// The spans of the source's comments in the order the layout writes them: item by
    /// item, imports in their order, each item's comments above it, then those inside
    /// it, then those at the end of its line.
    fn written_comments(&self) -> Vec<Span> {
        let mut items = items_of(self.tree.root(), true);
        sort_imports(&mut items);
        let tokens = self.tree.tokens();
        let mut spans = Vec::new();
        for item in &items {
            spans.extend(item.above.iter().filter_map(|line| line.comment()));
            match item.body {
                Body::Node(node) => {
                    let inner = node.tokens().get(1..).unwrap_or(&[]);
                    spans.extend(comments_of(inner.iter()));
                }
                Body::Stray(from, to) => {
                    let inner = tokens
                        .iter()
                        .filter(|token| token.span.byte_start > from && token.span.byte_end <= to);
                    spans.extend(comments_of(inner));
                }
                Body::End => {}
            }
            spans.extend(item.trailing.iter().copied());
        }
        spans
    }
}

/// The spans of the comments before each of `tokens`, in order.
fn comments_of<'t>(tokens: impl Iterator<Item = &'t Token>) -> Vec<Span> {
    tokens
        .flat_map(|token| token.leading.iter())
        .filter(|trivia| {
            matches!(
                trivia.kind,
                TriviaKind::LineComment
                    | TriviaKind::BlockComment { .. }
                    | TriviaKind::DocComment { .. }
            )
        })
        .map(|trivia| trivia.span)
        .collect()
}

/// The comments before `token`, placed; none for a token that begins an item, whose
/// comments the layout of items places.
pub(crate) fn placed(token: TokenRef<'_>) -> Vec<Placed> {
    if begins_an_item(token) {
        return Vec::new();
    }
    placed_in(token.token())
}

/// The comments in `token`'s leading trivia, placed.
fn placed_in(token: &Token) -> Vec<Placed> {
    let leading = &token.leading;
    let mut placed = Vec::new();
    let mut after_break = false;
    for (at, trivia) in leading.iter().enumerate() {
        match trivia.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::LineTerminator(_) => after_break = true,
            TriviaKind::LineComment => placed.push(Placed {
                span: trivia.span,
                kind: if after_break {
                    Placement::Own
                } else {
                    Placement::Trailing
                },
            }),
            TriviaKind::BlockComment { .. } | TriviaKind::DocComment { .. } => {
                let break_follows = leading[at..]
                    .iter()
                    .any(|later| matches!(later.kind, TriviaKind::LineTerminator(_)));
                placed.push(Placed {
                    span: trivia.span,
                    kind: match (after_break, break_follows) {
                        (false, true) => Placement::Trailing,
                        (true, true) => Placement::Own,
                        (own_line, false) => Placement::Inline { own_line },
                    },
                });
            }
        }
    }
    placed
}

/// Whether `token` begins an item of the file, a block, a class body or a switch, so
/// that the layout of items places the comments before it.
fn begins_an_item(token: TokenRef<'_>) -> bool {
    let parent = token.parent();
    parent.children().next() == Some(Element::Token(token)) && leads_an_item(parent)
}

/// Whether `node` begins an item: it is one, or it begins the node that begins one.
fn leads_an_item(mut node: NodeRef<'_>) -> bool {
    loop {
        match node.parent() {
            Some(parent) if is_container(parent.kind()) => return true,
            Some(parent) if parent.children().next() == Some(Element::Node(node)) => {
                node = parent;
            }
            _ => return false,
        }
    }
}

fn is_container(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Module | NodeKind::Block | NodeKind::ClassBody | NodeKind::SwitchStmt
    )
}

/// `//x` → `// x`; a comment beginning with `/` or `*` (a ruler, ASCII art) as written;
/// trailing whitespace dropped.
fn line_comment(text: &str) -> String {
    let body = text[2..].trim_end();
    match body.chars().next() {
        Some(first) if !first.is_whitespace() && first != '/' && first != '*' => {
            format!("// {body}")
        }
        _ => format!("//{body}"),
    }
}

/// A block comment with each line's trailing whitespace dropped.
fn block_comment(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// `/** … */` reflowed: on one line it stays one when it fits and otherwise takes the
/// block form, `/**`, then ` * ` lines, then ` */`, in which prose is filled to the
/// width paragraph by paragraph while blank lines, tag lines (`@param`), fenced blocks,
/// list items, headings, table rows, quotes and indented lines stay as written.
fn doc_comment(text: &str) -> Vec<Doc> {
    let inner = &text[3..text.len() - 2];
    let lines: Vec<&str> = inner.lines().collect();
    if lines.len() <= 1 {
        let body = inner.trim();
        let one_line = if body.is_empty() {
            "/** */".to_owned()
        } else {
            format!("/** {body} */")
        };
        return vec![Doc::Group(vec![Doc::IfBreak {
            broken: block_form(&parts_of(&[body])),
            flat: vec![Doc::text(one_line)],
        }])];
    }
    let body: Vec<&str> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            if at == 0 {
                line.trim_start()
            } else {
                strip_star(line)
            }
            .trim_end()
        })
        .collect();
    block_form(&parts_of(&body))
}

/// A doc comment line without its leading ` * `: the star is dropped when a space or
/// the line's end follows it, so `**bold**` keeps its stars.
fn strip_star(line: &str) -> &str {
    let line = line.trim_start();
    match line.strip_prefix('*') {
        Some(rest) if rest.is_empty() => rest,
        Some(rest) => rest.strip_prefix(' ').unwrap_or(line),
        None => line,
    }
}

/// The body lines of a doc comment as parts: prose gathered into paragraphs, blank
/// lines collapsed to one and dropped at either end, everything structured as written.
fn parts_of(lines: &[&str]) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    let mut fenced = false;
    for &line in lines {
        if fenced || is_fence(line) {
            fenced ^= is_fence(line);
            parts.push(Part::Verbatim(line.to_owned()));
        } else if line.is_empty() {
            if matches!(parts.last(), Some(Part::Verbatim(_) | Part::Paragraph(_))) {
                parts.push(Part::Blank);
            }
        } else if line.starts_with("  ") || is_marker(first_word(line)) {
            parts.push(Part::Verbatim(line.to_owned()));
        } else if let Some(Part::Paragraph(words)) = parts.last_mut() {
            extend_words(words, line);
        } else {
            let mut words = Vec::new();
            extend_words(&mut words, line);
            parts.push(Part::Paragraph(words));
        }
    }
    if matches!(parts.last(), Some(Part::Blank)) {
        parts.pop();
    }
    parts
}

fn first_word(line: &str) -> &str {
    line.split(' ').next().unwrap_or("")
}

fn is_fence(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

/// Whether `word` would start a Markdown block: a tag, a heading, a table row, a quote,
/// a list item.
fn is_marker(word: &str) -> bool {
    let numbered = word
        .strip_suffix(['.', ')'])
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()));
    word.starts_with(['@', '#', '|', '>']) || matches!(word, "-" | "+" | "*") || numbered
}

/// `line`'s words onto `words`; a word that would start a block if it began a line is
/// glued to the word before it so that it never does.
fn extend_words(words: &mut Vec<String>, line: &str) {
    for word in line.split_whitespace() {
        match words.last_mut() {
            Some(last) if is_marker(word) => {
                last.push(' ');
                last.push_str(word);
            }
            _ => words.push(word.to_owned()),
        }
    }
}

/// The block form of a doc comment: `/**`, a line per part, ` */`.
fn block_form(parts: &[Part]) -> Vec<Doc> {
    let mut docs = vec![Doc::text("/**")];
    for part in parts {
        docs.push(Doc::HardLine);
        docs.push(match part {
            Part::Blank => Doc::text(" *"),
            Part::Verbatim(line) => Doc::text(format!(" * {line}")),
            Part::Paragraph(words) => Doc::Paragraph {
                prefix: " * ".to_owned(),
                words: words.clone(),
            },
        });
    }
    docs.push(Doc::HardLine);
    docs.push(Doc::text(" */"));
    docs
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    /// The S006 findings: line and column of each.
    fn long(text: &str) -> Vec<(u32, u32, bool)> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .long_lines()
            .into_iter()
            .map(|d| {
                assert_eq!(d.code, Code::S006);
                (
                    d.span.start.line,
                    d.span.start.column,
                    d.replacement.is_some(),
                )
            })
            .collect()
    }

    /// Formats twice: the second pass must change nothing.
    fn stable(text: &str) -> String {
        let out = module(text);
        assert_eq!(module(&out), out, "a second pass changes:\n{out}");
        out
    }

    #[test]
    fn line_comments_get_their_space_and_trailing_ones_their_two() {
        assert_eq!(stable("//x\nf()"), "// x\nf()\n");
        assert_eq!(stable("f() //x"), "f()  // x\n");
        assert_eq!(
            stable("f() // x   \n"),
            "f()  // x\n",
            "trailing whitespace dropped"
        );
        assert_eq!(
            stable("//   x\nf()"),
            "//   x\nf()\n",
            "a space is already there"
        );
        assert_eq!(
            stable("/////\n//*****\n//\nf()"),
            "/////\n//*****\n//\nf()\n",
            "rulers and the empty comment stay"
        );
        assert_eq!(stable("f(a, //x\nb)"), "f(\n  a,  // x\n  b,\n)\n");
    }

    #[test]
    fn comments_inside_a_list_stay_inside_it() {
        assert_eq!(
            stable("f(a, // about a\n  b, // about b\n)"),
            "f(\n  a,  // about a\n  b,  // about b\n)\n",
            "trailing comments end their lines, the list breaks"
        );
        assert_eq!(
            stable("f(\n  // about a\n  a,\n  // about b\n  b)"),
            "f(\n  // about a\n  a,\n  // about b\n  b,\n)\n",
            "comments of their own keep their lines"
        );
        assert_eq!(
            stable("f(/* a */ a, b /* b */, /* c */ c)"),
            "f(/* a */ a, b /* b */, /* c */ c)\n",
            "inline block comments stay inline"
        );
        assert_eq!(
            stable("f(a /* last */)"),
            "f(a /* last */)\n",
            "an inline comment before the closing bracket ends the last item"
        );
        assert_eq!(
            stable("f(a, // last\n)"),
            "f(\n  a,  // last\n)\n",
            "a trailing comment before the closing bracket"
        );
        assert_eq!(
            stable("f(\n  a,\n  // dangling\n)"),
            "f(\n  a,\n  // dangling\n)\n",
            "a comment of its own before the closing bracket hangs"
        );
        assert_eq!(
            stable("f( // open\n  a)"),
            "f(  // open\n  a,\n)\n",
            "a trailing comment after the opening bracket ends its line"
        );
        assert_eq!(
            stable("f( // none\n)\nconst o = [\n  /* dangling */\n]\nconst e = [/* c */]"),
            "f(  // none\n)\n\nconst o = [\n  /* dangling */\n]\n\nconst e = [\n  /* c */\n]\n",
            "comments in an empty list hang"
        );
        assert_eq!(
            stable("const o = {\n  a: 1, // a\n  // b\n  b: 2,\n}"),
            "const o = {\n  a: 1,  // a\n  // b\n  b: 2,\n}\n"
        );
        assert_eq!(
            stable("const { a /* a */, b } = o\nconst [x, /* y */ y] = p\nfunction f(a /* a */, b) {}"),
            "const { a /* a */, b } = o\n\nconst [x, /* y */ y] = p\n\nfunction f(a /* a */, b) {}\n",
            "patterns and parameters"
        );
        assert_eq!(
            stable("const a = [1\n  // own before the comma\n  , 2]"),
            "const a = [\n  1,\n  // own before the comma\n  2,\n]\n",
            "a comment of its own before a comma moves past it"
        );
        assert_eq!(
            stable("new A(a, // a\n b)\nimport(/* c */ x)"),
            "new A(\n  a,  // a\n  b,\n)\n\nimport(/* c */ x)\n"
        );
    }

    #[test]
    fn comments_between_operators_and_in_chains() {
        assert_eq!(stable("x = a + /* c */ b"), "x = a + /* c */ b\n");
        assert_eq!(stable("x = a /* c */ + b"), "x = a /* c */ + b\n");
        assert_eq!(
            stable("x = a + // c\n  b"),
            "x = a +  // c\n  b\n",
            "a trailing comment after an operator"
        );
        assert_eq!(
            stable("x = a // c\n  + b"),
            "x = a +  // c\n  b\n",
            "a trailing comment before an operator goes to the end of its line"
        );
        assert_eq!(
            stable("x = a +\n  // c\n  b"),
            "x = a +\n  // c\n  b\n",
            "a comment of its own before an operand"
        );
        assert_eq!(
            stable("x = a\n  // c\n  ? b : c"),
            "x = a\n  // c\n  ? b\n  : c\n",
            "before `?`, the ternary breaks around it"
        );
        assert_eq!(
            stable("x = a ? /* b */ b : /* c */ c"),
            "x = a ? /* b */ b : /* c */ c\n"
        );
        assert_eq!(
            stable("x = a /* c */ .b()\nx = a.b() // c\n  .c()\n  .d()"),
            "x = a/* c */ .b()\n\nx = a\n  .b()  // c\n  .c()\n  .d()\n",
            "glued, an inline comment takes the token's place"
        );
        assert_eq!(
            stable("x = /* c */ -a\nx = typeof /* c */ a\nx = /* c */ `t`\nx = (a) /* c */ => a"),
            "x = /* c */ -a\n\nx = typeof /* c */ a\n\nx = /* c */ `t`\n\nx = (a) /* c */ => a\n"
        );
        assert_eq!(
            stable("x = /* c */ (a)\nx = (a /* c */)\nx = (a\n  // own\n) + 1\nx = ((a) /* in */)"),
            "x = /* c */ a\n\nx = a /* c */\n\nx = a +  // own\n  1\n\nx = a /* in */\n",
            "a dropped pair keeps its comments at the end of the line, a `//` one breaking the group"
        );
        assert_eq!(
            stable("x = a // c\n  = b"),
            "x = a = b  // c\n",
            "a trailing comment the layout cannot end a line with goes to the end"
        );
        assert_eq!(
            stable("x = a /* c */\n  + b"),
            "x = a + /* c */\n  b\n",
            "a block comment ending a line trails like a `//` one"
        );
        assert_eq!(
            stable("x = (a\n  /* c */ ) + 1"),
            "x = a + /* c */\n  1\n",
            "a block comment on its own line before a dropped `)`"
        );
        assert_eq!(
            stable("x = (a /* c */) + 1"),
            "x = a /* c */ + 1\n",
            "an inline one stays where it is"
        );
        assert_eq!(
            stable("x = /* c */ `a${b}`\nswitch (a) /* s */ {\n  case 1: {}\n}"),
            "x = /* c */ `a${b}`\n\nswitch (a) /* s */ {\n  case 1: {}\n}\n",
            "before a template with a substitution, and before a switch's brace"
        );
        assert_eq!(
            stable("x = /* c */ [1, 2]\nconst { a } = /* o */ { a: 1 }\nx = { /* m */ [Symbol.iterator]() {} }"),
            "x = /* c */ [1, 2]\n\nconst { a } = /* o */ { a: 1 }\n\nx = {\n  /* m */ [Symbol.iterator]() {},\n}\n",
            "comments before an opening bracket, and before a form copied as written"
        );
    }

    #[test]
    fn comments_around_statements_blocks_and_members() {
        assert_eq!(
            stable("if (a) /* c */ {\n  b()\n} // tail\nelse {\n  c()\n}"),
            "if (a) /* c */ {\n  b()\n} else {\n  // tail\n  c()\n}\n",
            "a comment that would end a brace line goes above the block's first item"
        );
        assert_eq!(
            stable("if (a) // c\n{\n  b()\n}\nif (a) {} // empty"),
            "if (a) {\n  // c\n  b()\n}\n\nif (a) {}  // empty\n"
        );
        assert_eq!(
            stable("f() /* c */;\ng(); // after\n"),
            "f() /* c */\n\ng()  // after\n",
            "the dropped `;` keeps its comments"
        );
        assert_eq!(
            stable("switch (a) {\n  case 1:\n  // between labels\n  case 2: {\n    f()\n  }\n}"),
            "switch (a) {\n  case 1:\n  // between labels\n  case 2: {\n    f()\n  }\n}\n"
        );
        assert_eq!(
            stable("class A {\n  m(/* c */ a) {} // trailing\n  static /* s */ f = 1\n}"),
            "class A {\n  m(/* c */ a) {}  // trailing\n  static /* s */ f = 1\n}\n"
        );
        assert_eq!(
            stable("const o = { a /* k */: 1, b: /* v */ 2 }\nconst { a /* p */: x } = o"),
            "const o = { a /* k */: 1, b: /* v */ 2 }\n\nconst { a /* p */: x } = o\n"
        );
        assert_eq!(
            stable(
                "for (let i = 0; /* c */ i < n; i++) {}\nimport { a /* a */, b } from \"./a.js\""
            ),
            "for (let i = 0; /* c */ i < n; i++) {}\n\nimport { a /* a */, b } from \"./a.js\"\n"
        );
        assert_eq!(
            stable("x = a // c\n, b"),
            "x = a, b  // c\n",
            "the removed comma operator keeps its comment"
        );
        assert_eq!(
            module("export default (a) // c\nb()"),
            "export default\n(a)  // c\n\nb()\n",
            "a comment after what the recovery left bare"
        );
        assert_eq!(
            stable("try {\n  a()\n} // x\nfinally {} // y\nb()"),
            "try {\n  a()\n} finally {\n  // x\n}  // y\n\nb()\n",
            "a comment before an empty block hangs inside it"
        );
        assert_eq!(
            stable("x = (a // p\n) // y\nz()"),
            "x = a  // p\n\n// y\nz()\n",
            "two comments for one line's end: the second begins the next item's"
        );
        assert_eq!(
            stable("f(a // p\n, // x\n b)"),
            "f(\n  a,  // p\n  // x\n  b,\n)\n"
        );
    }

    #[test]
    fn doc_comments_are_reflowed_to_the_width() {
        let long = "word ".repeat(30);
        assert_eq!(
            stable("/** Short. */\nexport function f() {}"),
            "/** Short. */\nexport function f() {}\n",
            "one line that fits stays one line"
        );
        assert_eq!(
            stable("/**Short.*/\nexport function f() {}"),
            "/** Short. */\nexport function f() {}\n"
        );
        let out = stable(&format!("/** {long}*/\nexport function f() {{}}"));
        assert!(
            out.starts_with("/**\n * word word"),
            "one line too long takes the block form:\n{out}"
        );
        assert!(out.lines().all(|line| line.chars().count() <= 100), "{out}");
        assert_eq!(
            stable(&format!(
                "/**\n * {long}\n *\n * Second paragraph.\n */\nexport function f() {{}}"
            )),
            format!(
                "/**\n * {}\n * {}\n *\n * Second paragraph.\n */\nexport function f() {{}}\n",
                "word ".repeat(19).trim_end(),
                "word ".repeat(11).trim_end()
            ),
            "paragraphs are filled and kept apart"
        );
        assert_eq!(
            stable("/**\n * One\n * two.\n *\n *\n * @param a  the first, which is described at length so that the tag line runs past the width of the layout\n * @returns it\n */\nexport function f(a) {}"),
            "/**\n * One two.\n *\n * @param a  the first, which is described at length so that the tag line runs past the width of the layout\n * @returns it\n */\nexport function f(a) {}\n",
            "lines join, blank lines collapse, tag lines stay whole"
        );
        assert_eq!(
            stable("/**\n * @example\n * ```js\n * const x = f()\n *\n *   expect(x).toBe(1)\n * ```\n * - item one\n * - item two\n * # Heading\n * | a | b |\n * > quote\n * 1. first\n * 2) second\n *   indented\n */\nexport function f() {}"),
            "/**\n * @example\n * ```js\n * const x = f()\n *\n *   expect(x).toBe(1)\n * ```\n * - item one\n * - item two\n * # Heading\n * | a | b |\n * > quote\n * 1. first\n * 2) second\n *   indented\n */\nexport function f() {}\n",
            "fences, lists, headings, tables, quotes and indented lines are verbatim"
        );
        assert_eq!(
            stable("/**\n * Use a - b, not #1 or 2. items, and @never a tag.\n */\nexport function f() {}"),
            "/**\n * Use a - b, not #1 or 2. items, and @never a tag.\n */\nexport function f() {}\n"
        );
        let markers = format!(
            "/**\n * {}x - b #1 2. @tag\n */\nexport function f() {{}}",
            "word ".repeat(19)
        );
        assert_eq!(
            stable(&markers),
            format!(
                "/**\n * {}\n * x - b #1 2. @tag\n */\nexport function f() {{}}\n",
                "word ".repeat(19).trim_end()
            ),
            "a word that would read as structure at a line's start stays with the word before"
        );
        assert_eq!(
            stable("/**\n * ```js\n * const x = 1\n * const y = 2\n * ```\n * prose\n * joins\n */\nexport function f() {}"),
            "/**\n * ```js\n * const x = 1\n * const y = 2\n * ```\n * prose joins\n */\nexport function f() {}\n",
            "lines inside a fence stay apart, lines after it join"
        );
        assert_eq!(
            stable("/**\n *\n * Doc.\n *\n */\nexport function f() {}"),
            "/**\n * Doc.\n */\nexport function f() {}\n",
            "blank lines at either end are dropped"
        );
        assert_eq!(
            stable("/**\n Doc without stars.\n */\nexport function f() {}\n/** Summary on the first line.\n * More.\n * **Bold** stays. */\nexport function g() {}"),
            "/**\n * Doc without stars.\n */\nexport function f() {}\n\n/**\n * Summary on the first line. More. **Bold** stays.\n */\nexport function g() {}\n"
        );
        let words = "word ".repeat(19);
        assert_eq!(
            stable(&format!("/**\n * {words}ab\n */\nexport function f() {{}}\nclass A {{\n  /**\n   * {words}ab\n   */\n  m() {{}}\n}}")),
            format!("/**\n * {words}ab\n */\nexport function f() {{}}\n\nclass A {{\n  /**\n   * {}\n   * ab\n   */\n  m() {{}}\n}}\n", words.trim_end()),
            "the width counts the indentation: at the top it fits, in the class it wraps"
        );
        assert_eq!(
            stable("/***/\nf()\n/**/\ng()\n/** */\nh()"),
            "/** */\nf()\n\n/**/\ng()\n\n/** */\nh()\n",
            "the smallest doc comments"
        );
        assert_eq!(
            stable("f(/** inline */ a)\nf() /** trailing */"),
            "f(/** inline */ a)\n\nf() /** trailing */\n",
            "a doc comment not on a line of its own is left as written"
        );
    }

    #[test]
    fn s006_names_a_comment_on_a_line_the_layout_leaves_too_long() {
        let wide = "x".repeat(99);
        assert_eq!(long(&format!("// {wide}\nf()")), vec![(1, 1, false)]);
        assert_eq!(
            long(&format!("f() // {wide}")),
            vec![(1, 5, false)],
            "a trailing comment, counted with its line"
        );
        assert_eq!(
            long(&format!(
                "/**\n * Prose.\n * @param a  {wide}\n */\nexport function f(a) {{}}"
            )),
            vec![(1, 1, false)],
            "a tag line stays whole"
        );
        assert_eq!(
            long(&format!("f(a, // {wide}\n  b)")),
            vec![(1, 6, false)],
            "inside a list"
        );
        assert_eq!(
            long(&format!("// {}\nf()", "x".repeat(97))),
            Vec::new(),
            "exactly the width is not too long"
        );
        assert_eq!(
            long(&format!(
                "/** {}*/\nexport function f() {{}}",
                "word ".repeat(30)
            )),
            Vec::new(),
            "prose the layout can fill"
        );
        assert_eq!(
            long(&format!("const s = \"{wide}{wide}\" // c")),
            vec![(1, 212, false)],
            "a comment on a line a long token makes long"
        );
        assert_eq!(
            long(&format!("const s = \"{wide}{wide}\"")),
            Vec::new(),
            "no comment, nothing said"
        );
        assert_eq!(
            long(&format!(
                "import {{ b }} from \"./b.js\" // {wide}\nimport {{ a }} from \"./a.js\""
            )),
            vec![(1, 28, false)],
            "imports in their order"
        );
        assert_eq!(
            long(&format!("export default /* lost */ (a) // {wide}\nb()")),
            vec![(1, 31, false)],
            "a comment the recovery drops is left out of the pairing"
        );
        assert_eq!(
            long(&format!("export default ( /* c */ a) // {wide}\nb()")),
            vec![(1, 18, false), (1, 29, false)],
            "a comment among the tokens left bare is copied with them and paired"
        );
    }
}
