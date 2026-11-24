//! Exhaustiveness (SPEC.md §3.5, §4.3). A `switch` on a value that is one of a list
//! (a union of literals, a boolean, a discriminant) names every member or has a
//! `default`: what it leaves out is E103, and so is a `switch` without a `default`
//! on a value no list holds. A `default` that nothing is left for is E117, and a
//! `case` that the value can never be is E105. A chain of `if`/`else if` whose
//! branches all leave, and that nothing follows, is a `switch` written out: it is
//! judged the same. Where nothing is left of a name, a use of it is never reached
//! (E117).

use crate::expressions::{element_span_of, nodes, Typer};
use crate::narrowing::{Check, Path};
use sjs_bind::running;
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, TokenKind};
use sjs_types::{Type, TypeId};

/// A `case` of a `switch`: where its value is written, the literal of it, and
/// whether it has a minus before it.
struct Case<'t> {
    at: Span,
    literal: Element<'t>,
    minus: bool,
}

/// The labels of a `switch`: each `case` with a value, and the `default`.
fn labels(switch: NodeRef<'_>) -> (Vec<Case<'_>>, Option<NodeRef<'_>>) {
    let clauses = nodes(switch).filter(|part| part.kind() == NodeKind::CaseClause);
    let labels = clauses.flat_map(nodes);
    let mut cases = Vec::new();
    let mut default = None;
    for label in labels.filter(|part| part.kind() == NodeKind::CaseLabel) {
        // `case`, the value, `:`: a literal, with its sign.
        let written = label.tokens();
        let value = written.get(1..written.len().saturating_sub(1));
        let value = value.unwrap_or_default();
        let parts: Vec<Element<'_>> = label.children().collect();
        let literal = parts.iter().rev().nth(1).copied();
        match value.first().zip(value.last()).zip(literal) {
            Some(((first, last), literal)) => cases.push(Case {
                at: Span::new(
                    first.span.start,
                    last.span.end,
                    first.span.byte_start,
                    last.span.byte_end,
                ),
                literal,
                minus: first.kind == TokenKind::Punct(Punct::Minus),
            }),
            None => default = Some(label),
        }
    }
    (cases, default)
}

/// The branches of a chain of `if`/`else if`: each test with its block, and whether
/// the chain ends in an `else`.
fn branches(head: NodeRef<'_>) -> (Vec<(Element<'_>, NodeRef<'_>)>, bool) {
    let mut found = Vec::new();
    let mut at = Some(head);
    let mut closed = false;
    while let Some(statement) = at.take() {
        let test = statement.children().nth(2);
        let mut blocks = nodes(statement).filter(|part| part.kind() == NodeKind::Block);
        found.extend(test.zip(blocks.next()));
        // What follows `else`: another `if`, or the block that closes the chain.
        closed = blocks.next().is_some();
        at = nodes(statement).find(|part| part.kind() == NodeKind::IfStmt);
    }
    (found, closed)
}

impl Typer<'_> {
    /// Whether a type is one of a list: each member one value, or a boolean.
    fn is_listed(&self, ty: TypeId) -> bool {
        self.members_of(ty).iter().all(|&member| {
            let unit = matches!(
                self.types.get(member),
                Type::Literal(_) | Type::Undefined | Type::Null
            );
            unit || member == TypeId::BOOLEAN
        })
    }

    /// What a `switch` is of, and what its `case`s leave of that; `None` while what
    /// it is of has no type.
    fn switch_left(&mut self, switch: NodeRef<'_>, cases: &[Case<'_>]) -> Option<(TypeId, TypeId)> {
        // `switch`, `(`, the subject.
        let subject = switch.children().nth(2);
        let of = subject.and_then(|subject| self.type_of(subject))?;
        let mut left = of;
        for case in cases {
            left = self.apply(left, &Check::Equals(case.literal, case.minus), false);
        }
        Some((of, left))
    }

    /// Whether the `case`s of a `switch` leave nothing of what it is of.
    pub(crate) fn switch_exhausts(&mut self, switch: NodeRef<'_>) -> bool {
        let (cases, _) = labels(switch);
        let left = self.switch_left(switch, &cases);
        left.is_some_and(|(_, left)| left == TypeId::NEVER)
    }

    /// Judges a `switch`: E105 for a `case` its subject is never, E103 for what no
    /// `case` names when there is no `default`, E117 for a `default` nothing is
    /// left for.
    pub(crate) fn switch_rule(&mut self, node: NodeRef<'_>) {
        if node.kind() != NodeKind::SwitchStmt {
            return;
        }
        let (cases, default) = labels(node);
        let Some((of, left)) = self.switch_left(node, &cases) else {
            return;
        };
        // What is never reached is judged for that.
        if of == TypeId::NEVER {
            return;
        }
        for case in &cases {
            let value = self.compared(case.literal, case.minus);
            if let Some(value) = value.filter(|&value| !self.overlap(of, value)) {
                let message = format!(
                    "a `{}` is never `{}`: this `case` is never taken (SPEC.md §4.1)",
                    self.print_type(of),
                    self.print_type(value)
                );
                self.report(true, Code::E105, case.at, message);
            }
        }
        let listed = self.is_listed(of);
        let subject = node.children().nth(2);
        let at = subject.as_ref().map_or(node.span(), element_span_of);
        match default {
            Some(default) if listed && left == TypeId::NEVER => {
                let message = "this `default` is never reached: every value is named by a `case` (SPEC.md §3.5)";
                self.report(true, Code::E117, default.span(), message.to_owned());
            }
            None if listed && left != TypeId::NEVER => {
                let message = format!(
                    "this `switch` names no `case` for `{}`: name each, or add a `default` (SPEC.md §3.5)",
                    self.print_type(left)
                );
                self.report(true, Code::E103, at, message);
            }
            None if !listed => {
                let message = format!(
                    "a `{}` has more values than its `case`s name: add a `default` (SPEC.md §4.3)",
                    self.print_type(of)
                );
                self.report(true, Code::E103, at, message);
            }
            _ => {}
        }
    }

    /// The path every test of a chain is of, and what the tests leave of it when
    /// none holds: for a chain of two branches or more that all leave, without an
    /// `else`.
    fn chain_left(&mut self, head: NodeRef<'_>) -> Option<(Path, TypeId)> {
        let (found, closed) = branches(head);
        let mut all_leave = true;
        for (_, block) in &found {
            all_leave = all_leave && self.ends(*block);
        }
        let (first, _) = found
            .first()
            .filter(|_| found.len() > 1 && all_leave && !closed)?;
        // Each path a test inside the first test is of: the longest first, which
        // says the most (`s.kind` before `s`).
        let span = element_span_of(first);
        let tests = self
            .narrowing
            .tests
            .range((span.byte_start, 0)..(span.byte_end, 0));
        let mut paths: Vec<Path> = tests
            .flat_map(|(_, tests)| tests.iter().map(|test| test.path.clone()))
            .collect();
        paths.sort_by_key(|path| std::cmp::Reverse(path.1.len()));
        for path in paths {
            let written = self.written(*first, &path);
            let Some(whole) = written.and_then(|written| self.type_of(written)) else {
                continue;
            };
            // The chain is of this path when every test tells something of it.
            let mut left = whole;
            let mut tells = true;
            for (test, _) in &found {
                let passed = self.told(*test, &path, left, true);
                let failed = self.told(*test, &path, left, false);
                tells = tells && (passed != left || failed != left);
                left = failed;
            }
            if tells {
                return Some((path, left));
            }
        }
        None
    }

    /// Where a path is written in an expression: the first place.
    fn written<'t>(&self, within: Element<'t>, path: &Path) -> Option<Element<'t>> {
        let mut places: Vec<Element<'t>> = vec![within];
        let inside = match within {
            Element::Node(within) => running(within),
            Element::Token(_) => Vec::new(),
        };
        for node in inside {
            places.push(Element::Node(node));
            places.extend(
                node.children()
                    .filter(|part| matches!(part, Element::Token(_))),
            );
        }
        places.sort_by_key(|place| element_span_of(place).byte_start);
        places
            .into_iter()
            .find(|place| self.path_of(place).as_ref() == Some(path))
    }

    /// Whether a chain of `if`/`else if` leaves nothing of what it tests: then
    /// nothing runs past it.
    pub(crate) fn chain_exhausts(&mut self, head: NodeRef<'_>) -> bool {
        let left = self.chain_left(head);
        left.is_some_and(|(_, left)| left == TypeId::NEVER)
    }

    /// E103 for a chain of `if`/`else if` whose branches all leave, that nothing
    /// follows in its block, and that leaves values of what it tests unnamed.
    pub(crate) fn chain_rule(&mut self, node: NodeRef<'_>) {
        let parent = node.parent().filter(|_| node.kind() == NodeKind::IfStmt);
        let Some(parent) = parent.filter(|parent| parent.kind() != NodeKind::IfStmt) else {
            return;
        };
        // What follows the chain is what the values it leaves are for.
        let last = nodes(parent)
            .filter(|part| part.kind() != NodeKind::FunctionDecl)
            .last();
        if last.map(NodeRef::id) != Some(node.id()) {
            return;
        }
        let Some((_, left)) = self
            .chain_left(node)
            .filter(|(_, left)| *left != TypeId::NEVER)
        else {
            return;
        };
        let keyword = node.children().find(|part| {
            matches!(part, Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::If))
        });
        let at = keyword.as_ref().map_or(node.span(), element_span_of);
        let message = format!(
            "this chain has no branch for `{}`: add one, or an `else` (SPEC.md §3.5)",
            self.print_type(left)
        );
        self.report(true, Code::E103, at, message);
    }
}
