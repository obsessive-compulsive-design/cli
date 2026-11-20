//! Where `this` may be written (SPEC.md §4.4, §4.5): in a method, an accessor, the value
//! of a field and a `static` block, and in the arrows inside them. Anywhere else it is
//! E126, a parameter's default included; in the constructor of a class with `extends`
//! it is E000 before `super()` has been called, as the text reads.

use crate::scopes::{child, is_use};
use sjs_diag::{Code, Diagnostic, SourceFile};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, SyntaxTree, TokenKind};

const OUTSIDE: &str = "`this` is used outside a class: only a method, an accessor, a field's value and a `static` block have one, and the arrows in them (SPEC.md §4.4)";
const DEFAULT: &str =
    "`this` is used in a parameter's default, which may not refer to it (SPEC.md §4.4)";
const EARLY: &str = "`this` is used before `super()` has been called (SPEC.md §4.5)";

/// What `this` is where a node stands.
#[derive(Clone, Copy)]
struct Receiver {
    /// Why there is none, when there is none.
    refused: Option<&'static str>,
    /// In the constructor of a class with `extends`: where its first `super()` ends, or
    /// the end of everything when it calls none.
    ready: usize,
}

fn is_keyword(element: Option<&Element<'_>>, keyword: Keyword) -> bool {
    matches!(element, Some(Element::Token(token)) if token.token().kind == TokenKind::Keyword(keyword))
}

/// Where the first `super(…)` of a constructor ends: the first as the text reads, in
/// the arrows of the constructor too, and not in a class declared inside it.
fn first_super(constructor: NodeRef<'_>) -> Option<usize> {
    let mut first: Option<usize> = None;
    let mut pending = vec![constructor];
    while let Some(node) = pending.pop() {
        let children: Vec<Element<'_>> = node.children().collect();
        let is_call =
            node.kind() == NodeKind::CallExpr && is_keyword(children.first(), Keyword::Super);
        if is_call {
            let end = node.span().byte_end;
            first = Some(first.map_or(end, |first| first.min(end)));
        }
        pending.extend(children.iter().filter_map(|child| match child {
            Element::Node(inner) if inner.kind() != NodeKind::ClassBody => Some(*inner),
            _ => None,
        }));
    }
    first
}

/// Whether `method` is the constructor of a class with `extends`.
fn is_derived_constructor(source: &SourceFile, method: NodeRef<'_>) -> bool {
    let name = method.tokens().first().map(|token| token.span);
    let name = name.and_then(|span| source.slice(&span).map(str::to_owned));
    let class = method.parent().and_then(NodeRef::parent);
    name.as_deref() == Some("constructor")
        && class.is_some_and(|class| child(class, NodeKind::ClassHeritage).is_some())
}

/// E126 for each `this` where there is none, and E000 for each one a derived
/// constructor uses before its `super()`.
pub(crate) fn judge(source: &SourceFile, tree: &SyntaxTree) -> Vec<Diagnostic> {
    let mut found = Vec::new();
    let module = Receiver {
        refused: Some(OUTSIDE),
        ready: 0,
    };
    let mut pending = vec![(tree.root(), module)];
    while let Some((node, around)) = pending.pop() {
        let here = match node.kind() {
            NodeKind::FunctionDecl | NodeKind::FunctionExpr | NodeKind::ClassBody => module,
            NodeKind::Property if child(node, NodeKind::Params).is_some() => module,
            NodeKind::Params => Receiver {
                refused: Some(DEFAULT),
                ready: 0,
            },
            NodeKind::ClassMethod if is_derived_constructor(source, node) => Receiver {
                refused: None,
                ready: first_super(node).unwrap_or(usize::MAX),
            },
            NodeKind::ClassMethod
            | NodeKind::ClassAccessor
            | NodeKind::ClassField
            | NodeKind::StaticBlock => Receiver {
                refused: None,
                ready: 0,
            },
            _ => around,
        };
        let children: Vec<Element<'_>> = node.children().collect();
        for (at, element) in children.iter().enumerate() {
            match element {
                Element::Node(inner) => pending.push((*inner, here)),
                Element::Token(token) => {
                    let span = token.token().span;
                    let is_this = is_keyword(Some(element), Keyword::This);
                    if !(is_this && is_use(node, &children, at)) {
                        continue;
                    }
                    // Early when it begins before the constructor's `super()` has ended.
                    let early = (!(span.byte_start..here.ready).is_empty()).then_some(EARLY);
                    let wrong = here.refused.map(|why| (Code::E126, why));
                    let wrong = wrong.or(early.map(|why| (Code::E000, why)));
                    found.extend(
                        wrong.map(|(code, why)| Diagnostic::new(code, source.path(), span, why)),
                    );
                }
            }
        }
    }
    found.sort_by_key(|diagnostic| diagnostic.span.byte_start);
    found
}
