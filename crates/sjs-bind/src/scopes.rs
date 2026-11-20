//! Scopes, declarations and references of one file (SPEC.md §4.2, §4.4), with the
//! findings that need nothing else: a name declared twice in one scope (E000), a block
//! that declares a name of the function it is in (E107) and a binding used before its
//! declaration has run (E135, the temporal dead zone); and those about what is assigned:
//! a `let` nothing assigns (E106), a loop variable its loop's body assigns (E123), a
//! binding that may not be assigned at all (E138).

use crate::ambient::{Table, Use};
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
    /// Where the last node inside it begins: for a loop, its body.
    body: usize,
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
    /// Whether it is exported where it is declared (`export const …`).
    pub exported: bool,
    /// What it is given its value by, where a use of it is no use by anything else:
    /// the initializer of a `const` or `let`, the whole of a function or a class.
    own: Range<usize>,
    /// The `let` that declares it, when one does.
    keyword: Option<Span>,
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
    /// Whether it is written to and not read: the target of an assignment (compound
    /// ones included) or of `++`/`--`.
    pub write: bool,
    /// The key it is read by, `name.key` or `name[…]`: the key's text when it is a
    /// name or a string literal, and where the key is.
    pub member: Option<(Option<String>, Span)>,
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
    binder
        .diagnostics
        .extend(crate::receiver::judge(source, tree));
    (binder.out, binder.diagnostics)
}

/// What the names of a pattern are declared as.
#[derive(Clone)]
struct Declaring {
    kind: DeclKind,
    origin: usize,
    dead: Range<usize>,
    exported: bool,
    keyword: Option<Span>,
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
pub(crate) fn child(node: NodeRef<'_>, kind: NodeKind) -> Option<NodeRef<'_>> {
    node.children().find_map(|child| match child {
        Element::Node(inner) if inner.kind() == kind => Some(inner),
        _ => None,
    })
}

/// Where the last node directly inside `node` begins: the body, for a loop.
fn last_node(node: NodeRef<'_>) -> usize {
    let nodes = node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner.span().byte_start),
        Element::Token(_) => None,
    });
    nodes.last().unwrap_or_default()
}

/// Whether a class field is `static`: only then do two words come before its value,
/// the word and the field's name.
pub(crate) fn is_static(field: NodeRef<'_>) -> bool {
    let mut words = field
        .children()
        .take_while(|child| !is_node(Some(child), NodeKind::Initializer));
    words
        .nth(1)
        .is_some_and(|name| !is_punct(Some(&name), Punct::Semicolon))
}

/// Whether the use of a name at `children[at]` of `node` writes to it: the target of an
/// assignment or of an update, a name in the pattern an assignment fills.
fn is_write(node: NodeRef<'_>, children: &[Element<'_>], at: usize) -> bool {
    let before = at.checked_sub(1).and_then(|before| children.get(before));
    match node.kind() {
        NodeKind::AssignExpr | NodeKind::PatternElement => at == 0,
        NodeKind::ForUpdate => !children[..at].iter().any(is_ident),
        NodeKind::UpdateExpr | NodeKind::ArrayPattern | NodeKind::RestElement => true,
        NodeKind::PatternProperty => at == 0 || is_punct(before, Punct::Colon),
        // `(x) = 1` assigns as `x = 1` does.
        NodeKind::ParenExpr => node.parent().is_some_and(|parent| {
            let around: Vec<Element<'_>> = parent.children().collect();
            let here = around.iter().position(
                |child| matches!(child, Element::Node(inner) if inner.span() == node.span()),
            );
            here.is_some_and(|here| is_write(parent, &around, here))
        }),
        _ => false,
    }
}

/// Whether the identifier at `children[at]` of `node`, which declares nothing, is a use
/// of a name, and not a key, a property name or a contextual word.
pub(crate) fn is_use(node: NodeRef<'_>, children: &[Element<'_>], at: usize) -> bool {
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

/// The findings about the names themselves: E107, E115 and E153. `runs_in` is the
/// function each scope runs as part of.
fn judge_names(out: &Bindings, runs_in: &[usize], found: &mut Vec<(Code, Span, String)>) {
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
    // A binding nothing reads is E115, unless its name begins with `_`; one that
    // begins with `_` and is read is E153. A write is no read, nor is a use inside
    // what gives the binding its own value; an export is one.
    let mut reads: Vec<Option<u32>> = vec![None; out.declarations.len()];
    for reference in out
        .references
        .iter()
        .rev()
        .filter(|reference| !reference.write)
    {
        if let Some(at) = reference.declaration {
            if !out.declarations[at]
                .own
                .contains(&reference.span.byte_start)
            {
                reads[at] = Some(reference.span.start.line);
            }
        }
    }
    for (id, declaration) in out.declarations.iter().enumerate() {
        let name = &declaration.name;
        let is_first = out.scopes[declaration.scope].names.get(name) == Some(&id);
        let kept = name.starts_with('_');
        match reads[id] {
            None if is_first && !kept && !declaration.exported => {
                let message = format!(
                    "`{name}` is never read: remove it, or name it `_{name}` to keep it (SPEC.md §8.1)"
                );
                found.push((Code::E115, declaration.span, message));
            }
            Some(line) if kept => {
                let message = format!(
                    "`{name}` is read on line {line}: a name that begins with `_` is one nothing reads (SPEC.md §8.3)"
                );
                found.push((Code::E153, declaration.span, message));
            }
            _ => {}
        }
    }
}

/// The findings about what is assigned: E123 and E138 onto `found`, and the E106s,
/// which say what to write instead.
fn judge_writes(
    out: &Bindings,
    path: &str,
    found: &mut Vec<(Code, Span, String)>,
) -> Vec<Diagnostic> {
    let mut written = vec![false; out.declarations.len()];
    // The declarations of which some name is assigned.
    let mut assigned = BTreeSet::new();
    for reference in out.references.iter().filter(|reference| reference.write) {
        let Some(at) = reference.declaration else {
            continue;
        };
        let declaration = &out.declarations[at];
        let scope = &out.scopes[declaration.scope];
        let (name, line) = (&declaration.name, declaration.span.start.line);
        written[at] = true;
        assigned.insert(declaration.origin);
        let what = match declaration.kind {
            DeclKind::Const => "a constant",
            DeclKind::Import => "an import",
            DeclKind::Function => "a function",
            DeclKind::Class => "a class",
            DeclKind::Let | DeclKind::Param | DeclKind::Catch => {
                let is_loop = matches!(scope.kind, ScopeKind::For | ScopeKind::ForOf);
                if is_loop && reference.span.byte_start >= scope.body {
                    let message = format!(
                        "`{name}` is the variable of the loop on line {line}: only the loop's header may change it (SPEC.md §8.1)"
                    );
                    found.push((Code::E123, reference.span, message));
                }
                continue;
            }
        };
        let message = format!(
            "`{name}` cannot be assigned: it is {what}, declared on line {line} (SPEC.md §8.1)"
        );
        found.push((Code::E138, reference.span, message));
    }
    // A `let` nothing assigns is a `const` (E106), reported once for the declaration;
    // where another name of the declaration is assigned, on the name that is not.
    let mut told = BTreeSet::new();
    let mut unused = Vec::new();
    for (id, declaration) in out.declarations.iter().enumerate() {
        let name = &declaration.name;
        let is_first = out.scopes[declaration.scope].names.get(name) == Some(&id);
        let Some(keyword) = declaration.keyword.filter(|_| is_first && !written[id]) else {
            continue;
        };
        if assigned.contains(&declaration.origin) {
            let message = format!(
                "`{name}` is never reassigned, unlike another name of its `let`: declare it on its own, with `const` (SPEC.md §4.2)"
            );
            unused.push(Diagnostic::new(Code::E106, path, declaration.span, message));
        } else if told.insert(declaration.origin) {
            let message =
                format!("`{name}` is never reassigned: declare it with `const` (SPEC.md §4.2)");
            unused.extend(
                Diagnostic::new(Code::E106, path, keyword, message).with_fix("let", "const"),
            );
        }
    }
    unused
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
                    body: last_node(node),
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
                    exported: is_node(
                        node.parent().map(Element::Node).as_ref(),
                        NodeKind::ExportDecl,
                    ),
                    origin: span.byte_start,
                    dead: own.map_or(span.byte_end..iterable, |own| own.byte_start..own.byte_end),
                    keyword: node.children().find_map(|child| match child {
                        Element::Token(token) if kind == NodeKind::LetDecl => {
                            Some(token.token().span)
                        }
                        _ => None,
                    }),
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
                    exported: false,
                    keyword: None,
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
            exported: is_node(
                node.parent().map(Element::Node).as_ref(),
                NodeKind::ExportDecl,
            ),
            own: if function.is_some() || kind == DeclKind::Class {
                span.byte_start..span.byte_end
            } else {
                0..0
            },
            keyword: None,
        };
        let declared = match kind {
            NodeKind::PatternProperty if is_key => return,
            _ if own.is_some() => own.map(|own| Declaration {
                dead: own.dead.clone(),
                own: own.dead.clone(),
                exported: own.exported,
                origin: own.origin,
                keyword: own.keyword,
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
                write: is_write(node, children, at),
                member: self.member(node, children).filter(|_| at == 0),
            });
        }
    }

    /// The key the first child of `node` is read by, when `node` reads one by a key.
    fn member(
        &self,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<(Option<String>, Span)> {
        match (node.kind(), children.last(), children.iter().rev().nth(1)) {
            (NodeKind::MemberExpr, Some(Element::Token(name)), _) => {
                let span = name.token().span;
                let text = self.source.slice(&span).unwrap_or_default();
                Some((Some(text.to_owned()), span))
            }
            (NodeKind::IndexExpr, _, Some(Element::Token(key))) => {
                let name = match &key.token().kind {
                    TokenKind::String(value) => Some(value.to_string()),
                    _ => None,
                };
                Some((name, key.token().span))
            }
            (NodeKind::IndexExpr, _, Some(Element::Node(key))) => Some((None, key.span())),
            _ => None,
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
        judge_names(out, &runs_in, &mut found);
        self.diagnostics
            .extend(judge_writes(out, self.source.path(), &mut found));
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
        // A name the file does not declare is a global, or nothing (E204, E133, E138).
        let table = Table::shipped();
        let test_file = self.source.path().ends_with(".test.js");
        for reference in out.references.iter().filter(|r| r.declaration.is_none()) {
            let used = Use {
                name: &reference.name,
                keyed: reference.member.is_some(),
                member: reference
                    .member
                    .as_ref()
                    .and_then(|(key, _)| key.as_deref()),
                write: reference.write,
                test_file,
            };
            if let Some(verdict) = table.judge(&used) {
                let key = reference.member.as_ref().filter(|_| verdict.on_member);
                let span = key.map_or(reference.span, |(_, span)| *span);
                found.push((verdict.code, span, verdict.message));
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
