//! The control-flow graph of one function (SPEC.md §11.1). A node is a statement or
//! an expression that is evaluated whole; an edge says how control goes from one to
//! the next: plainly, or by the answer of a condition (`true`, `false`), a `??`
//! (`nullish`, `present`), a `?.` (`absent`, `present`), a loop that is done
//! (`done`), a jump (`break`, `continue`, `return`, `throw`), or what a node of a
//! `try` may throw (`throws`). A `finally` block is built once for each way its `try`
//! is left, so that what follows each way is told apart.

use sjs_diag::{SourceFile, Span};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// An edge that waits for the node it goes to: where it comes from, and its label.
pub(crate) type Pending = (usize, &'static str);

/// One node of a graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// What it is, as the text has it, cut short.
    pub label: String,
    /// The bytes of the text it stands for; none for the entry and the exits.
    pub range: Option<(usize, usize)>,
}

/// One edge of a graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// The node it comes from.
    pub from: usize,
    /// The node it goes to.
    pub to: usize,
    /// How control goes: empty for the next thing in order.
    pub label: &'static str,
}

/// The control-flow graph of a function, or of a module's own statements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cfg {
    /// The nodes: the entry first, then the exit by `return` or by running off the
    /// end, then the exit by `throw`.
    pub nodes: Vec<Node>,
    /// The edges, in the order they were made.
    pub edges: Vec<Edge>,
    /// Each statement and block by its bytes, with the nodes it is entered from,
    /// every time it was built.
    pub(crate) statements: BTreeMap<(usize, usize), Vec<usize>>,
    /// Each `case` block that is not the last of its `switch`: where its label is,
    /// and the nodes control runs off its end from.
    pub falls: Vec<(Span, Vec<usize>)>,
}

/// The entry of every graph.
pub const ENTRY: usize = 0;
/// The exit by `return`, or by running off the end.
pub const EXIT: usize = 1;
/// The exit by a `throw` nothing catches.
pub const RAISE: usize = 2;

/// A loop or a `switch` a `break` or a `continue` may leave.
struct Target {
    label: Option<String>,
    /// Whether `continue` binds to it: a loop, not a `switch`.
    is_loop: bool,
    breaks: Vec<Pending>,
    continues: Vec<Pending>,
    /// How many `try` statements were open when it began.
    tries: usize,
}

/// A `try` statement that is open.
struct Try<'t> {
    /// Whether what is thrown now is caught by it: in its block, and it has a
    /// `catch`.
    catches: bool,
    /// What its `catch` is entered from.
    thrown: Vec<Pending>,
    /// Its `finally` block.
    finally: Option<NodeRef<'t>>,
    /// How many loops and switches were open when it began.
    targets: usize,
}

/// Where a jump goes.
#[derive(Clone, Copy)]
enum Jump {
    Return,
    Throw,
    /// Out of the target at this place.
    Break(usize),
    /// To the next round of the target at this place.
    Continue(usize),
}

struct Builder<'a, 't> {
    source: &'a SourceFile,
    graph: Cfg,
    targets: Vec<Target>,
    tries: Vec<Try<'t>>,
    /// What each open `?.` group is left by when a receiver is absent.
    absent: Vec<Vec<Pending>>,
    /// The label a `LabelledStmt` gives the statement being built.
    label: Option<String>,
}

fn nodes_of(node: NodeRef<'_>) -> impl Iterator<Item = NodeRef<'_>> {
    node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner),
        Element::Token(_) => None,
    })
}

fn punct_of(element: Option<&Element<'_>>) -> Option<Punct> {
    match element {
        Some(Element::Token(token)) => match token.token().kind {
            TokenKind::Punct(punct) => Some(punct),
            _ => None,
        },
        _ => None,
    }
}

fn span_of(element: &Element<'_>) -> Span {
    match element {
        Element::Node(node) => node.span(),
        Element::Token(token) => token.token().span,
    }
}

/// Whether a node is a function or a class: what is inside is no part of the flow
/// around it.
#[must_use]
pub fn is_scope(node: NodeRef<'_>) -> bool {
    let is_method = || nodes_of(node).any(|part| part.kind() == NodeKind::Params);
    match node.kind() {
        NodeKind::FunctionDecl
        | NodeKind::FunctionExpr
        | NodeKind::ArrowFunction
        | NodeKind::ClassDecl
        | NodeKind::ClassExpr => true,
        NodeKind::Property => is_method(),
        _ => false,
    }
}

/// The operator of a binary expression that evaluates its right side only
/// sometimes.
fn short_circuit(node: NodeRef<'_>) -> Option<Punct> {
    let parts: Vec<Element<'_>> = node.children().collect();
    let operator = punct_of(parts.get(1));
    let short = [Punct::AmpAmp, Punct::PipePipe, Punct::QuestionQuestion];
    operator.filter(|operator| node.kind() == NodeKind::BinaryExpr && short.contains(operator))
}

/// Whether evaluating an expression branches or waits: it holds `&&`, `||`, `??`,
/// `?:`, a `?.` group, `await` or `yield`, outside the functions written in it.
fn has_flow(element: &Element<'_>) -> bool {
    let Element::Node(node) = element else {
        return false;
    };
    let mut pending: Vec<NodeRef<'_>> = Some(*node)
        .into_iter()
        .filter(|node| !is_scope(*node))
        .collect();
    while let Some(node) = pending.pop() {
        let flows = matches!(
            node.kind(),
            NodeKind::CondExpr | NodeKind::AwaitExpr | NodeKind::YieldExpr
        );
        let optional = node
            .children()
            .any(|part| punct_of(Some(&part)) == Some(Punct::QuestionDot));
        if flows || optional || short_circuit(node).is_some() {
            return true;
        }
        pending.extend(nodes_of(node).filter(|inner| !is_scope(*inner)));
    }
    false
}

impl<'t> Builder<'_, 't> {
    fn text(&self, span: &Span) -> String {
        let text = self.source.slice(span).unwrap_or_default();
        let line: String = text.split_whitespace().collect::<Vec<&str>>().join(" ");
        let short: String = line.chars().take(40).collect();
        if short.len() < line.len() {
            format!("{short}…")
        } else {
            short
        }
    }

    /// Adds a node for the text at `span`, entered by `ends`: one that evaluates
    /// nothing, and so throws nothing by itself.
    fn jumping(&mut self, span: Span, ends: Vec<Pending>) -> usize {
        let at = self.graph.nodes.len();
        self.graph.nodes.push(Node {
            label: self.text(&span),
            range: Some((span.byte_start, span.byte_end)),
        });
        self.join(ends, at);
        at
    }

    /// Adds a node for the text at `span`, entered by `ends`. Inside a `try`, the
    /// node may throw.
    fn node(&mut self, label: String, span: Span, ends: Vec<Pending>) -> usize {
        let at = self.jumping(span, ends);
        self.graph.nodes[at].label = label;
        // The innermost open `try`, when there is one.
        for open in self.tries.iter_mut().rev().take(1) {
            open.thrown.push((at, "throws"));
        }
        at
    }

    /// Ends every pending edge at `to`.
    fn join(&mut self, ends: Vec<Pending>, to: usize) {
        for (from, label) in ends {
            self.graph.edges.push(Edge { from, to, label });
        }
    }

    /// A node for an expression evaluated whole.
    fn leaf(&mut self, expression: &Element<'t>, ends: Vec<Pending>) -> usize {
        let span = span_of(expression);
        self.node(self.text(&span), span, ends)
    }

    /// The one node a branch is taken from: the node `ends` come from when that is
    /// one node plainly left, else a node of its own for the text at `span`.
    fn branch(&mut self, ends: Vec<Pending>, span: Span) -> usize {
        match ends.as_slice() {
            [(from, "")] => *from,
            _ => self.node(self.text(&span), span, ends),
        }
    }

    /// Builds an expression for its value: the edges it is left by.
    fn value(&mut self, expression: Element<'t>, ends: Vec<Pending>) -> Vec<Pending> {
        let node = match expression {
            Element::Node(node) if has_flow(&expression) => node,
            _ => return vec![(self.leaf(&expression, ends), "")],
        };
        let parts: Vec<Element<'t>> = node.children().collect();
        match (node.kind(), short_circuit(node)) {
            (NodeKind::ParenExpr | NodeKind::Initializer, _) => self.value(parts[1], ends),
            (_, Some(Punct::QuestionQuestion)) => {
                let left = self.value(parts[0], ends);
                let test = self.branch(left, span_of(&parts[0]));
                let mut left_by = self.value(parts[2], vec![(test, "nullish")]);
                left_by.push((test, "present"));
                left_by
            }
            (_, Some(operator)) => {
                let (held, failed) = self.condition(parts[0], ends);
                // `&&` goes on when the left holds, `||` when it does not.
                let (on, mut over) = if operator == Punct::AmpAmp {
                    (held, failed)
                } else {
                    (failed, held)
                };
                over.extend(self.value(parts[2], on));
                over
            }
            (NodeKind::CondExpr, _) => {
                let (held, failed) = self.condition(parts[0], ends);
                let mut left_by = self.value(parts[2], held);
                left_by.extend(self.value(parts[4], failed));
                left_by
            }
            (NodeKind::OptionalChain, _) => {
                self.absent.push(Vec::new());
                let mut left_by = self.value(parts[0], ends);
                left_by.extend(self.absent.pop().unwrap_or_default());
                left_by
            }
            _ => {
                // What it is made of, in order, then the whole.
                let mut ends = ends;
                for (at, part) in parts.iter().enumerate() {
                    let is_node = matches!(part, Element::Node(inner) if !is_scope(*inner));
                    let optional = punct_of(parts.get(at + 1)) == Some(Punct::QuestionDot);
                    if !is_node && !optional {
                        continue;
                    }
                    ends = self.value(*part, ends);
                    // After `?.`, nothing more is evaluated when the receiver is absent.
                    if optional {
                        let test = self.branch(ends, span_of(part));
                        for group in self.absent.iter_mut().rev().take(1) {
                            group.push((test, "absent"));
                        }
                        ends = vec![(test, "present")];
                    }
                }
                let span = node.span();
                let label = match node.kind() {
                    NodeKind::AwaitExpr => "await".to_owned(),
                    NodeKind::YieldExpr => "yield".to_owned(),
                    _ => self.text(&span),
                };
                vec![(self.node(label, span, ends), "")]
            }
        }
    }

    /// Builds an expression as a condition: the edges it is left by when it holds,
    /// and when it does not. A condition written `true` or `false` is left one way.
    fn condition(
        &mut self,
        expression: Element<'t>,
        ends: Vec<Pending>,
    ) -> (Vec<Pending>, Vec<Pending>) {
        if let Element::Node(node) = expression {
            let parts: Vec<Element<'t>> = node.children().collect();
            let operator = short_circuit(node).filter(|op| *op != Punct::QuestionQuestion);
            let negates =
                node.kind() == NodeKind::UnaryExpr && punct_of(parts.first()) == Some(Punct::Bang);
            if node.kind() == NodeKind::ParenExpr {
                return self.condition(parts[1], ends);
            }
            if negates {
                let (held, failed) = self.condition(parts[1], ends);
                return (failed, held);
            }
            if let Some(operator) = operator {
                let (held, failed) = self.condition(parts[0], ends);
                return if operator == Punct::AmpAmp {
                    let (both, second) = self.condition(parts[2], held);
                    (both, [failed, second].concat())
                } else {
                    let (second, neither) = self.condition(parts[2], failed);
                    ([held, second].concat(), neither)
                };
            }
        }
        let written = match &expression {
            Element::Token(token) => match token.token().kind {
                TokenKind::Bool(value) => Some(value),
                _ => None,
            },
            Element::Node(_) => None,
        };
        let left = self.value(expression, ends);
        let test = self.branch(left, span_of(&expression));
        let held = (written != Some(false)).then_some((test, "true"));
        let failed = (written != Some(true)).then_some((test, "false"));
        (held.into_iter().collect(), failed.into_iter().collect())
    }

    /// Builds a `finally` block as it runs when its `try`, at `at` of the open ones,
    /// is left by `ends`.
    fn through_finally(&mut self, at: usize, ends: Vec<Pending>) -> Vec<Pending> {
        let block = self.tries[at].finally.filter(|_| !ends.is_empty());
        let Some(block) = block else {
            return ends;
        };
        // The block is outside its `try`, and outside what was opened in it.
        let inner_tries = self.tries.split_off(at);
        let inner_targets = self.targets.split_off(inner_tries[0].targets);
        let left_by = self.block(block, ends);
        self.targets.extend(inner_targets);
        self.tries.extend(inner_tries);
        left_by
    }

    /// Sends `ends` where a jump goes, through each `finally` on the way.
    fn jump(&mut self, mut ends: Vec<Pending>, jump: Jump) {
        // The `try` statements the jump leaves, the innermost first.
        let outside = match jump {
            Jump::Break(at) | Jump::Continue(at) => self.targets[at].tries,
            Jump::Return | Jump::Throw => 0,
        };
        for at in (outside..self.tries.len()).rev() {
            let throws = matches!(jump, Jump::Throw);
            if throws && self.tries[at].catches {
                self.tries[at].thrown.extend(ends);
                return;
            }
            ends = self.through_finally(at, ends);
        }
        match jump {
            Jump::Return => self.join(ends, EXIT),
            Jump::Throw => self.join(ends, RAISE),
            Jump::Break(at) => self.targets[at].breaks.extend(ends),
            Jump::Continue(at) => self.targets[at].continues.extend(ends),
        }
    }

    /// The target a `break` or a `continue` binds to: the one of its label, or the
    /// nearest, which for `continue` is the nearest loop.
    fn target(&self, statement: NodeRef<'t>, continues: bool) -> Option<usize> {
        let label = statement.tokens().get(1).map(|label| label.span);
        let label = label.and_then(|span| self.source.slice(&span));
        let targets = self.targets.iter().enumerate().rev();
        let mut fitting = targets.filter(|(_, target)| match label {
            Some(label) => target.label.as_deref() == Some(label),
            None => target.is_loop || !continues,
        });
        fitting.next().map(|(at, _)| at)
    }

    fn open(&mut self, is_loop: bool) {
        self.targets.push(Target {
            label: self.label.take(),
            is_loop,
            breaks: Vec::new(),
            continues: Vec::new(),
            tries: self.tries.len(),
        });
    }

    /// Closes the innermost target: what `continue` and `break` were sent to it.
    fn close(&mut self) -> (Vec<Pending>, Vec<Pending>) {
        let target = self.targets.pop();
        target.map_or((Vec::new(), Vec::new()), |target| {
            (target.continues, target.breaks)
        })
    }

    /// Notes where the statement or block at `span` is entered from.
    fn enter(&mut self, span: Span, ends: &[Pending]) {
        let range = (span.byte_start, span.byte_end);
        let from = self.graph.statements.entry(range).or_default();
        from.extend(ends.iter().map(|(from, _)| *from));
    }

    fn block(&mut self, block: NodeRef<'t>, mut ends: Vec<Pending>) -> Vec<Pending> {
        self.enter(block.span(), &ends);
        for statement in nodes_of(block) {
            ends = self.statement(statement, ends);
        }
        ends
    }

    /// Builds a loop: `enter` gives the node control comes back to and the edges
    /// into the body and past the loop.
    fn looped(
        &mut self,
        body: Option<NodeRef<'t>>,
        ends: Vec<Pending>,
        head: Option<Element<'t>>,
        update: Option<Element<'t>>,
        span: Span,
    ) -> Vec<Pending> {
        self.open(true);
        // The place control comes back to, round after round.
        let back = self.node("loop".to_owned(), span, ends);
        let (into, mut past) = match head {
            Some(head) => self.condition(head, vec![(back, "")]),
            None => (vec![(back, "")], Vec::new()),
        };
        let round = body.map_or(into.clone(), |body| self.block(body, into));
        let (continues, breaks) = self.close();
        let mut again = [round, continues].concat();
        if let Some(update) = update {
            again = self.value(update, again);
        }
        self.join(again, back);
        past.extend(breaks);
        past
    }

    #[allow(clippy::too_many_lines)]
    fn statement(&mut self, statement: NodeRef<'t>, ends: Vec<Pending>) -> Vec<Pending> {
        let span = statement.span();
        let parts: Vec<Element<'t>> = statement.children().collect();
        let body = nodes_of(statement)
            .filter(|part| part.kind() == NodeKind::Block)
            .last();
        if statement.kind() != NodeKind::Block {
            self.enter(span, &ends);
        }
        // What is written after the first `(`.
        let headed = || {
            let mut from = parts
                .iter()
                .skip_while(|part| punct_of(Some(part)) != Some(Punct::LParen));
            from.nth(1).copied()
        };
        match statement.kind() {
            NodeKind::Block => self.block(statement, ends),
            NodeKind::EmptyStmt | NodeKind::FunctionDecl => ends,
            NodeKind::LabelledStmt => {
                let label = statement.tokens().first().map(|label| label.span);
                self.label = label.and_then(|span| self.source.slice(&span).map(str::to_owned));
                let inner = nodes_of(statement).last();
                let left_by = inner.map_or(ends.clone(), |inner| self.statement(inner, ends));
                self.label = None;
                left_by
            }
            NodeKind::IfStmt => {
                let unknown = (ends.clone(), ends.clone());
                let (held, failed) = headed().map_or(unknown, |test| self.condition(test, ends));
                let mut branches = nodes_of(statement)
                    .filter(|part| matches!(part.kind(), NodeKind::Block | NodeKind::IfStmt));
                let then = branches.next();
                let mut left_by = then.map_or(held.clone(), |then| self.statement(then, held));
                let other = branches.next();
                left_by.extend(other.map_or(failed.clone(), |other| self.statement(other, failed)));
                left_by
            }
            NodeKind::WhileStmt => self.looped(body, ends, headed(), None, span),
            NodeKind::ForStmt => {
                // `for (first; test; update)`: what is between the semicolons.
                let is = |part: &Element<'t>, punct: Punct| punct_of(Some(part)) == Some(punct);
                let open = parts.iter().position(|part| is(part, Punct::LParen));
                let close = parts.iter().rposition(|part| is(part, Punct::RParen));
                let inside = open
                    .zip(close)
                    .and_then(|(open, close)| parts.get(open + 1..close));
                let mut clauses = inside
                    .unwrap_or_default()
                    .split(|part| is(part, Punct::Semicolon))
                    .map(|clause| clause.first().copied());
                let first = clauses.next().flatten();
                let ends = first.map_or(ends.clone(), |first| self.value(first, ends));
                let test = clauses.next().flatten();
                let update = clauses.next().flatten();
                self.looped(body, ends, test, update, span)
            }
            NodeKind::ForOfStmt | NodeKind::ForInStmt => {
                // What is iterated comes before the `)`.
                let iterated = parts.iter().rev().nth(2).copied();
                let ends = iterated.map_or(ends.clone(), |iterated| self.value(iterated, ends));
                self.open(true);
                let next = self.node("next".to_owned(), span, ends);
                let into = vec![(next, "")];
                let round = body.map_or(into.clone(), |body| self.block(body, into));
                let (continues, breaks) = self.close();
                self.join([round, continues].concat(), next);
                [vec![(next, "done")], breaks].concat()
            }
            NodeKind::SwitchStmt => {
                let mut tested = headed().map_or(ends.clone(), |subject| self.value(subject, ends));
                self.open(false);
                let clauses: Vec<NodeRef<'t>> = nodes_of(statement)
                    .filter(|part| part.kind() == NodeKind::CaseClause)
                    .collect();
                // The tests in order; then the bodies, each running into the next.
                let mut entries: Vec<Vec<Pending>> = Vec::new();
                let mut default = None;
                for (at, clause) in clauses.iter().enumerate() {
                    let mut entered = Vec::new();
                    for label in nodes_of(*clause).filter(|part| part.kind() == NodeKind::CaseLabel)
                    {
                        // `case`, the value, `:`: a `default` has no value. A
                        // value is a literal, with its sign.
                        let written = label.tokens();
                        let value = written.get(1..written.len().saturating_sub(1));
                        let value = value.unwrap_or_default();
                        match (value.first(), value.last()) {
                            (Some(first), Some(last)) => {
                                let span = Span::new(
                                    first.span.start,
                                    last.span.end,
                                    first.span.byte_start,
                                    last.span.byte_end,
                                );
                                let test = self.node(self.text(&span), span, tested);
                                entered.push((test, "case"));
                                tested = vec![(test, "")];
                            }
                            _ => default = Some(at),
                        }
                    }
                    entries.push(entered);
                }
                // What matches no `case` goes to the `default`, or past the switch.
                let mut past = Vec::new();
                match default {
                    Some(at) => entries[at].extend(tested),
                    None => past = tested,
                }
                let mut fallen: Vec<Pending> = Vec::new();
                for (at, clause) in clauses.iter().enumerate() {
                    let entered = [std::mem::take(&mut entries[at]), fallen].concat();
                    let block = nodes_of(*clause)
                        .filter(|part| part.kind() == NodeKind::Block)
                        .last();
                    fallen = block.map_or(entered.clone(), |block| self.block(block, entered));
                    if at + 1 < clauses.len() {
                        let label = nodes_of(*clause)
                            .next()
                            .map_or(clause.span(), NodeRef::span);
                        let from = fallen.iter().map(|(from, _)| *from).collect();
                        self.graph.falls.push((label, from));
                    }
                }
                let (_, breaks) = self.close();
                [past, fallen, breaks].concat()
            }
            NodeKind::TryStmt => {
                let clause = |kind: NodeKind| nodes_of(statement).find(|part| part.kind() == kind);
                let block_of = |clause: NodeRef<'t>| {
                    nodes_of(clause)
                        .filter(|part| part.kind() == NodeKind::Block)
                        .last()
                };
                let catch = clause(NodeKind::CatchClause);
                let finally = clause(NodeKind::FinallyClause).and_then(block_of);
                self.tries.push(Try {
                    catches: catch.is_some(),
                    thrown: Vec::new(),
                    finally,
                    targets: self.targets.len(),
                });
                let at = self.tries.len() - 1;
                let tried = nodes_of(statement).find(|part| part.kind() == NodeKind::Block);
                let mut left_by = tried.map_or(ends.clone(), |tried| self.block(tried, ends));
                // What the block throws enters the `catch`; what the `catch` throws
                // is caught no more.
                self.tries[at].catches = false;
                let thrown = std::mem::take(&mut self.tries[at].thrown);
                let escaping = match catch.and_then(block_of) {
                    Some(handler) => {
                        left_by.extend(self.block(handler, thrown));
                        std::mem::take(&mut self.tries[at].thrown)
                    }
                    None => thrown,
                };
                // What escapes runs the `finally`, and is thrown on.
                let left_by = self.through_finally(at, left_by);
                let thrown_on = self.through_finally(at, escaping);
                self.tries.pop();
                if !thrown_on.is_empty() {
                    self.jump(thrown_on, Jump::Throw);
                }
                left_by
            }
            NodeKind::ReturnStmt | NodeKind::ThrowStmt => {
                let value = parts
                    .get(1)
                    .copied()
                    .filter(|value| punct_of(Some(value)).is_none());
                let ends = match value.filter(has_flow) {
                    Some(value) => self.value(value, ends),
                    None => ends,
                };
                // A `return` may throw as it evaluates what it gives; what a `throw`
                // throws is its own.
                if statement.kind() == NodeKind::ReturnStmt {
                    let at = self.node(self.text(&span), span, ends);
                    self.jump(vec![(at, "return")], Jump::Return);
                } else {
                    let at = self.jumping(span, ends);
                    self.jump(vec![(at, "throw")], Jump::Throw);
                }
                Vec::new()
            }
            NodeKind::BreakStmt | NodeKind::ContinueStmt => {
                let continues = statement.kind() == NodeKind::ContinueStmt;
                let at = self.jumping(span, ends);
                let label = if continues { "continue" } else { "break" };
                match (self.target(statement, continues), continues) {
                    (Some(target), true) => self.jump(vec![(at, label)], Jump::Continue(target)),
                    (Some(target), false) => self.jump(vec![(at, label)], Jump::Break(target)),
                    // Bound to nothing, it goes nowhere: the parser's finding.
                    (None, _) => {}
                }
                Vec::new()
            }
            _ => {
                // An expression, a declaration, or a statement of a removed form:
                // what it evaluates, then the whole.
                let flowing: Vec<Element<'t>> = parts.iter().copied().filter(has_flow).collect();
                let mut ends = ends;
                for part in flowing {
                    ends = self.value(part, ends);
                }
                vec![(self.node(self.text(&span), span, ends), "")]
            }
        }
    }
}

impl Cfg {
    /// The graph of a body: a block of statements, or the expression an arrow gives.
    #[must_use]
    pub fn of_body(source: &SourceFile, body: Element<'_>) -> Self {
        let named = |label: &str| Node {
            label: label.to_owned(),
            range: None,
        };
        let mut builder = Builder {
            source,
            graph: Self {
                nodes: vec![named("entry"), named("exit"), named("raise")],
                edges: Vec::new(),
                statements: BTreeMap::new(),
                falls: Vec::new(),
            },
            targets: Vec::new(),
            tries: Vec::new(),
            absent: Vec::new(),
            label: None,
        };
        let entered = vec![(ENTRY, "")];
        let left_by = match body {
            Element::Node(block) if matches!(block.kind(), NodeKind::Block | NodeKind::Module) => {
                builder.block(block, entered)
            }
            // What an arrow gives is what it returns.
            expression => {
                let ends = builder.value(expression, entered);
                ends.into_iter().map(|(from, _)| (from, "return")).collect()
            }
        };
        builder.join(left_by, EXIT);
        builder.graph
    }

    /// The nodes control can reach from the entry.
    #[must_use]
    pub fn reachable(&self) -> Vec<bool> {
        self.reachable_avoiding(&[])
    }

    /// The nodes control can reach from the entry without going on from any of
    /// `blockers`: a blocker is reached, and left only by what it may throw, for it
    /// may throw before it is done.
    #[must_use]
    pub fn reachable_avoiding(&self, blockers: &[usize]) -> Vec<bool> {
        let mut leaving: Vec<Vec<usize>> = vec![Vec::new(); self.nodes.len()];
        for edge in &self.edges {
            if edge.label == "throws" || !blockers.contains(&edge.from) {
                leaving[edge.from].push(edge.to);
            }
        }
        let mut reached = vec![false; self.nodes.len()];
        let mut pending = vec![ENTRY];
        while let Some(at) = pending.pop() {
            if !std::mem::replace(&mut reached[at], true) {
                pending.extend(&leaving[at]);
            }
        }
        reached
    }

    /// Whether the statement or block at `range` is entered from a node of
    /// `reached`; `None` when nothing was built there.
    #[must_use]
    pub fn entered(&self, range: (usize, usize), reached: &[bool]) -> Option<bool> {
        let from = self.statements.get(&range)?;
        Some(from.iter().any(|from| reached[*from]))
    }

    /// Whether every way from the entry to the node `b` goes through the node `a`.
    /// A node dominates itself; nothing dominates a node control cannot reach.
    #[must_use]
    pub fn dominates(&self, a: usize, b: usize) -> bool {
        let reached = self.reachable()[b];
        reached && (a == b || !self.reachable_avoiding(&[a])[b])
    }

    /// The node that stands for the text at `range` most closely: the smallest
    /// node that holds it. Every node that is that text, when it was built more
    /// than once.
    #[must_use]
    pub fn nodes_holding(&self, range: (usize, usize)) -> Vec<usize> {
        let holds = |node: &Node| {
            node.range
                .filter(|(from, to)| *from <= range.0 && range.1 <= *to)
        };
        let sizes = self
            .nodes
            .iter()
            .filter_map(|node| holds(node).map(|(from, to)| to - from));
        let smallest = sizes.min();
        let nodes = self.nodes.iter().enumerate();
        nodes
            .filter(|(_, node)| holds(node).map(|(from, to)| to - from) == smallest)
            .filter(|_| smallest.is_some())
            .map(|(at, _)| at)
            .collect()
    }

    /// The graph in the DOT language: a node a line, then an edge a line, in the
    /// order they were made.
    #[must_use]
    pub fn to_dot(&self) -> String {
        let mut out = String::from("digraph cfg {\n");
        for (at, node) in self.nodes.iter().enumerate() {
            let label = node.label.replace('\\', "\\\\").replace('"', "\\\"");
            // Writing to a string does not fail.
            let _ = writeln!(out, "  n{at} [label=\"{label}\"]");
        }
        for edge in &self.edges {
            let label = if edge.label.is_empty() {
                String::new()
            } else {
                format!(" [label=\"{}\"]", edge.label)
            };
            let _ = writeln!(out, "  n{} -> n{}{label}", edge.from, edge.to);
        }
        out.push_str("}\n");
        out
    }
}

/// A node and every node inside it that runs when it does: not the functions and
/// classes written in it.
#[must_use]
pub fn running(node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        pending.extend(nodes_of(node).filter(|inner| !is_scope(*inner)));
        found.push(node);
    }
    found
}

/// The body of a function, when a node is one: the block of a function, a method,
/// an accessor or a `static` block, what an arrow gives, the statements of a module.
#[must_use]
pub fn body_of(node: NodeRef<'_>) -> Option<Element<'_>> {
    let block = || {
        nodes_of(node)
            .filter(|part| part.kind() == NodeKind::Block)
            .last()
    };
    match node.kind() {
        NodeKind::Module => Some(Element::Node(node)),
        NodeKind::ArrowFunction => node.children().last(),
        NodeKind::FunctionDecl
        | NodeKind::FunctionExpr
        | NodeKind::ClassMethod
        | NodeKind::ClassAccessor
        | NodeKind::StaticBlock
        | NodeKind::Property => block().map(Element::Node),
        _ => None,
    }
}

/// A node and every node inside it, as the text has them.
fn every(node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        let inner: Vec<NodeRef<'_>> = nodes_of(node).collect();
        pending.extend(inner.into_iter().rev());
        found.push(node);
    }
    found
}

/// The graph of every function of a file as the text has them, the module's own
/// statements first.
#[must_use]
pub fn graphs(source: &SourceFile, tree: &SyntaxTree) -> Vec<Cfg> {
    let bodies = every(tree.root()).into_iter().filter_map(body_of);
    bodies.map(|body| Cfg::of_body(source, body)).collect()
}
