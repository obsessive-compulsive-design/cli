//! The lossless syntax tree (SPEC.md §11, §11.1): an arena of nodes over the token
//! sequence, with parent links, built once by the parser and never rewritten. Every
//! token keeps its leading trivia, so printing the tree reproduces the source byte for
//! byte; a node's span is the union of its tokens' own text, trivia excluded.
//!
//! The tree has no `&mut` API: every accessor takes `&self`, the fields are private and
//! the only constructor is [`TreeBuilder`], which the parser consumes. The guarantee is
//! the type system's, as this refusal shows:
//!
//! ```compile_fail
//! use sjs_diag::SourceFile;
//! let file = SourceFile::new("a.js", b"const a = 1".to_vec()).unwrap();
//! let (tree, _) = sjs_syntax::parse(&file);
//! tree.tokens()[0].kind = sjs_syntax::TokenKind::Eof; // cannot assign through `&`
//! ```
//!
//! Reading is free:
//!
//! ```
//! use sjs_diag::SourceFile;
//! let file = SourceFile::new("a.js", b"const a = 1".to_vec()).unwrap();
//! let (tree, diagnostics) = sjs_syntax::parse(&file);
//! assert!(diagnostics.is_empty());
//! assert_eq!(tree.print(&file), "const a = 1");
//! assert_eq!(tree.root().kind(), sjs_syntax::NodeKind::Module);
//! assert!(tree.root().parent().is_none());
//! ```

use std::fmt::Write as _;

use sjs_diag::{SourceFile, Span};

use crate::token::{print_tokens, Token};

/// What a node is; the grammar stories add kinds as they land.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeKind {
    /// The root: a whole file, `Eof` included.
    Module,
    /// `import … from "…"` with its optional attributes (US-021).
    ImportDecl,
    /// `{ a, b as c }` of an import.
    NamedImports,
    /// `a` or `a as b` inside `NamedImports`.
    ImportSpecifier,
    /// `* as ns`.
    NamespaceImport,
    /// The bound name of `import cfg from "./c.json" …`.
    DefaultImport,
    /// `with { type: "json" }`.
    ImportAttributes,
    /// `export …` (US-022): the keyword and, for a list form, the list and its `from`
    /// path; an exported declaration follows the node until its own grammar story.
    ExportDecl,
    /// `{ a, b as c }` of an export.
    NamedExports,
    /// `a` or `a as b` inside `NamedExports`.
    ExportSpecifier,
    /// `const pattern = expr` (US-023).
    ConstDecl,
    /// `let pattern = expr`.
    LetDecl,
    /// `= expr`: a declaration's initializer or a pattern's default.
    Initializer,
    /// `{ a, b: pattern = default, ...rest }`.
    ObjectPattern,
    /// One property of an object pattern: `a`, `a: pattern`, with an optional default.
    PatternProperty,
    /// `[ pattern = default, ...rest ]`.
    ArrayPattern,
    /// One element of an array pattern with its optional default.
    PatternElement,
    /// `...pattern` in a pattern.
    RestElement,
    /// `[async] function [*] name(params) { … }` (US-024).
    FunctionDecl,
    /// The removed `function` expression, parsed for recovery after its E000.
    FunctionExpr,
    /// `[async] (params) => expr | block`.
    ArrowFunction,
    /// `( param, …, ...rest )`.
    Params,
    /// One parameter: a pattern with an optional default.
    Param,
    /// `...pattern` in a parameter list.
    RestParam,
    /// `{ … }` holding declarations and statements.
    Block,
    /// `class Name [extends …] { … }` (US-025).
    ClassDecl,
    /// The removed `class` expression, parsed for recovery after its E000.
    ClassExpr,
    /// `extends Name` or `extends ns.Name`.
    ClassHeritage,
    /// `{ members }` of a class.
    ClassBody,
    /// `[static] key [= expr]`.
    ClassField,
    /// `[static] [async] [*] key(params) { … }`.
    ClassMethod,
    /// `[static] get key() { … }` or `[static] set key(param) { … }`.
    ClassAccessor,
    /// `static { … }`.
    StaticBlock,
    /// `[Symbol.iterator]` and the other three permitted computed keys.
    SymbolKey,
    /// An expression on its own as a statement (US-026).
    ExprStmt,
    /// A bare `;`.
    EmptyStmt,
    /// `if (cond) { … } [else …]`.
    IfStmt,
    /// `while (cond) { … }`.
    WhileStmt,
    /// The classic `for (let i = …; cond; update) { … }`.
    ForStmt,
    /// `for [await] (const|let pattern of iterable) { … }`.
    ForOfStmt,
    /// The removed `for (… in …)`, parsed for recovery.
    ForInStmt,
    /// The update of a classic `for` header.
    ForUpdate,
    /// The removed `do { … } while (cond)`, parsed for recovery.
    DoWhileStmt,
    /// The removed `with (obj) { … }`, parsed for recovery.
    WithStmt,
    /// The removed `label: statement`, parsed for recovery.
    LabelledStmt,
    /// The removed `debugger`, parsed for recovery.
    DebuggerStmt,
    /// The removed `var` declaration, parsed for recovery.
    VarDecl,
    /// `return [expr]`.
    ReturnStmt,
    /// `break`.
    BreakStmt,
    /// `continue`.
    ContinueStmt,
    /// `throw expr`.
    ThrowStmt,
    /// `switch (expr) { clauses }`.
    SwitchStmt,
    /// Stacked labels and one block in a `switch` (US-027).
    CaseClause,
    /// `case literal :` or `default :`.
    CaseLabel,
    /// `try { … } [catch …] [finally …]`.
    TryStmt,
    /// `catch [(name)] { … }`.
    CatchClause,
    /// `finally { … }`.
    FinallyClause,
    /// `( expr )` (US-028).
    ParenExpr,
    /// The removed comma operator, `a, b`, parsed for recovery.
    SequenceExpr,
    /// `target op value` for `=` and the compound operators.
    AssignExpr,
    /// `test ? a : b`.
    CondExpr,
    /// `left op right` for every binary operator, `??`, `&&` and `||` included.
    BinaryExpr,
    /// `! - + ~ typeof` and the removed `void`/`delete` before an operand.
    UnaryExpr,
    /// `await operand`.
    AwaitExpr,
    /// `yield [operand]` or `yield* operand`.
    YieldExpr,
    /// `++`/`--`, before or after the operand.
    UpdateExpr,
    /// `object.name` or `object?.name`.
    MemberExpr,
    /// `object[index]` or `object?.[index]`.
    IndexExpr,
    /// `callee(args)` or `callee?.(args)`.
    CallExpr,
    /// `new Callee(args)`.
    NewExpr,
    /// `import(…)`.
    ImportCall,
    /// The removed `new.target` and `import.meta`, parsed for recovery.
    MetaProperty,
    /// `` tag`…` ``.
    TaggedTemplate,
    /// `` `…${expr}…` ``: a template with substitutions.
    TemplateLit,
    /// `[ … ]` as a value.
    ArrayLit,
    /// `{ … }` as a value.
    ObjectLit,
    /// One `key: value`, shorthand or `key = default` entry of an object literal.
    Property,
    /// `...expr` in an array or object literal or an argument list.
    SpreadElement,
}

/// A node's index in the arena; `NodeId(0)` is the root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

/// A token's index in the token sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TokenId(u32);

/// A child of a node in the arena: a nested node or a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Child {
    Node(NodeId),
    Token(TokenId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NodeData {
    kind: NodeKind,
    parent: Option<NodeId>,
    children: Vec<Child>,
    /// Index of the first token inside the node, `end` when it has none.
    first: usize,
    /// One past the last token inside the node.
    end: usize,
}

/// A whole file as a tree over its tokens. Immutable once built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxTree {
    tokens: Vec<Token>,
    nodes: Vec<NodeData>,
}

fn to_index(id: u32) -> usize {
    usize::try_from(id).unwrap_or(usize::MAX)
}

fn to_id(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

impl SyntaxTree {
    /// The root node.
    #[must_use]
    pub fn root(&self) -> NodeRef<'_> {
        NodeRef {
            tree: self,
            id: NodeId(0),
        }
    }

    /// The node with this id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> NodeRef<'_> {
        NodeRef { tree: self, id }
    }

    /// The token with this id.
    #[must_use]
    pub fn token(&self, id: TokenId) -> TokenRef<'_> {
        TokenRef { tree: self, id }
    }

    /// Every token in source order, `Eof` last.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// How many nodes the arena holds.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The source text, reassembled from every token's trivia and text.
    #[must_use]
    pub fn print(&self, file: &SourceFile) -> String {
        print_tokens(&self.tokens, file)
    }

    /// The tree as nested parentheses: each node as `(Kind child …)` and each token as
    /// its text, trivia dropped. For tests and debugging.
    #[must_use]
    pub fn to_sexp(&self, file: &SourceFile) -> String {
        let mut out = String::new();
        self.root().write_sexp(file, &mut out);
        out
    }

    fn data(&self, id: NodeId) -> &NodeData {
        &self.nodes[to_index(id.0)]
    }

    /// The node whose children include `id`.
    fn owner_of(&self, id: TokenId) -> NodeId {
        // Every token sits in exactly one node's children (the builder puts it there), so
        // walk from the root to the innermost node whose token range holds it.
        let index = to_index(id.0);
        let mut owner = NodeId(0);
        loop {
            let next = self
                .data(owner)
                .children
                .iter()
                .find_map(|child| match child {
                    Child::Node(node)
                        if self.data(*node).first <= index && index < self.data(*node).end =>
                    {
                        Some(*node)
                    }
                    _ => None,
                });
            match next {
                Some(node) => owner = node,
                None => return owner,
            }
        }
    }
}

/// A node seen through its tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeRef<'t> {
    tree: &'t SyntaxTree,
    id: NodeId,
}

/// A token seen through its tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenRef<'t> {
    tree: &'t SyntaxTree,
    id: TokenId,
}

/// A child of a node: a nested node or a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element<'t> {
    /// A nested node.
    Node(NodeRef<'t>),
    /// A token.
    Token(TokenRef<'t>),
}

impl<'t> NodeRef<'t> {
    /// Its id.
    #[must_use]
    pub fn id(self) -> NodeId {
        self.id
    }

    /// What it is.
    #[must_use]
    pub fn kind(self) -> NodeKind {
        self.tree.data(self.id).kind
    }

    /// The node it sits in; `None` for the root.
    #[must_use]
    pub fn parent(self) -> Option<NodeRef<'t>> {
        self.tree.data(self.id).parent.map(|id| self.tree.node(id))
    }

    /// Its children in source order.
    pub fn children(self) -> impl Iterator<Item = Element<'t>> + 't {
        let tree = self.tree;
        tree.data(self.id)
            .children
            .iter()
            .map(move |child| match *child {
                Child::Node(id) => Element::Node(tree.node(id)),
                Child::Token(id) => Element::Token(tree.token(id)),
            })
    }

    /// Every token inside it, nested nodes included, in source order.
    #[must_use]
    pub fn tokens(self) -> &'t [Token] {
        let data = self.tree.data(self.id);
        &self.tree.tokens[data.first..data.end]
    }

    /// From the first token's own text to the last token's end, leading trivia
    /// excluded. A node without tokens has the empty span where its first token would
    /// start: at the text of the token that follows it.
    #[must_use]
    pub fn span(self) -> Span {
        let data = self.tree.data(self.id);
        let tokens = &self.tree.tokens;
        if let Some((first, rest)) = tokens[data.first..data.end].split_first() {
            let last = rest.last().unwrap_or(first);
            return Span::new(
                first.span.start,
                last.span.end,
                first.span.byte_start,
                last.span.byte_end,
            );
        }
        // The token after the node; `Eof` is last, so one always exists.
        let next = &tokens[data.first.min(tokens.len() - 1)];
        Span::new(
            next.span.start,
            next.span.start,
            next.span.byte_start,
            next.span.byte_start,
        )
    }
}

impl NodeRef<'_> {
    fn write_sexp(self, file: &SourceFile, out: &mut String) {
        out.push('(');
        let _ = write!(out, "{:?}", self.kind());
        for child in self.children() {
            out.push(' ');
            match child {
                Element::Node(node) => node.write_sexp(file, out),
                Element::Token(token) => {
                    let text = file.slice(&token.token().span).unwrap_or("");
                    out.push_str(if text.is_empty() { "<eof>" } else { text });
                }
            }
        }
        out.push(')');
    }
}

impl<'t> TokenRef<'t> {
    /// Its id.
    #[must_use]
    pub fn id(self) -> TokenId {
        self.id
    }

    /// The token itself.
    #[must_use]
    pub fn token(self) -> &'t Token {
        &self.tree.tokens[to_index(self.id.0)]
    }

    /// The node whose child it is.
    #[must_use]
    pub fn parent(self) -> NodeRef<'t> {
        self.tree.node(self.tree.owner_of(self.id))
    }
}

/// A place in the innermost open node, taken before its next child, to which
/// [`TreeBuilder::start_node_at`] can later return: an operand learns it is the left
/// side of `a + b` only at the `+`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    children: usize,
    tokens: usize,
}

impl Marker {
    /// How many tokens the builder held when the marker was taken: the index of the
    /// first token added after it.
    #[must_use]
    pub fn token_index(self) -> usize {
        self.tokens
    }
}

/// Builds a tree once, in source order: open a node, add its tokens and nested nodes,
/// close it. Consumed by [`TreeBuilder::finish`], after which the tree is immutable.
#[derive(Debug, Default)]
pub struct TreeBuilder {
    tokens: Vec<Token>,
    nodes: Vec<NodeData>,
    open: Vec<NodeId>,
}

impl TreeBuilder {
    /// An empty builder; the first node started is the root.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a node inside the innermost open one.
    pub fn start_node(&mut self, kind: NodeKind) -> NodeId {
        let id = NodeId(to_id(self.nodes.len()));
        let parent = self.open.last().copied();
        self.nodes.push(NodeData {
            kind,
            parent,
            children: Vec::new(),
            first: self.tokens.len(),
            end: self.tokens.len(),
        });
        if let Some(parent) = parent {
            self.nodes[to_index(parent.0)]
                .children
                .push(Child::Node(id));
        }
        self.open.push(id);
        id
    }

    /// The place before the next child of the innermost open node.
    #[must_use]
    pub fn marker(&self) -> Marker {
        let children = self
            .open
            .last()
            .map_or(0, |id| self.nodes[to_index(id.0)].children.len());
        Marker {
            children,
            tokens: self.tokens.len(),
        }
    }

    /// Opens a node that adopts every child the innermost open node gained since
    /// `marker` was taken in it, and makes it the innermost open node.
    pub fn start_node_at(&mut self, marker: Marker, kind: NodeKind) -> NodeId {
        let id = NodeId(to_id(self.nodes.len()));
        let parent = self.open.last().copied();
        let adopted = match parent {
            Some(parent) => {
                let children = &mut self.nodes[to_index(parent.0)].children;
                let at = marker.children.min(children.len());
                children.split_off(at)
            }
            None => Vec::new(),
        };
        for child in &adopted {
            if let Child::Node(node) = child {
                self.nodes[to_index(node.0)].parent = Some(id);
            }
        }
        self.nodes.push(NodeData {
            kind,
            parent,
            children: adopted,
            first: marker.tokens,
            end: self.tokens.len(),
        });
        if let Some(parent) = parent {
            self.nodes[to_index(parent.0)]
                .children
                .push(Child::Node(id));
        }
        self.open.push(id);
        id
    }

    /// The kind of a node already started.
    pub(crate) fn kind_of(&self, id: NodeId) -> NodeKind {
        self.nodes[to_index(id.0)].kind
    }

    /// The children of a node already started, in source order.
    pub(crate) fn children_of(&self, id: NodeId) -> &[Child] {
        &self.nodes[to_index(id.0)].children
    }

    /// From the first token's text to the last's end, for a closed node that holds
    /// tokens.
    pub(crate) fn span_of(&self, id: NodeId) -> Span {
        let data = &self.nodes[to_index(id.0)];
        let first = &self.tokens[data.first];
        let last = &self.tokens[data.end - 1];
        Span::new(
            first.span.start,
            last.span.end,
            first.span.byte_start,
            last.span.byte_end,
        )
    }

    /// Adds a token to the innermost open node.
    pub fn token(&mut self, token: Token) -> TokenId {
        let id = TokenId(to_id(self.tokens.len()));
        self.tokens.push(token);
        if let Some(parent) = self.open.last() {
            self.nodes[to_index(parent.0)]
                .children
                .push(Child::Token(id));
        }
        id
    }

    /// A token already added.
    pub(crate) fn token_ref(&self, id: TokenId) -> &Token {
        &self.tokens[to_index(id.0)]
    }

    /// Changes the kind of a node already started, for a grammar that learns what a
    /// node is only after its first tokens (the header of a `for` loop).
    pub fn retag(&mut self, id: NodeId, kind: NodeKind) {
        self.nodes[to_index(id.0)].kind = kind;
    }

    /// Closes the innermost open node.
    pub fn finish_node(&mut self) {
        if let Some(id) = self.open.pop() {
            self.nodes[to_index(id.0)].end = self.tokens.len();
        }
    }

    /// The finished tree. Nodes still open are closed; a builder that never started a
    /// node gets an empty `Module` root so the tree always has one.
    #[must_use]
    pub fn finish(mut self) -> SyntaxTree {
        if self.nodes.is_empty() {
            self.start_node(NodeKind::Module);
        }
        while !self.open.is_empty() {
            self.finish_node();
        }
        SyntaxTree {
            tokens: self.tokens,
            nodes: self.nodes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Element, NodeId, NodeKind, NodeRef, SyntaxTree, TokenId, TokenRef, TreeBuilder};
    use crate::lex;
    use crate::token::TokenKind;
    use sjs_diag::{Position, SourceFile, Span};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("valid UTF-8")
    }

    fn as_node(child: Option<Element<'_>>) -> Option<NodeRef<'_>> {
        match child? {
            Element::Node(node) => Some(node),
            Element::Token(_) => None,
        }
    }

    fn as_token(child: Option<Element<'_>>) -> Option<TokenRef<'_>> {
        match child? {
            Element::Token(token) => Some(token),
            Element::Node(_) => None,
        }
    }

    #[test]
    fn the_test_helpers_tell_nodes_from_tokens() {
        let (_, tree) = nested("a b c");
        let mut children = tree.root().children();
        let first = children.next();
        let second = children.next();
        assert!(as_node(first).is_some());
        assert!(as_token(first).is_none());
        assert!(as_node(second).is_none());
        assert!(as_token(second).is_some());
        assert!(as_node(None).is_none());
        assert!(as_token(None).is_none());
    }

    #[test]
    fn an_empty_node_after_the_last_token_anchors_at_that_token() {
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        let eof = lex(&file("ab")).0.tokens.remove(1);
        builder.token(eof);
        builder.start_node(NodeKind::Module);
        builder.finish_node();
        let tree = builder.finish();
        assert_eq!(
            tree.node(NodeId(1)).span(),
            Span::new(Position::new(1, 3), Position::new(1, 3), 2, 2),
            "at the Eof that precedes it, there being nothing after"
        );
    }

    /// `Module(Module(a) b Module() c Eof)` over `a b c`.
    fn nested(text: &str) -> (SourceFile, SyntaxTree) {
        let f = file(text);
        let (stream, _) = lex(&f);
        let mut tokens = stream.tokens.into_iter();
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        builder.start_node(NodeKind::Module);
        builder.token(tokens.next().expect("a"));
        builder.finish_node();
        builder.token(tokens.next().expect("b"));
        builder.start_node(NodeKind::Module);
        builder.finish_node();
        for token in tokens {
            builder.token(token);
        }
        builder.finish_node();
        (f, builder.finish())
    }

    #[test]
    fn the_root_has_no_parent_and_owns_every_token() {
        let (f, tree) = nested("a b c");
        let root = tree.root();
        assert_eq!(root.id(), NodeId(0));
        assert_eq!(root.kind(), NodeKind::Module);
        assert!(root.parent().is_none());
        assert_eq!(root.tokens().len(), 4, "a, b, c, Eof");
        assert_eq!(tree.tokens().len(), 4);
        assert_eq!(tree.node_count(), 3);
        assert_eq!(tree.print(&f), "a b c");
        assert_eq!(root.children().count(), 5, "node, b, node, c, Eof");
        let kinds: Vec<&str> = root
            .children()
            .map(|child| match child {
                Element::Node(_) => "node",
                Element::Token(_) => "token",
            })
            .collect();
        assert_eq!(kinds, ["node", "token", "node", "token", "token"]);
    }

    #[test]
    fn nested_nodes_know_their_parent_tokens_and_span() {
        let (_, tree) = nested("a b c");
        let root = tree.root();
        let inner = as_node(root.children().next()).expect("first child is a node");
        assert_eq!(inner.parent().map(super::NodeRef::id), Some(NodeId(0)));
        assert_eq!(inner.tokens().len(), 1);
        assert_eq!(inner.tokens()[0].kind, TokenKind::Ident);
        assert_eq!(
            inner.span(),
            Span::new(Position::new(1, 1), Position::new(1, 2), 0, 1)
        );
        assert_eq!(tree.node(inner.id()).kind(), NodeKind::Module);
        assert_eq!(
            root.span(),
            Span::new(Position::new(1, 1), Position::new(1, 6), 0, 5),
            "the root spans from `a` to `Eof` (zero width at 5)"
        );
    }

    #[test]
    fn a_node_without_tokens_has_the_empty_span_where_the_next_token_starts() {
        let (_, tree) = nested("a b  c");
        let empty = tree.node(NodeId(2));
        assert_eq!(empty.tokens().len(), 0);
        assert_eq!(
            empty.span(),
            Span::new(Position::new(1, 6), Position::new(1, 6), 5, 5),
            "at the `c`, after its leading whitespace"
        );
        assert_eq!(empty.parent().map(super::NodeRef::id), Some(NodeId(0)));
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        builder.start_node(NodeKind::Module);
        builder.finish_node();
        let eof = lex(&file("")).0.tokens.remove(0);
        builder.token(eof);
        let tree = builder.finish();
        assert_eq!(
            tree.node(NodeId(1)).span(),
            Span::new(Position::new(1, 1), Position::new(1, 1), 0, 0),
            "an empty node before Eof in an empty file"
        );
        assert_eq!(
            tree.root().span(),
            Span::new(Position::new(1, 1), Position::new(1, 1), 0, 0)
        );
    }

    #[test]
    fn a_node_span_excludes_leading_trivia_and_includes_the_last_token() {
        let f = file("  a /* c */ b // t\n");
        let (stream, _) = lex(&f);
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        builder.start_node(NodeKind::Module);
        let mut tokens = stream.tokens.into_iter();
        builder.token(tokens.next().expect("a"));
        builder.token(tokens.next().expect("b"));
        builder.finish_node();
        for token in tokens {
            builder.token(token);
        }
        let tree = builder.finish();
        assert_eq!(
            tree.node(NodeId(1)).span(),
            Span::new(Position::new(1, 3), Position::new(1, 14), 2, 13),
            "from `a` to the end of `b`"
        );
        assert_eq!(tree.print(&f), "  a /* c */ b // t\n");
    }

    #[test]
    fn tokens_know_the_innermost_node_that_owns_them() {
        let (_, tree) = nested("a b c");
        assert_eq!(
            tree.token(TokenId(0)).parent().id(),
            NodeId(1),
            "`a` sits in the inner node"
        );
        assert_eq!(
            tree.token(TokenId(1)).parent().id(),
            NodeId(0),
            "`b` sits in the root"
        );
        assert_eq!(
            tree.token(TokenId(3)).parent().id(),
            NodeId(0),
            "Eof sits in the root"
        );
        assert_eq!(tree.token(TokenId(0)).id(), TokenId(0));
        assert_eq!(tree.token(TokenId(2)).token().kind, TokenKind::Ident);
        let b = as_token(tree.root().children().nth(1)).expect("second child is a token");
        assert_eq!(b.id(), TokenId(1));
    }

    #[test]
    fn the_sexp_rendering_shows_nodes_and_token_text() {
        let (f, tree) = nested("a b  c");
        assert_eq!(tree.to_sexp(&f), "(Module (Module a) b (Module) c <eof>)");
    }

    #[test]
    fn a_node_opened_at_a_marker_adopts_what_followed_it() {
        let f = file("a b c");
        let mut tokens = lex(&f).0.tokens.into_iter();
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        builder.token(tokens.next().expect("a"));
        let marker = builder.marker();
        assert_eq!(marker.token_index(), 1, "after `a`");
        builder.start_node(NodeKind::ExprStmt);
        builder.token(tokens.next().expect("b"));
        builder.finish_node();
        builder.token(tokens.next().expect("c"));
        let wrapper = builder.start_node_at(marker, NodeKind::BinaryExpr);
        builder.finish_node();
        assert_eq!(builder.kind_of(wrapper), NodeKind::BinaryExpr);
        assert_eq!(
            builder.span_of(wrapper),
            Span::new(Position::new(1, 3), Position::new(1, 6), 2, 5),
            "from `b` to the end of `c`"
        );
        assert_eq!(
            builder.children_of(wrapper).len(),
            2,
            "the ExprStmt and `c`"
        );
        for token in tokens {
            builder.token(token);
        }
        builder.finish_node();
        let tree = builder.finish();
        assert_eq!(
            tree.to_sexp(&f),
            "(Module a (BinaryExpr (ExprStmt b) c) <eof>)"
        );
        let adopted = tree.node(NodeId(1));
        assert_eq!(adopted.kind(), NodeKind::ExprStmt);
        assert_eq!(
            adopted.parent().map(NodeRef::id),
            Some(wrapper),
            "the adopted node's parent is the wrapper"
        );
        assert_eq!(
            tree.node(wrapper).parent().map(NodeRef::id),
            Some(NodeId(0))
        );
        assert_eq!(tree.node(wrapper).tokens().len(), 2);
        let c = tree.token(TokenId(2));
        assert_eq!(c.parent().id(), wrapper, "`c` is found inside the wrapper");
        let mut builder = TreeBuilder::new();
        let marker = builder.marker();
        assert_eq!(marker.token_index(), 0);
        builder.start_node_at(marker, NodeKind::Module);
        builder.finish_node();
        assert_eq!(
            builder.finish().node_count(),
            1,
            "a node opened at a marker with nothing open becomes the root"
        );
    }

    #[test]
    fn the_builder_tolerates_an_unfinished_or_empty_build() {
        let tree = TreeBuilder::new().finish();
        assert_eq!(tree.node_count(), 1, "an empty build still has a root");
        assert_eq!(tree.root().kind(), NodeKind::Module);
        assert_eq!(tree.tokens().len(), 0);
        let mut builder = TreeBuilder::new();
        let eof = lex(&file("")).0.tokens.remove(0);
        builder.token(eof.clone());
        let tree = builder.finish();
        assert_eq!(
            tree.tokens().len(),
            1,
            "a token before any node still belongs to the tree"
        );
        assert_eq!(
            tree.root().tokens().len(),
            0,
            "but not to the root, which started after it"
        );
        let mut builder = TreeBuilder::new();
        builder.start_node(NodeKind::Module);
        builder.start_node(NodeKind::Module);
        builder.token(eof);
        let tree = builder.finish();
        assert_eq!(
            tree.node(NodeId(1)).tokens().len(),
            1,
            "open nodes are closed at the end"
        );
        assert_eq!(tree.root().tokens().len(), 1);
        let mut builder = TreeBuilder::new();
        builder.finish_node();
        assert_eq!(
            builder.finish().node_count(),
            1,
            "finishing with nothing open is harmless"
        );
    }
}
