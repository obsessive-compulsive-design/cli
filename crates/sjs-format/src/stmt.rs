//! Statements, declarations and blank lines (US-042, SPEC.md §9.1, §9.4): the layout of
//! a file as a whole. Every item sits on its own line and a block's items are indented
//! one level. One blank line separates two top-level items, none two imports; inside a
//! block the author's blank lines stay, at most one in a row and none after `{` or
//! before `}`, and one always precedes a `function` declaration, above its comment. A
//! comment above an item stays above it and one at the end of its line stays there,
//! as written (US-046 refines the text); a template literal keeps its lines (§9.3).

use sjs_diag::Span;
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, Token, TokenKind, Trivia, TriviaKind};

use crate::doc::{print, Doc};
use crate::expr::Formatter;

/// Where a `*` glues: to the word before it (`function* g`) or to the name after it
/// (`async *gen()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Star {
    Before,
    After,
}

/// One line above an item: a blank line, or a comment on a line of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Above {
    Blank,
    Comment(Span),
}

impl Formatter<'_> {
    /// The canonical text of the whole file, ending in exactly one `\n`; empty for a
    /// file with nothing in it.
    #[must_use]
    pub fn module(&self) -> String {
        let mut docs = self.items(self.tree.root(), true);
        if matches!(docs.first(), Some(Doc::HardLine)) {
            docs.remove(0);
        }
        let text = print(&docs);
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

    /// `{ … }` with one item per line, indented; `{}` with nothing inside.
    fn block(&self, node: NodeRef<'_>) -> Doc {
        let inner = self.items(node, false);
        if inner.is_empty() {
            return Doc::text("{}");
        }
        Doc::Group(vec![
            Doc::text("{"),
            Doc::indent(inner),
            Doc::HardLine,
            Doc::text("}"),
        ])
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
                    TokenKind::Punct(Punct::Semicolon) if at == last => break,
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
    /// blank lines placed, comments kept above or beside their item, and the comments
    /// before the closing brace (or the end of the file) last. A block's items follow
    /// its `{`; the file's are all of its children. Tokens left bare by the parser's
    /// recovery (E000) are copied as written, a run of them to a line.
    fn items(&self, parent: NodeRef<'_>, top: bool) -> Vec<Doc> {
        let children: Vec<Element<'_>> = parent.children().collect();
        let start = if top {
            0
        } else {
            children
                .iter()
                .position(|child| is_punct(*child, Punct::LBrace))
                .map_or(children.len(), |brace| brace + 1)
        };
        let mut out = Vec::new();
        let mut previous: Option<NodeKind> = None;
        // The lines above an empty statement (dropped) move to what follows it, and so
        // does a comment on its line.
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
                out.push(Doc::HardLine);
                out.push(Doc::text(&self.file.text()[from..to]));
            }
            let leading = first_token(*element).map_or(&[][..], |token| &token.leading[..]);
            let (trailing, mut above) = split_leading(leading, previous.is_some() && !after_empty);
            for span in trailing {
                out.push(Doc::text("  "));
                out.push(Doc::text(self.text(span)));
            }
            above.splice(0..0, carried.drain(..));
            after_empty = false;
            match *element {
                Element::Node(node) if node.kind() == NodeKind::EmptyStmt => {
                    carried = above;
                    after_empty = true;
                }
                Element::Node(node) => {
                    let source = matches!(above.first(), Some(Above::Blank));
                    out.push(Doc::HardLine);
                    if blank_line_before(top, previous, node.kind(), source) {
                        out.push(Doc::HardLine);
                    }
                    self.push_above(&mut out, &above);
                    out.push(self.statement(node));
                    previous = Some(node.kind());
                }
                Element::Token(_) => self.push_dangling(&mut out, &above, previous.is_some()),
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
                    out.push(Doc::text(self.text(span)));
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
                    out.push(Doc::text(self.text(span)));
                    blank = false;
                    separated = true;
                }
            }
        }
    }
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
            "class C extends a.b {\n  static s = 1\n  #p = 2\n  constructor() {\n    super()\n  }\n  get g() {\n    return 1\n  }\n  set g(v) {}\n  static async *gen() {}\n  #q() {}\n  static {\n    f()\n  }\n  [Symbol.iterator]() {}\n}\n"
        );
        assert_eq!(module("class A {}"), "class A {}\n");
    }

    #[test]
    fn modules() {
        assert_eq!(
            module("import {a,b as c} from './a.js'\nimport * as ns from \"./n.js\"\nimport d from './d.json' with {type:'json'}\nexport const k = 1\nexport {k as j} from './k.js'\nexport function f(){}"),
            "import { a, b as c } from \"./a.js\"\nimport * as ns from \"./n.js\"\nimport d from \"./d.json\" with { type: \"json\" }\n\nexport const k = 1\n\nexport { k as j } from \"./k.js\"\n\nexport function f() {}\n"
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
            "// lead\nconst a = 1  // trail\n\n/** doc */\nfunction f() {}  /* after */\n"
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
