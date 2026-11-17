//! Scopes, declarations and references of one file (SPEC.md §4.2, §4.4), with the
//! findings that need nothing else: a name declared twice in one scope (E000), a block
//! that declares a name of the function it is in (E107) and a binding used before its
//! declaration has run (E135, the temporal dead zone).

use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// What opens a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// The file.
    Module,
    /// A `function`, method or accessor, or the initializer of an instance field (which
    /// runs when an instance is made): its parameters live here, its body in a block.
    Function,
    /// An arrow function: its parameters, and its body when that is an expression.
    Arrow,
    /// `{ … }`: a statement block, a function body, a `case` body, a `static` block.
    Block,
    /// The header of a classic `for`.
    For,
    /// The header of a `for…of`.
    ForOf,
    /// A `switch`; each of its `case` bodies is a block of its own.
    Switch,
    /// A `catch` clause: its binding.
    Catch,
    /// A class, from its name to the end of its body.
    Class,
}

/// A scope of the file. Scopes are numbered in source order, so a scope's parent
/// comes before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    /// What opened it.
    pub kind: ScopeKind,
    /// The scope around it; `None` for the module.
    pub parent: Option<usize>,
    /// The text it covers.
    pub span: Span,
    /// The first declaration of each name.
    names: BTreeMap<String, usize>,
    /// For the block that is the body of a function, arrow or `catch`: the scope of
    /// the parameters, whose names the body may not declare again.
    head: Option<usize>,
}

/// How a name is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    /// A name an `import` binds.
    Import,
    /// `const` (and the names of its pattern).
    Const,
    /// `let`, and the removed `var`.
    Let,
    /// A `function` declaration, hoisted to the top of its block.
    Function,
    /// A class.
    Class,
    /// A parameter (and the names of its pattern).
    Param,
    /// The binding of a `catch`.
    Catch,
}

/// A declared name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The name.
    pub name: String,
    /// Where it is written.
    pub span: Span,
    /// How it is declared.
    pub kind: DeclKind,
    /// The scope it belongs to.
    pub scope: usize,
    /// The offset from which it holds a value.
    ready: usize,
    /// A stretch after that in which it still holds none: the initializer of a
    /// declaration that destructures, the iterable of a `for…of`.
    dead: Range<usize>,
    /// What declares it, so that two names of one pattern or one parameter list (which
    /// the parser judges) are told from two declarations.
    origin: usize,
    /// Where the function it names begins, when it names one.
    function: Option<usize>,
}

/// A use of a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    /// The name.
    pub name: String,
    /// Where it is written.
    pub span: Span,
    /// The scope it is written in.
    pub scope: usize,
    /// The declaration it resolves to: an index into [`Bindings::declarations`]. `None`
    /// for a name the file does not declare (a global, or nothing at all).
    pub declaration: Option<usize>,
    /// Whether it is the callee of a call, `name(…)`.
    pub call: bool,
    /// Whether it is a name in an `export { … }` list, which evaluates nothing.
    pub export: bool,
}

/// The scopes of a file, what each declares and what every name refers to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bindings {
    /// The scopes, the module first.
    pub scopes: Vec<Scope>,
    /// The declarations, in source order.
    pub declarations: Vec<Declaration>,
    /// The references, in source order.
    pub references: Vec<Reference>,
}

impl Bindings {
    /// The declaration `name` resolves to from `scope`: the nearest one around it.
    #[must_use]
    pub fn lookup(&self, scope: usize, name: &str) -> Option<usize> {
        let mut at = Some(scope);
        while let Some(scope) = at {
            if let Some(&found) = self.scopes[scope].names.get(name) {
                return Some(found);
            }
            at = self.scopes[scope].parent;
        }
        None
    }

    /// Whether `scope` is `ancestor` or inside it.
    fn within(&self, scope: usize, ancestor: usize) -> bool {
        let mut at = Some(scope);
        while let Some(scope) = at {
            if scope == ancestor {
                return true;
            }
            at = self.scopes[scope].parent;
        }
        false
    }
}

/// Binds the names of a file: its scopes, declarations and references, with E000 for a
/// name declared twice in one scope, E107 for a name of the enclosing function declared
/// again in a block and E135 for a binding used, or reached through a
/// call, before its declaration has run.
#[must_use]
pub fn bind(source: &SourceFile, tree: &SyntaxTree) -> (Bindings, Diagnostics) {
    let mut binder = Binder {
        source,
        out: Bindings::default(),
        functions: BTreeMap::new(),
        diagnostics: Diagnostics::new(),
    };
    binder.node(tree.root(), None, None);
    binder.resolve();
    (binder.out, binder.diagnostics)
}

/// What the names of a pattern are declared as.
#[derive(Clone)]
struct Declaring {
    kind: DeclKind,
    origin: usize,
    dead: Range<usize>,
}

struct Binder<'a> {
    source: &'a SourceFile,
    out: Bindings,
    /// The scope of each function and arrow, by where it begins.
    functions: BTreeMap<usize, usize>,
    diagnostics: Diagnostics,
}

/// The node kinds every identifier directly inside is a use of a name.
const USES: &[NodeKind] = &[
    NodeKind::ExprStmt,
    NodeKind::Initializer,
    NodeKind::ReturnStmt,
    NodeKind::ThrowStmt,
    NodeKind::IfStmt,
    NodeKind::WhileStmt,
    NodeKind::DoWhileStmt,
    NodeKind::WithStmt,
    NodeKind::SwitchStmt,
    NodeKind::CaseLabel,
    NodeKind::ForStmt,
    NodeKind::ForInStmt,
    NodeKind::ForUpdate,
    NodeKind::ParenExpr,
    NodeKind::SequenceExpr,
    NodeKind::AssignExpr,
    NodeKind::CondExpr,
    NodeKind::BinaryExpr,
    NodeKind::UnaryExpr,
    NodeKind::UpdateExpr,
    NodeKind::IndexExpr,
    NodeKind::CallExpr,
    NodeKind::OptionalChain,
    NodeKind::NewExpr,
    NodeKind::TaggedTemplate,
    NodeKind::TemplateLit,
    NodeKind::ArrayLit,
    NodeKind::SpreadElement,
    // The patterns of an assignment; those of a declaration declare.
    NodeKind::ArrayPattern,
    NodeKind::PatternElement,
    NodeKind::RestElement,
];

const PATTERNS: &[NodeKind] = &[
    NodeKind::ObjectPattern,
    NodeKind::ArrayPattern,
    NodeKind::PatternProperty,
    NodeKind::PatternElement,
    NodeKind::RestElement,
];

fn is_punct(element: Option<&Element<'_>>, punct: Punct) -> bool {
    matches!(element, Some(Element::Token(token)) if token.token().kind == TokenKind::Punct(punct))
}

fn is_keyword(element: Option<&Element<'_>>, keyword: Keyword) -> bool {
    matches!(element, Some(Element::Token(token)) if token.token().kind == TokenKind::Keyword(keyword))
}

fn is_ident(element: &Element<'_>) -> bool {
    matches!(element, Element::Token(token) if token.token().kind == TokenKind::Ident)
}

fn is_node(element: Option<&Element<'_>>, kind: NodeKind) -> bool {
    matches!(element, Some(Element::Node(node)) if node.kind() == kind)
}

/// The child of `node` of the given kind, if it has one.
fn child(node: NodeRef<'_>, kind: NodeKind) -> Option<NodeRef<'_>> {
    node.children().find_map(|child| match child {
        Element::Node(inner) if inner.kind() == kind => Some(inner),
        _ => None,
    })
}

/// Whether a class field is `static`: only then do two words come before its value,
/// the word and the field's name.
fn is_static(field: NodeRef<'_>) -> bool {
    let mut words = field
        .children()
        .take_while(|child| !is_node(Some(child), NodeKind::Initializer));
    words
        .nth(1)
        .is_some_and(|name| !is_punct(Some(&name), Punct::Semicolon))
}

/// Whether the identifier at `children[at]` of `node`, which declares nothing, is a use
/// of a name, and not a key, a property name or a contextual word.
fn is_use(node: NodeRef<'_>, children: &[Element<'_>], at: usize) -> bool {
    let kind = node.kind();
    let before = at.checked_sub(1).and_then(|before| children.get(before));
    let after = children.get(at + 1);
    let earlier = &children[..at];
    let is_key = is_punct(after, Punct::Colon) || is_node(after, NodeKind::Params);
    let from_elsewhere = || {
        let list = node.parent().and_then(NodeRef::parent);
        list.is_some_and(|export| {
            export.children().any(|child| {
                matches!(child, Element::Token(token) if matches!(token.token().kind, TokenKind::String(_)))
            })
        })
    };
    match kind {
        NodeKind::MemberExpr | NodeKind::ClassHeritage => {
            !is_punct(before, Punct::Dot) && !is_punct(before, Punct::QuestionDot)
        }
        NodeKind::AwaitExpr | NodeKind::YieldExpr => at > 0,
        NodeKind::ArrowFunction => earlier
            .iter()
            .any(|child| is_punct(Some(child), Punct::Arrow)),
        NodeKind::Property | NodeKind::PatternProperty => !is_key,
        NodeKind::ForOfStmt => {
            let header = earlier
                .iter()
                .skip_while(|child| matches!(child, Element::Token(_)));
            header.filter(|child| is_ident(child)).count() > 0
        }
        NodeKind::ExportSpecifier => !earlier.iter().any(is_ident) && !from_elsewhere(),
        _ => USES.contains(&kind),
    }
}

impl Binder<'_> {
    /// The kind of scope `node` opens, if it opens one.
    fn opens(node: NodeRef<'_>) -> Option<ScopeKind> {
        let parent = node.parent();
        Some(match node.kind() {
            NodeKind::Module => ScopeKind::Module,
            NodeKind::FunctionDecl
            | NodeKind::FunctionExpr
            | NodeKind::ClassMethod
            | NodeKind::ClassAccessor => ScopeKind::Function,
            NodeKind::Property if child(node, NodeKind::Params).is_some() => ScopeKind::Function,
            NodeKind::Initializer
                if parent.is_some_and(|field| {
                    field.kind() == NodeKind::ClassField && !is_static(field)
                }) =>
            {
                ScopeKind::Function
            }
            NodeKind::ArrowFunction => ScopeKind::Arrow,
            NodeKind::Block => ScopeKind::Block,
            NodeKind::ForStmt => ScopeKind::For,
            NodeKind::ForOfStmt | NodeKind::ForInStmt => ScopeKind::ForOf,
            NodeKind::SwitchStmt => ScopeKind::Switch,
            NodeKind::CatchClause => ScopeKind::Catch,
            NodeKind::ClassDecl | NodeKind::ClassExpr => ScopeKind::Class,
            _ => return None,
        })
    }

    /// Walks `node` in the scope `outer`; `incoming` is what the pattern it is part of
    /// declares, when it is part of one that declares.
    fn node(&mut self, node: NodeRef<'_>, outer: Option<usize>, incoming: Option<&Declaring>) {
        let kind = node.kind();
        let span = node.span();
        let scope = match Self::opens(node) {
            None => outer.unwrap_or_default(),
            Some(opened) => {
                let scope = self.out.scopes.len();
                let is_body = kind == NodeKind::Block
                    && outer.is_some_and(|outer| {
                        let head = self.out.scopes[outer].kind;
                        let around = self.out.scopes[outer].span;
                        matches!(
                            head,
                            ScopeKind::Function | ScopeKind::Arrow | ScopeKind::Catch
                        ) && around.byte_end == span.byte_end
                    });
                self.out.scopes.push(Scope {
                    kind: opened,
                    parent: outer,
                    span,
                    names: BTreeMap::new(),
                    head: outer.filter(|_| is_body),
                });
                if matches!(opened, ScopeKind::Function | ScopeKind::Arrow) {
                    self.functions.insert(span.byte_start, scope);
                }
                scope
            }
        };
        let own = match kind {
            NodeKind::ConstDecl | NodeKind::LetDecl | NodeKind::VarDecl => {
                let own = child(node, NodeKind::Initializer).map(NodeRef::span);
                // The iterable of a `for…of` runs before the declaration it fills.
                let block = node
                    .parent()
                    .and_then(|header| child(header, NodeKind::Block));
                let iterable = block.map_or(span.byte_end, |block| block.span().byte_start);
                Some(Declaring {
                    kind: if kind == NodeKind::ConstDecl {
                        DeclKind::Const
                    } else {
                        DeclKind::Let
                    },
                    origin: span.byte_start,
                    dead: own.map_or(span.byte_end..iterable, |own| own.byte_start..own.byte_end),
                })
            }
            NodeKind::Param | NodeKind::RestParam | NodeKind::Params => {
                // The list the parameter is in, which is the node itself for a
                // parameter written without parentheses.
                let list = node.parent().filter(|_| kind != NodeKind::Params);
                Some(Declaring {
                    kind: DeclKind::Param,
                    origin: list.map_or(span.byte_start, |list| list.span().byte_start),
                    dead: 0..0,
                })
            }
            _ => incoming.cloned(),
        };
        let children: Vec<Element<'_>> = node.children().collect();
        for (at, element) in children.iter().enumerate() {
            match element {
                Element::Node(inner) => {
                    let declaring = own.as_ref().filter(|_| PATTERNS.contains(&inner.kind()));
                    self.node(*inner, Some(scope), declaring);
                }
                Element::Token(token) if token.token().kind == TokenKind::Ident => {
                    let name = self
                        .source
                        .slice(&token.token().span)
                        .unwrap_or_default()
                        .to_owned();
                    let ident = token.token().span;
                    self.ident(
                        node,
                        &children,
                        at,
                        (name, ident),
                        (outer, scope),
                        own.as_ref(),
                    );
                }
                Element::Token(_) => {}
            }
        }
    }

    /// Judges the identifier at `children[at]` of `node`: a declaration, a use of a
    /// name, or neither (a key, a property name, a contextual word).
    fn ident(
        &mut self,
        node: NodeRef<'_>,
        children: &[Element<'_>],
        at: usize,
        (name, ident): (String, Span),
        (outer, scope): (Option<usize>, usize),
        own: Option<&Declaring>,
    ) {
        let kind = node.kind();
        let span = node.span();
        let before = at.checked_sub(1).and_then(|before| children.get(before));
        let after = children.get(at + 1);
        let is_key = is_punct(after, Punct::Colon) || is_node(after, NodeKind::Params);
        let outer = outer.unwrap_or_default();
        let new_declaration = |kind, scope, ready, function| Declaration {
            name: name.clone(),
            span: ident,
            kind,
            scope,
            ready,
            dead: 0..0,
            origin: ident.byte_start,
            function,
        };
        let declared = match kind {
            NodeKind::PatternProperty if is_key => return,
            _ if own.is_some() => own.map(|own| Declaration {
                dead: own.dead.clone(),
                origin: own.origin,
                function: child(node, NodeKind::Initializer)
                    .and_then(|value| child(value, NodeKind::ArrowFunction))
                    .map(|arrow| arrow.span().byte_start),
                ..new_declaration(own.kind, scope, span.byte_end, None)
            }),
            NodeKind::CatchClause => Some(new_declaration(
                DeclKind::Catch,
                scope,
                ident.byte_end,
                None,
            )),
            NodeKind::ImportSpecifier | NodeKind::NamespaceImport | NodeKind::DefaultImport => {
                if children[at + 1..].iter().any(is_ident) {
                    return;
                }
                Some(new_declaration(DeclKind::Import, scope, 0, None))
            }
            NodeKind::FunctionDecl | NodeKind::FunctionExpr
                if is_keyword(before, Keyword::Function) || is_punct(before, Punct::Star) =>
            {
                let home = if kind == NodeKind::FunctionDecl {
                    outer
                } else {
                    scope
                };
                Some(new_declaration(
                    DeclKind::Function,
                    home,
                    0,
                    Some(span.byte_start),
                ))
            }
            NodeKind::ClassDecl | NodeKind::ClassExpr => {
                let home = if kind == NodeKind::ClassDecl {
                    outer
                } else {
                    scope
                };
                let body = child(node, NodeKind::ClassBody)
                    .map_or(span.byte_end, |body| body.span().byte_start);
                Some(new_declaration(DeclKind::Class, home, body, None))
            }
            _ => None,
        };
        if let Some(declaration) = declared {
            self.declare(declaration);
            return;
        }
        let used = is_use(node, children, at);
        if used {
            self.out.references.push(Reference {
                name,
                span: ident,
                scope,
                declaration: None,
                call: kind == NodeKind::CallExpr && at == 0,
                export: kind == NodeKind::ExportSpecifier,
            });
        }
    }

    /// Records a declaration; a second one of its name in its scope (or, for the body
    /// of a function or `catch`, among the parameters) is E000.
    fn declare(&mut self, declaration: Declaration) {
        let scope = &self.out.scopes[declaration.scope];
        let head = scope
            .head
            .and_then(|head| self.out.scopes[head].names.get(&declaration.name));
        let first = scope.names.get(&declaration.name).or(head).copied();
        let id = self.out.declarations.len();
        match first.map(|first| &self.out.declarations[first]) {
            None => {
                let names = &mut self.out.scopes[declaration.scope].names;
                names.insert(declaration.name.clone(), id);
            }
            Some(first) if first.origin == declaration.origin => {}
            Some(first) => self.diagnostics.push(Diagnostic::new(
                Code::E000,
                self.source.path(),
                declaration.span,
                format!(
                    "`{}` is already declared in this scope, on line {}",
                    declaration.name, first.span.start.line
                ),
            )),
        }
        self.out.declarations.push(declaration);
    }

    /// Resolves every reference, and reports the uses that come too early.
    fn resolve(&mut self) {
        let out = &mut self.out;
        // The function each scope runs as part of: itself for a function or an arrow,
        // else that of the scope around it.
        let mut runs_in: Vec<usize> = Vec::new();
        for (at, scope) in out.scopes.iter().enumerate() {
            let own = matches!(scope.kind, ScopeKind::Function | ScopeKind::Arrow);
            let around = scope.parent.filter(|_| !own);
            runs_in.push(around.map_or(at, |parent| runs_in[parent]));
        }
        let mut inside: Vec<Vec<usize>> = vec![Vec::new(); out.scopes.len()];
        for at in 0..out.references.len() {
            let found = out.lookup(out.references[at].scope, &out.references[at].name);
            out.references[at].declaration = found;
            inside[runs_in[out.references[at].scope]].push(at);
        }
        let early = |declaration: &Declaration, at: usize| {
            at < declaration.ready || declaration.dead.contains(&at)
        };
        let mut found = Vec::new();
        // A name of the function a block is in may not be declared again in the block
        // (E107); a nested function has its own names.
        for (id, declaration) in out.declarations.iter().enumerate() {
            let scope = &out.scopes[declaration.scope];
            let is_first = scope.names.get(&declaration.name) == Some(&id);
            let around = scope.parent.filter(|_| is_first);
            let outer = around.and_then(|parent| out.lookup(parent, &declaration.name));
            let outer = outer.map(|outer| &out.declarations[outer]);
            if outer.is_some_and(|outer| runs_in[outer.scope] == runs_in[declaration.scope]) {
                let line = outer.map_or(0, |outer| outer.span.start.line);
                let message = format!(
                    "`{}` shadows the binding declared on line {line}: only a nested function may reuse an outer name (SPEC.md §4.2)",
                    declaration.name
                );
                found.push((Code::E107, declaration.span, message));
            }
        }
        for reference in out.references.iter().filter(|reference| !reference.export) {
            let Some(declaration) = reference.declaration.map(|at| &out.declarations[at]) else {
                continue;
            };
            let at = reference.span.byte_start;
            if runs_in[reference.scope] == runs_in[declaration.scope] && early(declaration, at) {
                let message = format!(
                    "`{}` is used before its declaration on line {} has run (SPEC.md §8.1)",
                    reference.name, declaration.span.start.line
                );
                found.push((Code::E135, reference.span, message));
            }
            // A call runs the function now: what the function reads, and what the
            // functions it calls read, must be declared by now too.
            let called = declaration.function.and_then(|at| self.functions.get(&at));
            let Some(&called) = called.filter(|_| reference.call) else {
                continue;
            };
            let mut reached = BTreeSet::new();
            let mut pending = vec![called];
            while let Some(function) = pending.pop() {
                if reached.insert(function) {
                    let calls = inside[function].iter().map(|&at| &out.references[at]);
                    pending.extend(calls.filter(|inner| inner.call).filter_map(|inner| {
                        let target = &out.declarations[inner.declaration?];
                        self.functions.get(&target.function?).copied()
                    }));
                }
            }
            for inner in reached.iter().flat_map(|&function| &inside[function]) {
                let inner = &out.references[*inner];
                let Some(read) = inner.declaration.map(|at| &out.declarations[at]) else {
                    continue;
                };
                // A binding of a function that runs is made anew by the call.
                let fresh = reached
                    .iter()
                    .any(|&function| out.within(read.scope, function));
                if !fresh && runs_in[read.scope] == runs_in[reference.scope] && early(read, at) {
                    let message = format!(
                        "`{}` is called before `{}` is declared on line {}: line {} uses it (SPEC.md §8.1)",
                        reference.name, read.name, read.span.start.line, inner.span.start.line
                    );
                    found.push((Code::E135, reference.span, message));
                }
            }
        }
        let path = self.source.path();
        self.diagnostics.extend(
            found
                .into_iter()
                .map(|(code, span, message)| Diagnostic::new(code, path, span, message)),
        );
    }
}
