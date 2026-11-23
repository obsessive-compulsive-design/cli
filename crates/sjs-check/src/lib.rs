//! Control flow (SPEC.md §11.1): the graph of each function, and what it shows.
//!
//! - E117: code control never reaches, once for each stretch of it;
//! - E108: a `case` block control runs off the end of, into the next;
//! - E109: a `return` in a `finally` block;
//! - E114: a field without an initializer that the constructor leaves unassigned on
//!   some path;
//! - E000: a `this` a derived constructor reaches on some path that calls no
//!   `super()`, where the text has one before it (the binder judges the text).

pub use sjs_bind::{graphs, Cfg, Edge, Node, ENTRY, EXIT, RAISE};

use sjs_bind::{body_of, running};
use sjs_diag::{Code, Diagnostic, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};

const DEAD: &str = "this code is never run: control does not reach it (SPEC.md §8.1)";
const FALLS: &str = "this `case` runs on into the next: end its block with `break`, `return`, `throw` or `continue`, or stack the labels on one block (SPEC.md §4.3)";
const FINALLY: &str = "`return` in a `finally` block replaces what its `try` returned or threw: return after the statement (SPEC.md §4.3)";
const EARLY: &str = "`this` is used on a path where `super()` has not been called: call it on every path before (SPEC.md §4.5)";

fn nodes(node: NodeRef<'_>) -> impl Iterator<Item = NodeRef<'_>> {
    node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner),
        Element::Token(_) => None,
    })
}

fn range(span: &Span) -> (usize, usize) {
    (span.byte_start, span.byte_end)
}

fn is_punct(element: Option<&Element<'_>>, punct: Punct) -> bool {
    matches!(element, Some(Element::Token(token)) if token.token().kind == TokenKind::Punct(punct))
}

fn is_keyword(element: Option<&Element<'_>>, keyword: Keyword) -> bool {
    matches!(element, Some(Element::Token(token)) if token.token().kind == TokenKind::Keyword(keyword))
}

/// A node and every node inside it, as the text has them.
fn within(node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        let inner: Vec<NodeRef<'_>> = nodes(node).collect();
        pending.extend(inner.into_iter().rev());
        found.push(node);
    }
    found
}

/// The first word of a class member, when it begins with one.
fn first_word<'a>(source: &'a SourceFile, member: NodeRef<'_>) -> Option<&'a str> {
    let first = member.tokens().first().map(|token| token.span);
    first.and_then(|span| source.slice(&span))
}

struct Judge<'a> {
    source: &'a SourceFile,
    found: Vec<Diagnostic>,
}

impl Judge<'_> {
    fn report(&mut self, code: Code, span: Span, message: impl Into<String>) {
        let path = self.source.path();
        self.found.push(Diagnostic::new(code, path, span, message));
    }

    /// E117 for each statement of `node` control does not enter: the first of a
    /// stretch of them, for what follows it in its block is the same finding.
    fn dead(&mut self, graph: &Cfg, reached: &[bool], node: NodeRef<'_>) {
        let listed = matches!(node.kind(), NodeKind::Block | NodeKind::Module);
        // A function declaration is hoisted: where it stands, nothing runs.
        for inner in nodes(node).filter(|inner| inner.kind() != NodeKind::FunctionDecl) {
            let span = inner.span();
            let clause = matches!(
                inner.kind(),
                NodeKind::CaseClause | NodeKind::CatchClause | NodeKind::FinallyClause
            );
            match graph.entered(range(&span), reached) {
                Some(false) => {
                    self.report(Code::E117, span, DEAD);
                    if listed {
                        return;
                    }
                }
                entered => {
                    if entered.is_some() || clause {
                        self.dead(graph, reached, inner);
                    }
                }
            }
        }
    }

    /// E000 for each `this` of a derived constructor that the text has after a
    /// `super()`, and that control reaches by a path that calls none.
    fn receiver(&mut self, graph: &Cfg, constructor: NodeRef<'_>) {
        let parts = running(constructor);
        let calls: Vec<Span> = parts
            .iter()
            .filter(|part| {
                let callee = part.children().next();
                let is_super = is_keyword(callee.as_ref(), Keyword::Super);
                part.kind() == NodeKind::CallExpr && is_super
            })
            .map(|call| call.span())
            .collect();
        // Before the first is the binder's to judge: so is a constructor with none.
        let ready = calls.iter().map(|call| call.byte_end).min();
        let calls: Vec<(usize, Vec<usize>)> = calls
            .iter()
            .map(|call| (call.byte_end, graph.nodes_holding(range(call))))
            .collect();
        let calling: Vec<usize> = calls.iter().flat_map(|(_, nodes)| nodes.clone()).collect();
        let reached = graph.reachable_avoiding(&calling);
        for part in parts {
            let children: Vec<Element<'_>> = part.children().collect();
            for (at, child) in children.iter().enumerate() {
                let is_this = is_keyword(Some(child), Keyword::This);
                let before = at.checked_sub(1).and_then(|before| children.get(before));
                // `a.this` and `{ this: a }` name a property.
                let named = is_punct(before, Punct::Dot)
                    || is_punct(before, Punct::QuestionDot)
                    || is_punct(children.get(at + 1), Punct::Colon);
                let Element::Token(token) = child else {
                    continue;
                };
                let span = token.token().span;
                let late = ready.is_some_and(|ready| span.byte_start >= ready);
                // Where it is evaluated, no `super()` has been called on some path:
                // none on the way there, and none there before it.
                let uncalled = graph.nodes_holding(range(&span)).iter().any(|node| {
                    let called = calls
                        .iter()
                        .any(|(end, nodes)| nodes.contains(node) && *end <= span.byte_start);
                    reached[*node] && !called
                });
                if is_this && !named && late && uncalled {
                    self.report(Code::E000, span, EARLY);
                }
            }
        }
    }

    /// E114 for each field of a class body that has no initializer, and that some
    /// path through the constructor leaves unassigned.
    fn fields(&mut self, body: NodeRef<'_>) {
        let source = self.source;
        let members: Vec<NodeRef<'_>> = nodes(body).collect();
        let constructor = members.iter().copied().find(|member| {
            member.kind() == NodeKind::ClassMethod
                && first_word(source, *member) == Some("constructor")
        });
        let block = constructor.and_then(body_of);
        let graph = block.map(|block| Cfg::of_body(source, block));
        // Each `this.key = …` the constructor runs: the key, and where it is.
        let assigned: Vec<(&str, Span)> = constructor
            .map(running)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|node| {
                // `this`, `.`, the key, then the `=` of the assignment itself.
                let words = node.tokens();
                let kinds: Vec<&TokenKind> = words.iter().take(4).map(|word| &word.kind).collect();
                let assigns = kinds
                    == [
                        &TokenKind::Keyword(Keyword::This),
                        &TokenKind::Punct(Punct::Dot),
                        kinds.get(2).copied().unwrap_or(&TokenKind::Eof),
                        &TokenKind::Punct(Punct::Eq),
                    ];
                let key = words.get(2).filter(|_| assigns);
                let key = key.and_then(|key| source.slice(&key.span));
                key.map(|key| (key, node.span()))
            })
            .collect();
        for member in members {
            // The name is the last word before the value: `static` may come first.
            let words: Vec<Span> = member
                .children()
                .map_while(|child| match child {
                    Element::Token(token) => Some(token.token()),
                    Element::Node(_) => None,
                })
                .filter(|token| token.kind != TokenKind::Punct(Punct::Semicolon))
                .map(|token| token.span)
                .collect();
            // A key that is computed is a node: `[Symbol.iterator]`.
            let symbol = nodes(member).next();
            let is_static = words.len() > 1 && first_word(source, member) == Some("static");
            let valued = nodes(member).any(|part| part.kind() == NodeKind::Initializer);
            let at = symbol.map(NodeRef::span).or(words.last().copied());
            let Some(at) =
                at.filter(|_| member.kind() == NodeKind::ClassField && !is_static && !valued)
            else {
                continue;
            };
            let name = source.slice(&at).unwrap_or_default();
            // Whether the constructor can end with no assignment to the field run.
            let unassigned = graph.as_ref().is_none_or(|graph| {
                let assigning: Vec<usize> = assigned
                    .iter()
                    .filter(|(key, _)| *key == name)
                    .flat_map(|(_, span)| graph.nodes_holding(range(span)))
                    .collect();
                graph.reachable_avoiding(&assigning)[EXIT]
            });
            if unassigned {
                let message = format!(
                    "the field `{name}` has no initializer, and the constructor does not assign it on every path: give it a value, `undefined` if it has none yet (SPEC.md §4.5)"
                );
                self.report(Code::E114, at, message);
            }
        }
    }
}

/// What the flow of control of a file shows: E117, E108, E109, E114 and E000, in
/// the order of the text.
#[must_use]
pub fn check_flow(source: &SourceFile, tree: &SyntaxTree) -> Vec<Diagnostic> {
    let mut judge = Judge {
        source,
        found: Vec::new(),
    };
    for node in within(tree.root()) {
        if let Some(body) = body_of(node) {
            let graph = Cfg::of_body(source, body);
            let reached = graph.reachable();
            judge.dead(&graph, &reached, node);
            let mut falls: Vec<Span> = graph
                .falls
                .iter()
                .filter(|(_, from)| from.iter().any(|from| reached[*from]))
                .map(|(label, _)| *label)
                .collect();
            falls.dedup_by_key(|label| label.byte_start);
            for label in falls {
                judge.report(Code::E108, label, FALLS);
            }
            // Only the constructor of a class with `extends` calls `super()`.
            if node.kind() == NodeKind::ClassMethod {
                judge.receiver(&graph, node);
            }
        }
        if node.kind() == NodeKind::ClassBody {
            judge.fields(node);
        }
        // A `return` of the function a `finally` is in, in that `finally`.
        let around = std::iter::successors(node.parent(), |outer| outer.parent());
        let mut around = around.take_while(|outer| body_of(*outer).is_none());
        let in_finally = around.any(|outer| outer.kind() == NodeKind::FinallyClause);
        if node.kind() == NodeKind::ReturnStmt && in_finally {
            judge.report(Code::E109, node.span(), FINALLY);
        }
    }
    judge.found.sort_by_key(|found| found.span.byte_start);
    judge.found
}
