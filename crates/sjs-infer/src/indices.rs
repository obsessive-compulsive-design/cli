//! Array index proofs (SPEC.md §4.6). `arr[i]` reads an element, and not
//! `undefined`, when the place is shown to be in the array: `i` is not negative (it
//! starts at a literal and only grows, or `i >= 0` was asked) and is less than the
//! length (`i < arr.length` was asked, as a loop's test or a condition's), on every
//! way to the read; or the place is written as a number and `arr.length > n` was
//! asked. What cancels a proof: an assignment to the index; a method of the array
//! that mutates it, an assignment to the array or to its `length`; and a call, when
//! the array is mutated somewhere and other code can reach it.
//!
//! A read that is not proven gives the element or `undefined`. Where it is used as
//! the element itself (`arr[i].x`, `arr[i] + 1`), that is E124.

use crate::expressions::{element_span_of, punct, Typer};
use crate::mutation::{arguments, MUTATING};
use crate::narrowing::{owner_of, range_of, Flow, Names, Path, Range};
use sjs_bind::{running, ENTRY};
use sjs_diag::Code;
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, TokenKind};
use sjs_types::{Literal, Type};
use std::collections::BTreeMap;
use std::rc::Rc;

/// What an array is read by: a name, or a place written as a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Index {
    Name(usize),
    Place(usize),
}

/// What a comparison shows.
#[derive(Debug, Clone)]
enum Shown {
    /// The name is less than the length of the array.
    Below(usize, Path),
    /// The name is not negative.
    Counted(usize),
    /// The array has this many elements at least.
    Holds(Path, usize),
}

/// What a comparison shows when it holds, or when it does not.
#[derive(Debug, Clone)]
pub(crate) struct Fact {
    when: bool,
    shown: Shown,
}

/// What is proven at each node of a graph: that the index is not negative, and
/// that it is less than the length; `None` where control does not come.
pub(crate) type Proven = Vec<Option<(bool, bool)>>;

/// A side of a comparison.
#[derive(Clone)]
enum Side {
    Name(usize),
    Length(Path),
    Place(usize),
}

/// What happens to an index at a node.
#[derive(Clone, Copy)]
enum Step {
    /// It is given a value nothing is known of.
    Lost,
    /// It is given a literal that is not negative.
    Set,
    /// It grows: by `+= n` or `++`.
    Grown,
}

/// The place a token writes: a whole number, written in decimal.
fn place_of(names: &Names<'_>, element: &Element<'_>) -> Option<usize> {
    let Element::Token(written) = element else {
        return None;
    };
    let is_number = matches!(written.token().kind, TokenKind::Number(_));
    let text = names
        .source
        .slice(&written.token().span)
        .filter(|_| is_number);
    text.and_then(|text| text.parse().ok())
}

fn side_of(names: &Names<'_>, element: &Element<'_>) -> Option<Side> {
    if let Some(place) = place_of(names, element) {
        return Some(Side::Place(place));
    }
    let (root, keys) = names.path(element)?;
    match keys.strip_suffix(".length") {
        Some(array) => Some(Side::Length((root, array.to_owned()))),
        None => keys.is_empty().then_some(Side::Name(root)),
    }
}

/// What a comparison shows of an index or of a length.
fn facts(names: &Names<'_>, node: NodeRef<'_>) -> Vec<Fact> {
    let parts: Vec<Element<'_>> = node.children().collect();
    // The sides are the first and the last part, the operator between them.
    let last = parts.len() - 1;
    let (left, operator, right) = (&parts[0], &parts[last / 2], &parts[last]);
    let Some((a, b)) = side_of(names, left).zip(side_of(names, right)) else {
        return Vec::new();
    };
    // Each ordering as `less < more` or `less <= more`: what holds when the
    // comparison does, and the opposite when it does not.
    let ordered: Vec<(Side, Side, bool, bool)> = match punct(Some(operator)) {
        Some(Punct::Lt) => vec![(a, b, true, true)],
        Some(Punct::Gt) => vec![(b, a, true, true)],
        Some(Punct::LtEq) => vec![(a, b, false, true)],
        Some(Punct::GtEq) => vec![(b, a, false, true)],
        // A length that is a number is that many; one that is not none is one or
        // more.
        Some(sign @ (Punct::EqEqEq | Punct::BangEqEq)) => {
            let equal = sign == Punct::EqEqEq;
            let ((Side::Length(array), Side::Place(count))
            | (Side::Place(count), Side::Length(array))) = (a, b)
            else {
                return Vec::new();
            };
            let some = Fact {
                when: !equal,
                shown: Shown::Holds(array.clone(), 1),
            };
            let exact = Fact {
                when: equal,
                shown: Shown::Holds(array, count),
            };
            return if count == 0 { vec![some] } else { vec![exact] };
        }
        _ => Vec::new(),
    };
    let mut found = Vec::new();
    for (less, more, strict, when) in ordered {
        // When it does not hold, `more <= less`, or `more < less`.
        let both = [
            (less.clone(), more.clone(), strict, when),
            (more, less, !strict, !when),
        ];
        for (less, more, strict, when) in both {
            let shown = match (less, more) {
                (Side::Name(index), Side::Length(array)) if strict => Shown::Below(index, array),
                (Side::Place(_), Side::Name(index)) => Shown::Counted(index),
                (Side::Place(count), Side::Length(array)) => {
                    Shown::Holds(array, count + usize::from(strict))
                }
                _ => continue,
            };
            found.push(Fact { when, shown });
        }
    }
    found
}

/// What each comparison among `nodes` shows, by its bytes.
pub(crate) fn orders_of(names: &Names<'_>, nodes: &[NodeRef<'_>]) -> BTreeMap<Range, Vec<Fact>> {
    let binary = nodes
        .iter()
        .filter(|node| node.kind() == NodeKind::BinaryExpr);
    let found = binary.map(|node| (range_of(*node), facts(names, *node)));
    found.filter(|(_, facts)| !facts.is_empty()).collect()
}

/// Whether what is read may be `undefined` where it stands: where it is kept,
/// passed on, compared or asked for, and not used as the element itself.
fn accepts_absent(node: NodeRef<'_>) -> bool {
    node.parent().is_none_or(|parent| {
        let parts: Vec<Element<'_>> = parent.children().collect();
        let first = parent.span().byte_start == node.span().byte_start;
        let sign = punct(parts.get(1));
        let word = parent.tokens().first().map(|word| &word.kind);
        match parent.kind() {
            NodeKind::ParenExpr | NodeKind::OptionalChain => accepts_absent(parent),
            NodeKind::Initializer
            | NodeKind::ReturnStmt
            | NodeKind::ArrayLit
            | NodeKind::Property
            | NodeKind::ExprStmt
            | NodeKind::CondExpr
            | NodeKind::IfStmt
            | NodeKind::WhileStmt
            | NodeKind::ArrowFunction => true,
            // What is assigned, and the place it is assigned to: `+=` and the like
            // work on the element.
            NodeKind::AssignExpr => sign == Some(Punct::Eq),
            // An argument.
            NodeKind::CallExpr | NodeKind::NewExpr => !first,
            NodeKind::BinaryExpr => matches!(
                sign,
                Some(
                    Punct::EqEqEq
                        | Punct::BangEqEq
                        | Punct::QuestionQuestion
                        | Punct::AmpAmp
                        | Punct::PipePipe
                )
            ),
            NodeKind::UnaryExpr => {
                let asks = [
                    TokenKind::Keyword(Keyword::Typeof),
                    TokenKind::Punct(Punct::Bang),
                ];
                word.is_some_and(|word| asks.contains(word))
            }
            // Read by `?.`, which asks.
            NodeKind::MemberExpr | NodeKind::IndexExpr => first && sign == Some(Punct::QuestionDot),
            _ => false,
        }
    })
}

impl Typer<'_> {
    /// What an array is read by in `arr[key]`: a place when the key is one number
    /// that is known, a name when it is one.
    fn index_of(&mut self, key: Element<'_>) -> Option<Index> {
        let ty = self.type_of(key);
        let place = ty.and_then(|ty| match self.types.get(ty) {
            Type::Literal(Literal::Number(number)) => number.text().parse().ok(),
            _ => None,
        });
        let name = self.path_of(&key).filter(|(_, keys)| keys.is_empty());
        place
            .map(Index::Place)
            .or(name.map(|(root, _)| Index::Name(root)))
    }

    /// What happens to `index` and to `array` at each node of `flow`, the graph of
    /// the function `owner`: the step of the index, and whether what
    /// was shown of the array is cancelled.
    fn steps(
        &mut self,
        flow: &Flow,
        owner: NodeRef<'_>,
        index: Index,
        array: &Path,
    ) -> (Vec<Option<Step>>, Vec<bool>) {
        let count = flow.cfg.nodes.len();
        let mut steps: Vec<Option<Step>> = vec![None; count];
        let mut cancels = vec![false; count];
        let named = match index {
            Index::Name(named) => Some(named),
            Index::Place(_) => None,
        };
        let declared = |at: usize| {
            let span = self.bindings.declarations[at].span;
            (span.byte_start, span.byte_end)
        };
        // Where each is declared, it holds a new value.
        for at in named
            .into_iter()
            .flat_map(|named| flow.cfg.nodes_holding(declared(named)))
        {
            steps[at] = Some(Step::Lost);
        }
        for at in flow.cfg.nodes_holding(declared(array.0)) {
            cancels[at] = true;
        }
        let mutated = self.facts.mutated.contains(&array.0);
        for node in running(owner) {
            let parts: Vec<Element<'_>> = node.children().collect();
            let resolver = self.names_of();
            let is_index = |element: &Element<'_>| {
                let path = resolver.path(element);
                named.is_some() && path == named.map(|named| (named, String::new()))
            };
            let counts = |element: Option<&Element<'_>>| {
                element.is_some_and(|element| place_of(&resolver, element).is_some())
            };
            let sign = punct(parts.get(1));
            let value = parts.last();
            // What is written to: after `++`, when that comes first.
            let target = match parts.first() {
                Some(first) if punct(Some(first)).is_some() => parts.get(1),
                first => first,
            };
            let grows = parts
                .iter()
                .any(|part| punct(Some(part)) == Some(Punct::PlusPlus));
            let step = match (node.kind(), parts.as_slice()) {
                // `let i = 0`: the value is what the initializer ends in.
                (
                    NodeKind::LetDecl | NodeKind::ConstDecl,
                    [_, Element::Token(name), Element::Node(given)],
                ) => {
                    let is = named.map(declared).map(|(start, _)| start)
                        == Some(name.token().span.byte_start);
                    let given = given.children().last();
                    is.then_some(if counts(given.as_ref()) {
                        Step::Set
                    } else {
                        Step::Lost
                    })
                }
                // `i = 0`, `i += 1`, `i++`, `++i`: in a statement, or as the update of
                // a `for`.
                (NodeKind::AssignExpr | NodeKind::ForUpdate | NodeKind::UpdateExpr, _)
                    if target.is_some_and(is_index) =>
                {
                    Some(match sign {
                        Some(Punct::Eq) if counts(value) => Step::Set,
                        Some(Punct::PlusEq) if counts(value) => Step::Grown,
                        _ if grows => Step::Grown,
                        _ => Step::Lost,
                    })
                }
                _ => None,
            };
            // The array is given another value, or another length, or a method
            // mutates it, or it is passed to what may.
            let is_array = |element: &Element<'_>| resolver.path(element).as_ref() == Some(array);
            let length = (array.0, format!("{}.length", array.1));
            let cancel = match (node.kind(), parts.as_slice()) {
                (NodeKind::AssignExpr | NodeKind::ForUpdate, [target, ..]) => {
                    is_array(target) || resolver.path(target) == Some(length)
                }
                (NodeKind::CallExpr | NodeKind::NewExpr, [callee, ..]) => {
                    let method = match callee {
                        Element::Node(callee) => {
                            let on = callee.children().next();
                            let method = callee.tokens().last().map(|method| method.span);
                            let method = method.and_then(|span| self.source.slice(&span));
                            on.filter(is_array).and(method)
                        }
                        Element::Token(_) => None,
                    };
                    let passed = arguments(&parts).iter().any(is_array);
                    method.is_some_and(|method| MUTATING.contains(&method)) || (passed && mutated)
                }
                _ => false,
            };
            if step.is_none() && !cancel {
                continue;
            }
            for at in flow.cfg.nodes_holding(range_of(node)) {
                steps[at] = step.or(steps[at]);
                cancels[at] = cancels[at] || cancel;
            }
        }
        (steps, cancels)
    }

    /// What is proven of `index` into `array` at each node of the function `owner`.
    fn proven(
        &mut self,
        owner: NodeRef<'_>,
        body: Element<'_>,
        index: Index,
        array: &Path,
    ) -> Rc<Proven> {
        let key = (range_of(owner), index, array.clone());
        if let Some(proven) = self.narrowing.proofs.get(&key) {
            return Rc::clone(proven);
        }
        let flow = self.flow_of(owner, body);
        let (steps, cancels) = self.steps(&flow, owner, index, array);
        // What a call may undo: an index a closure assigns; an array that is
        // mutated somewhere, when other code can reach it.
        let root = array.0;
        let reached = !array.1.is_empty() || self.narrowing.captured.contains(&root);
        let open = (self.facts.mutated.contains(&root) && reached)
            || self.narrowing.shared.contains(&root);
        let loose = matches!(index, Index::Name(named) if self.narrowing.shared.contains(&named));
        let mut start: Proven = vec![None; flow.cfg.nodes.len()];
        // A place that is written is not negative.
        start[ENTRY] = Some((matches!(index, Index::Place(_)), false));
        let orders = &self.narrowing.orders;
        let settled = settle(
            &flow,
            orders,
            (index, array),
            &(steps, cancels, open, loose),
            start,
        );
        let proven = Rc::new(settled);
        self.narrowing.proofs.insert(key, Rc::clone(&proven));
        proven
    }

    /// Whether `arr[key]` at `node` reads an element for certain: the place is
    /// shown to be in the array on every way there.
    fn index_proven(&mut self, node: NodeRef<'_>) -> bool {
        let parts: Vec<Element<'_>> = node.children().collect();
        let array = parts.first().and_then(|array| self.path_of(array));
        let key = parts.iter().rev().nth(1).copied();
        let index = key.and_then(|key| self.index_of(key));
        let Some(((array, index), (owner, body))) = array.zip(index).zip(owner_of(node)) else {
            return false;
        };
        let proven = self.proven(owner, body, index, &array);
        let flow = self.flow_of(owner, body);
        let held = flow.cfg.nodes_holding(range_of(node));
        let mut reaching = held.iter().filter_map(|at| proven[*at]);
        reaching.all(|(counted, below)| counted && below)
    }

    /// Whether the read `arr[key]` at `node` gives an element and never
    /// `undefined`: the place is proven to be in the array, or it is not and the
    /// read is used as the element itself, which is E124.
    pub(crate) fn dense_read(&mut self, node: NodeRef<'_>) -> bool {
        if self.index_proven(node) {
            return true;
        }
        if accepts_absent(node) {
            return false;
        }
        let parts: Vec<Element<'_>> = node.children().collect();
        let text = |element: Option<&Element<'_>>| {
            let span = element.map(element_span_of);
            let text = span.and_then(|span| self.source.slice(&span));
            text.unwrap_or_default()
                .split_whitespace()
                .collect::<String>()
        };
        let (array, key) = (text(parts.first()), text(parts.iter().rev().nth(1)));
        let whole = text(Some(&Element::Node(node)));
        let advice = if key == format!("{array}.length-1") {
            format!("read the last element with `{array}.at(-1)` and compare it with `undefined`")
        } else if key.parse::<usize>().is_ok() {
            format!(
                "test `{array}.length > {key}` before it, or compare what is read with `undefined`"
            )
        } else {
            format!("test `{key} >= 0 && {key} < {array}.length` before it, or compare what is read with `undefined`")
        };
        let message = format!(
            "`{whole}` may be `undefined`: nothing shows that the place is in the array; {advice} (SPEC.md §4.6)"
        );
        // Once for each read, however often it is typed.
        let first = self.narrowing.refused.insert(range_of(node));
        self.report(first, Code::E124, node.span(), message);
        true
    }
}

/// What is shown past the edge that leaves the node `at` by `label`.
fn shown_along(
    flow: &Flow,
    orders: &BTreeMap<Range, Vec<Fact>>,
    (index, array): (Index, &Path),
    (at, label): (usize, &str),
    (counted, below): (bool, bool),
) -> (bool, bool) {
    let holds = label == "true";
    let facts = flow.cfg.nodes[at]
        .range
        .and_then(|range| orders.get(&range));
    let facts = facts.filter(|_| holds || label == "false");
    let facts = facts
        .into_iter()
        .flatten()
        .filter(|fact| fact.when == holds);
    let mut proven = (counted, below);
    for fact in facts {
        match &fact.shown {
            Shown::Below(named, of) => {
                proven.1 = proven.1 || (index == Index::Name(*named) && of == array);
            }
            Shown::Counted(named) => proven.0 = proven.0 || index == Index::Name(*named),
            Shown::Holds(of, count) => {
                let within = matches!(index, Index::Place(place) if place < *count);
                proven.1 = proven.1 || (within && of == array);
            }
        }
    }
    proven
}

/// What is proven at each node once nothing more is taken back: where ways meet,
/// only what every way shows stays.
fn settle(
    flow: &Flow,
    orders: &BTreeMap<Range, Vec<Fact>>,
    of: (Index, &Path),
    undone: &(Vec<Option<Step>>, Vec<bool>, bool, bool),
    proven: Proven,
) -> Proven {
    let (steps, cancels, open, loose) = undone;
    let mut next = proven.clone();
    for at in 0..next.len() {
        let Some((counted, below)) = next[at] else {
            continue;
        };
        let (counted, below) = match steps[at] {
            Some(Step::Lost) => (false, false),
            Some(Step::Set) => (true, false),
            Some(Step::Grown) => (counted, false),
            None => (counted, below),
        };
        let calls = flow.calls[at];
        let below = below && !cancels[at] && !(calls && *open);
        let leaves = (counted && !(calls && *loose), below && !(calls && *loose));
        for &(to, label) in &flow.leaving[at] {
            let (counted, below) = shown_along(flow, orders, of, (at, label), leaves);
            let meeting = next[to].unwrap_or((true, true));
            next[to] = Some((meeting.0 && counted, meeting.1 && below));
        }
    }
    if next == proven {
        proven
    } else {
        settle(flow, orders, of, undone, next)
    }
}
