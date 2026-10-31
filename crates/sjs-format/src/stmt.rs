//! Statements, declarations and blank lines (US-042, SPEC.md §9.1, §9.4): the layout of
//! a file as a whole. Every item sits on its own line and a block's items are indented
//! one level. One blank line separates two top-level items, none two imports; inside a
//! block the author's blank lines stay, at most one in a row and none after `{` or
//! before `}`, and one always precedes a `function` declaration, above its comment. A
//! comment above an item stays above it and one at the end of its line stays there,
//! as written (US-046 refines the text); a template literal keeps its lines (§9.3).
//! Imports stand in order ([`crate::imports`], US-045), class members too
//! ([`crate::members`], US-047), and a comment is written as [`crate::comments`] says
//! (US-046).

use sjs_diag::Span;
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, Token, TokenKind, Trivia, TriviaKind};

use crate::doc::{layout, Doc, WIDTH};
use crate::expr::Formatter;
use crate::imports::sort_imports;

/// Where a `*` glues: to the word before it (`function* g`) or to the name after it
/// (`async *gen()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Star {
    Before,
    After,
}

/// One line above an item: a blank line, or a comment on a line of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Above {
    Blank,
    Comment(Span),
}

impl Above {
    /// The comment's span; `None` for a blank line.
    pub(crate) fn comment(self) -> Option<Span> {
        match self {
            Above::Comment(span) => Some(span),
            Above::Blank => None,
        }
    }
}

/// What stands on a line of its own among a block's or the file's items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Body<'t> {
    /// An item.
    Node(NodeRef<'t>),
    /// A run of tokens the parser's recovery left bare (E000), copied as written: its
    /// byte range.
    Stray(usize, usize),
    /// The closing brace or the end of the file: nothing but the comments before it.
    End,
}

/// One item with the lines above it and the comments at the end of its line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item<'t> {
    pub(crate) body: Body<'t>,
    pub(crate) above: Vec<Above>,
    pub(crate) trailing: Vec<Span>,
}

impl<'t> Item<'t> {
    fn new(body: Body<'t>, above: Vec<Above>) -> Self {
        Item {
            body,
            above,
            trailing: Vec::new(),
        }
    }
}

impl Formatter<'_> {
    /// The canonical text of the whole file, ending in exactly one `\n`; empty for a
    /// file with nothing in it.
    #[must_use]
    pub fn module(&self) -> String {
        self.module_at(WIDTH)
    }

    /// [`Self::module`] at `width` columns instead of the canonical width, for tests
    /// that show a layout flipping.
    #[must_use]
    pub fn module_at(&self, width: usize) -> String {
        let text = joined(self.items(self.tree.root(), true), width);
        if text.is_empty() {
            text
        } else {
            text + "\n"
        }
    }

    /// The document of a statement, a declaration, a class member, a clause or a module
    /// item; a removed form is copied as written. The `;` ending a statement is dropped
    /// (S002), and an empty statement is nothing.
    #[must_use]
    pub fn statement(&self, node: NodeRef<'_>) -> Doc {
        match node.kind() {
            NodeKind::EmptyStmt => Doc::text(""),
            NodeKind::Block | NodeKind::ClassBody => self.block(node),
            NodeKind::SwitchStmt => self.switch(node),
            NodeKind::CaseClause => self.case_clause(node),
            NodeKind::ClassMethod | NodeKind::ClassAccessor => self.words(node, Star::After),
            NodeKind::ExprStmt
            | NodeKind::ConstDecl
            | NodeKind::LetDecl
            | NodeKind::FunctionDecl
            | NodeKind::ClassDecl
            | NodeKind::ClassHeritage
            | NodeKind::ClassField
            | NodeKind::StaticBlock
            | NodeKind::IfStmt
            | NodeKind::WhileStmt
            | NodeKind::ForOfStmt
            | NodeKind::ForStmt
            | NodeKind::ReturnStmt
            | NodeKind::ThrowStmt
            | NodeKind::BreakStmt
            | NodeKind::ContinueStmt
            | NodeKind::TryStmt
            | NodeKind::CatchClause
            | NodeKind::FinallyClause
            | NodeKind::CaseLabel
            | NodeKind::ImportDecl
            | NodeKind::ExportDecl
            | NodeKind::ImportSpecifier
            | NodeKind::ExportSpecifier
            | NodeKind::NamespaceImport
            | NodeKind::DefaultImport
            | NodeKind::ImportAttributes => self.words(node, Star::Before),
            NodeKind::NamedImports | NodeKind::NamedExports => self.list(node, true),
            NodeKind::SymbolKey => self.glued(node),
            // `i++` glues; `i += 1` is spaced.
            NodeKind::ForUpdate if node.children().count() == 2 => self.glued(node),
            NodeKind::ForUpdate => self.spaced(node),
            _ => self.verbatim(node),
        }
    }

    /// `{ … }` with one item per line, indented; `{}` with nothing inside; the comments
    /// before the `{` before it, and a comment after it on a line of its own inside.
    fn block(&self, node: NodeRef<'_>) -> Doc {
        let brace = node.children().find_map(|child| match child {
            Element::Token(token) if is_punct(child, Punct::LBrace) => Some(token),
            _ => None,
        });
        let mut out = brace.map_or_else(Vec::new, |token| self.before(token));
        let mut inner = self.items(node, false);
        if inner.is_empty() {
            out.extend([
                Doc::text("{"),
                Doc::indent(vec![Doc::HangSuffixes]),
                Doc::HangEnd,
                Doc::text("}"),
            ]);
        } else {
            inner.insert(0, Doc::HangSuffixes);
            out.extend([
                Doc::text("{"),
                Doc::indent(inner),
                Doc::HardLine,
                Doc::text("}"),
            ]);
        }
        Doc::Group(out)
    }

    /// `switch (x) {` with one clause per line.
    fn switch(&self, node: NodeRef<'_>) -> Doc {
        let children: Vec<Element<'_>> = node.children().collect();
        let brace = children
            .iter()
            .position(|child| is_punct(*child, Punct::LBrace))
            .unwrap_or(children.len());
        Doc::Group(vec![
            self.words_of(&children[..brace], Star::Before),
            Doc::text(" "),
            self.block(node),
        ])
    }

    /// Stacked labels one per line, the block after the last.
    fn case_clause(&self, node: NodeRef<'_>) -> Doc {
        let mut out = Vec::new();
        for child in node.children() {
            match child {
                Element::Node(label) if label.kind() == NodeKind::CaseLabel => {
                    if !out.is_empty() {
                        out.push(Doc::HardLine);
                    }
                    out.push(self.statement(label));
                }
                other => {
                    out.push(Doc::text(" "));
                    out.push(self.element(other));
                }
            }
        }
        Doc::Group(out)
    }

    /// The children as words with one space between them, except that `(` and `[` glue
    /// to what follows, `)`, `]`, `,`, `:` and `;` to what precedes, `.` to both, a
    /// parameter list to its name, and `*` as `star` says; a `;` ending the node is
    /// dropped.
    fn words(&self, node: NodeRef<'_>, star: Star) -> Doc {
        let children: Vec<Element<'_>> = node.children().collect();
        self.words_of(&children, star)
    }

    fn words_of(&self, elements: &[Element<'_>], star: Star) -> Doc {
        let mut out = Vec::new();
        let mut glue = true;
        let last = elements.len().saturating_sub(1);
        for (at, element) in elements.iter().enumerate() {
            let (before, after) = match *element {
                Element::Token(token) => match token.token().kind {
                    TokenKind::Punct(Punct::Semicolon) if at == last => {
                        out.extend(self.suffixes(token));
                        break;
                    }
                    TokenKind::Punct(
                        Punct::Semicolon
                        | Punct::RParen
                        | Punct::RBracket
                        | Punct::Comma
                        | Punct::Colon,
                    ) => (true, false),
                    // A sign is a `case` label's number's.
                    TokenKind::Punct(
                        Punct::LParen | Punct::LBracket | Punct::Minus | Punct::Plus,
                    ) => (false, true),
                    TokenKind::Punct(Punct::Dot) => (true, true),
                    TokenKind::Punct(Punct::Star) => (star == Star::Before, star == Star::After),
                    _ => (false, false),
                },
                Element::Node(node) => (node.kind() == NodeKind::Params, false),
            };
            if !(glue || before) {
                out.push(Doc::text(" "));
            }
            out.push(self.element(*element));
            glue = after;
        }
        Doc::Group(out)
    }

    /// The lines of a block's or the file's items, each beginning with a line break:
    /// blank lines placed, comments kept above or beside their item, imports in order
    /// (US-045), and the comments before the closing brace (or the end of the file)
    /// last.
    fn items(&self, parent: NodeRef<'_>, top: bool) -> Vec<Doc> {
        let mut items = items_of(parent, top);
        sort_imports(&mut items);
        if parent.kind() == NodeKind::ClassBody {
            self.sort_members(&mut items);
        }
        self.lines(&items, top)
    }

    /// `laid_out`, its lines after the first indented like the source line `node` starts
    /// on, as a replacement for the text of `node` where it stands; a blank line stays
    /// blank.
    pub(crate) fn indented_like(&self, node: NodeRef<'_>, laid_out: &str) -> String {
        let line = self
            .file
            .line(node.span().start.line)
            .map_or("", |line| line.text);
        let indent = &line[..line.len() - line.trim_start().len()];
        laid_out
            .lines()
            .enumerate()
            .map(|(at, text)| {
                if at == 0 || text.is_empty() {
                    text.to_owned()
                } else {
                    format!("{indent}{text}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `items` as lines, each beginning with a line break; a class field takes the `;`
    /// ES needs when the member after it begins with `[` or `*` (US-047).
    pub(crate) fn lines(&self, items: &[Item<'_>], top: bool) -> Vec<Doc> {
        let mut out = Vec::new();
        let mut previous: Option<NodeKind> = None;
        for (at, item) in items.iter().enumerate() {
            match item.body {
                Body::Stray(from, to) => {
                    out.push(Doc::HardLine);
                    out.push(Doc::text(&self.file.text()[from..to]));
                }
                Body::Node(node) => {
                    let source = matches!(item.above.first(), Some(Above::Blank));
                    out.push(Doc::HardLine);
                    if blank_line_before(top, previous, node.kind(), source) {
                        out.push(Doc::HardLine);
                    }
                    self.push_above(&mut out, &item.above);
                    out.push(self.statement(node));
                    if needs_semicolon(node, items.get(at + 1)) {
                        out.push(Doc::text(";"));
                    }
                    previous = Some(node.kind());
                }
                Body::End => self.push_dangling(&mut out, &item.above, previous.is_some()),
            }
            for span in &item.trailing {
                out.push(Doc::LineSuffix(self.at_line_end(*span)));
            }
        }
        out
    }

    /// The lines above an item: a blank line as one more break, a comment on its own
    /// line. A blank line first is the item's separation, already placed.
    fn push_above(&self, out: &mut Vec<Doc>, above: &[Above]) {
        let first_blank = usize::from(matches!(above.first(), Some(Above::Blank)));
        for entry in &above[first_blank..] {
            match *entry {
                Above::Blank => out.push(Doc::HardLine),
                Above::Comment(span) => {
                    out.extend(self.comment_block(span));
                    out.push(Doc::HardLine);
                }
            }
        }
    }

    /// The comments before a closing brace or the end of the file, one per line: a
    /// blank line between them or before them stays (when something precedes them),
    /// one after them does not.
    fn push_dangling(&self, out: &mut Vec<Doc>, above: &[Above], has_items: bool) {
        let mut blank = false;
        let mut separated = has_items;
        for entry in above {
            match *entry {
                Above::Blank => blank = true,
                Above::Comment(span) => {
                    if blank && separated {
                        out.push(Doc::HardLine);
                    }
                    out.push(Doc::HardLine);
                    out.extend(self.comment_block(span));
                    blank = false;
                    separated = true;
                }
            }
        }
    }
}

/// Whether a `;` must end `node`: a class field followed by a member that begins with
/// `[` or `*`, which would otherwise continue the field (ES2025 ASI).
fn needs_semicolon(node: NodeRef<'_>, next: Option<&Item<'_>>) -> bool {
    node.kind() == NodeKind::ClassField
        && matches!(next, Some(Item { body: Body::Node(member), .. })
        if member.tokens().first().is_some_and(|token| {
            matches!(token.kind, TokenKind::Punct(Punct::LBracket | Punct::Star))
        }))
}

/// `items` with the item at `order[to]` moved to position `to`, for every `to`: it brings
/// its body, the comments at the end of its line and the lines above it after the first
/// `kept(above, from)` of them, while the kept lines of each position stay there.
pub(crate) fn reorder(
    items: &mut [Item<'_>],
    order: &[usize],
    kept: impl Fn(&[Above], usize) -> usize,
) {
    let moved: Vec<Item<'_>> = order
        .iter()
        .map(|&from| Item {
            body: items[from].body,
            above: items[from].above[kept(&items[from].above, from)..].to_vec(),
            trailing: items[from].trailing.clone(),
        })
        .collect();
    for (at, (item, moved)) in items.iter_mut().zip(moved).enumerate() {
        item.above.truncate(kept(&item.above, at));
        item.above.extend(moved.above);
        item.body = moved.body;
        item.trailing = moved.trailing;
    }
}

/// How many of the lines above an item are not its own: those up to and including the
/// last blank line, what follows being the item's comments.
pub(crate) fn detached_len(above: &[Above]) -> usize {
    above
        .iter()
        .rposition(|line| *line == Above::Blank)
        .map_or(0, |blank| blank + 1)
}

/// `docs`, each line beginning with a break, laid out at `width` with the first break
/// dropped.
pub(crate) fn joined(mut docs: Vec<Doc>, width: usize) -> String {
    if matches!(docs.first(), Some(Doc::HardLine)) {
        docs.remove(0);
    }
    layout(&docs, width)
}

/// The items of a block (those after its `{`) or of the file (all of its children), in
/// source order, each with the lines above it and the comments at the end of its line.
/// An empty statement is dropped, its lines and the comment on its line moving to what
/// follows it; tokens left bare by the parser's recovery (E000) are an item per run.
pub(crate) fn items_of(parent: NodeRef<'_>, top: bool) -> Vec<Item<'_>> {
    let children: Vec<Element<'_>> = parent.children().collect();
    let start = if top {
        0
    } else {
        children
            .iter()
            .position(|child| is_punct(*child, Punct::LBrace))
            .map_or(children.len(), |brace| brace + 1)
    };
    let mut items: Vec<Item<'_>> = Vec::new();
    let mut has_node = false;
    let mut carried: Vec<Above> = Vec::new();
    let mut after_empty = false;
    // The source range of the bare tokens seen since the last item.
    let mut stray: Option<(usize, usize)> = None;
    let last = children.len().saturating_sub(1);
    for (at, element) in children.iter().enumerate().skip(start) {
        if let Element::Token(token) = *element {
            if at < last {
                let span = token.token().span;
                stray = Some((
                    stray.map_or(span.byte_start, |(from, _)| from),
                    span.byte_end,
                ));
                continue;
            }
        }
        if let Some((from, to)) = stray.take() {
            items.push(Item::new(Body::Stray(from, to), Vec::new()));
        }
        let leading = first_token(*element).map_or(&[][..], |token| &token.leading[..]);
        let (trailing, mut above) = split_leading(leading, has_node && !after_empty);
        if let Some(previous) = items.last_mut() {
            previous.trailing.extend(trailing);
        }
        above.splice(0..0, carried.drain(..));
        after_empty = false;
        match *element {
            Element::Node(node) if node.kind() == NodeKind::EmptyStmt => {
                carried = above;
                after_empty = true;
            }
            Element::Node(node) => {
                items.push(Item::new(Body::Node(node), above));
                has_node = true;
            }
            Element::Token(_) => items.push(Item::new(Body::End, above)),
        }
    }
    items
}

/// Whether a blank line separates an item from the one before it (SPEC.md §9.1, §9.4):
/// at the top level always, except between two imports; in a block before a `function`
/// declaration, else where the source had one. Nothing precedes the first item.
fn blank_line_before(top: bool, previous: Option<NodeKind>, next: NodeKind, source: bool) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    if top {
        return !(previous == NodeKind::ImportDecl && next == NodeKind::ImportDecl);
    }
    next == NodeKind::FunctionDecl || source
}

/// The leading trivia of a token split into the comments on the line before it, which
/// belong to the item before (when `has_previous`), and the lines above it.
fn split_leading(leading: &[Trivia], has_previous: bool) -> (Vec<Span>, Vec<Above>) {
    let mut trailing = Vec::new();
    let mut above = Vec::new();
    let mut on_previous_line = has_previous;
    let mut line_has_content = has_previous;
    let mut blank = false;
    for trivia in leading {
        match trivia.kind {
            TriviaKind::Whitespace => {}
            TriviaKind::LineTerminator(_) => {
                if line_has_content {
                    line_has_content = false;
                } else {
                    blank = true;
                }
                on_previous_line = false;
            }
            TriviaKind::LineComment
            | TriviaKind::BlockComment { .. }
            | TriviaKind::DocComment { .. } => {
                if on_previous_line {
                    trailing.push(trivia.span);
                    continue;
                }
                if blank {
                    above.push(Above::Blank);
                    blank = false;
                }
                above.push(Above::Comment(trivia.span));
                line_has_content = true;
            }
        }
    }
    if blank {
        above.push(Above::Blank);
    }
    (trailing, above)
}

/// The first token of an element: a node's first, or the token itself.
fn first_token(element: Element<'_>) -> Option<&Token> {
    match element {
        Element::Node(node) => node.tokens().first(),
        Element::Token(token) => Some(token.token()),
    }
}

fn is_punct(element: Element<'_>, punct: Punct) -> bool {
    matches!(element, Element::Token(token) if token.token().kind == TokenKind::Punct(punct))
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::SourceFile;
    use sjs_syntax::parse;

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    #[test]
    fn declarations_and_expression_statements() {
        assert_eq!(module("const a=1\n"), "const a = 1\n");
        assert_eq!(module("let m\nlet  n = 2"), "let m\n\nlet n = 2\n");
        assert_eq!(module("const {x,y:[z]}=o"), "const { x, y: [z] } = o\n");
        assert_eq!(module("f( a );\n"), "f(a)\n");
        assert_eq!(
            module("x = 1 ;\n;\ny = 2"),
            "x = 1\n\ny = 2\n",
            "an empty statement is nothing"
        );
        assert_eq!(module(""), "", "nothing in, nothing out");
        assert_eq!(module("\n\n"), "", "blank lines alone are nothing");
    }

    #[test]
    fn functions_and_classes() {
        assert_eq!(
            module("async  function * g ( a , b = 1 , ...r ) { yield a }"),
            "async function* g(a, b = 1, ...r) {\n  yield a\n}\n"
        );
        assert_eq!(module("function f(){}"), "function f() {}\n");
        assert_eq!(
            module("class C extends a.b { static s = 1\n #p = 2\n constructor(){ super() }\n get g(){ return 1 }\n set g(v){}\n static async * gen(){}\n #q(){}\n static { f() }\n [ Symbol . iterator ](){} }"),
            "class C extends a.b {\n  static s = 1\n  static async *gen() {}\n  static {\n    f()\n  }\n  #p = 2\n  constructor() {\n    super()\n  }\n  get g() {\n    return 1\n  }\n  set g(v) {}\n  [Symbol.iterator]() {}\n  #q() {}\n}\n"
        );
        assert_eq!(module("class A {}"), "class A {}\n");
    }

    #[test]
    fn modules() {
        assert_eq!(
            module("import {a,b as c} from './a.js'\nimport * as ns from \"./n.js\"\nimport d from './d.json' with {type:'json'}\nexport const k = 1\nexport {k as j} from './k.js'\nexport function f(){}"),
            "import { a, b as c } from \"./a.js\"\nimport d from \"./d.json\" with { type: \"json\" }\nimport * as ns from \"./n.js\"\n\nexport const k = 1\n\nexport { k as j } from \"./k.js\"\n\nexport function f() {}\n"
        );
    }

    #[test]
    fn control_flow() {
        assert_eq!(
            module("if(a){b()}else if(c){d()}else{e()}"),
            "if (a) {\n  b()\n} else if (c) {\n  d()\n} else {\n  e()\n}\n"
        );
        assert_eq!(module("while(a){break}"), "while (a) {\n  break\n}\n");
        assert_eq!(
            module("for(const x of xs){continue outer}\nfor await(const x of xs){}\nfor(let i=0;i<n;i++){}\nfor(let i=0;i<n;i+=2){}"),
            "for (const x of xs) {\n  continue outer\n}\n\nfor await (const x of xs) {}\n\nfor (let i = 0; i < n; i++) {}\n\nfor (let i = 0; i < n; i += 2) {}\n"
        );
        assert_eq!(
            module("switch(a){case 1:{f()}\ncase 2:\ncase 3:{g()}\ndefault:{\n}}"),
            "switch (a) {\n  case 1: {\n    f()\n  }\n  case 2:\n  case 3: {\n    g()\n  }\n  default: {}\n}\n"
        );
        assert_eq!(module("switch(a){}"), "switch (a) {}\n");
        assert_eq!(
            module("switch (a) {\n  case -1: {}\n  case +2: {}\n}"),
            "switch (a) {\n  case -1: {}\n  case +2: {}\n}\n"
        );
        assert_eq!(
            module("try{f()}catch(e){g()}finally{h()}\ntry{f()}catch{}"),
            "try {\n  f()\n} catch (e) {\n  g()\n} finally {\n  h()\n}\n\ntry {\n  f()\n} catch {}\n"
        );
        assert_eq!(
            module("throw new Error('x')\n{\n  f()\n}\nreturn\nreturn a"),
            "throw new Error(\"x\")\n\n{\n  f()\n}\n\nreturn\n\nreturn a\n"
        );
    }

    #[test]
    fn removed_forms_are_copied_as_written() {
        assert_eq!(
            module("outer: while (a) {}\ndo {} while (a)\nwith (o) {}\nvar v = 1\ndebugger"),
            "outer: while (a) {}\n\ndo {} while (a)\n\nwith (o) {}\n\nvar v = 1\n\ndebugger\n"
        );
        assert_eq!(
            module("switch (a)"),
            "switch (a) {}\n",
            "a body the grammar wanted"
        );
        assert_eq!(
            module("switch (a) {\n  case 1:\n    f()\n}"),
            "switch (a) {\n  case 1: f()\n}\n",
            "a braceless clause keeps its statement"
        );
        assert_eq!(
            module("export default (a)\nb()"),
            "export default\n(a)\n\nb()\n",
            "tokens the recovery left bare are kept, a run to a line"
        );
        assert_eq!(module("let [a, , b] = c"), "let [a\n, b] = c\n");
        assert_eq!(
            module("if (a) ;"),
            "if (a)\n",
            "an empty body (E000) is nothing"
        );
    }

    #[test]
    fn blank_lines_at_the_top_level_and_in_blocks() {
        assert_eq!(module("a()\nb()\n\n\n\nc()"), "a()\n\nb()\n\nc()\n");
        assert_eq!(
            module("function f() {\n\n\n  a()\n  b()\n\n\n  c()\n\n\n}"),
            "function f() {\n  a()\n  b()\n\n  c()\n}\n"
        );
        assert_eq!(
            module("function f() {\n  a()\n  function g() {}\n  b()\n}"),
            "function f() {\n  a()\n\n  function g() {}\n  b()\n}\n",
            "a function gets its blank line, what follows it keeps the source's"
        );
        assert_eq!(
            module("function f() {\n  a()\n  // g\n  function g() {}\n}"),
            "function f() {\n  a()\n\n  // g\n  function g() {}\n}\n",
            "above its comment"
        );
        assert_eq!(
            module("class A {\n\n  a = 1\n\n\n  b = 2\n  c = 3\n\n}"),
            "class A {\n  a = 1\n\n  b = 2\n  c = 3\n}\n"
        );
    }

    #[test]
    fn comments_stay_above_or_beside_their_item() {
        assert_eq!(
            module("// lead\nconst a = 1 // trail\n\n\n/** doc */\nfunction f() {} /* after */\n"),
            "// lead\nconst a = 1  // trail\n\n/** doc */\nfunction f() {} /* after */\n"
        );
        assert_eq!(
            module("const a = 1\n// between\n\nconst b = 2"),
            "const a = 1\n\n// between\n\nconst b = 2\n",
            "a blank line after a comment stays"
        );
        assert_eq!(
            module("// only\n\n// comments\n"),
            "// only\n\n// comments\n"
        );
        assert_eq!(
            module("f() // x"),
            "f()  // x\n",
            "no final newline, a trailing comment"
        );
        assert_eq!(
            module("function f() { // why\n  g()\n\n  // end\n\n}"),
            "function f() {\n  // why\n  g()\n\n  // end\n}\n"
        );
        assert_eq!(
            module("function f() {\n\n  // alone\n\n}"),
            "function f() {\n  // alone\n}\n"
        );
        assert_eq!(module("a()\n\n// end"), "a()\n\n// end\n");
        assert_eq!(
            module("a();\n;  // on the empty one\nb()"),
            "a()\n\n// on the empty one\nb()\n",
            "a comment beside a dropped `;` goes above what follows"
        );
        assert_eq!(
            module("/* a */ /* b */\nconst x = `l1\n\n\n  l2`"),
            "/* a */\n/* b */\nconst x = `l1\n\n\n  l2`\n",
            "two comments on one line take a line each; a template keeps its lines"
        );
    }
}
