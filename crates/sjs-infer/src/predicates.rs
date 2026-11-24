//! Predicate functions (SPEC.md §3.5): a function whose body is one `return` of a
//! test of a parameter is that test when it is called in a condition, `!f(x)` and
//! `f(x) && g(x)` included. What it returns is read as conditions are: `!`, `&&` and
//! `||` over tests, each test of the parameter or of keys read from it; a call of
//! another predicate there is a test too. Passed to `filter`, a predicate tells what
//! the elements that are kept are.

use crate::expressions::{element_span_of, nodes, punct, Typer};
use crate::mutation::arguments;
use crate::narrowing::{range_of, Check, Names, Path, Range, Test};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct};
use sjs_types::TypeId;
use std::collections::{BTreeMap, BTreeSet};

/// How many predicates deep one is followed into another: one that calls itself is
/// followed no further.
const DEPTH: usize = 8;

/// A function that returns one expression.
#[derive(Clone)]
pub(crate) struct Predicate<'t> {
    /// The declaration of each parameter that is a plain name.
    parameters: Vec<Option<usize>>,
    /// What it returns.
    body: Element<'t>,
}

impl<'t> Predicate<'t> {
    /// Reads a function with these parameters and this body: one whose body is an
    /// expression, or a block of one `return` of a value.
    pub(crate) fn read(
        parameters: NodeRef<'t>,
        body: Element<'t>,
        declared_at: &BTreeMap<usize, usize>,
    ) -> Option<Self> {
        let body = match body {
            Element::Node(block) if block.kind() == NodeKind::Block => {
                let mut statements = nodes(block);
                let only = statements.next().filter(|_| statements.next().is_none());
                let returned = only.filter(|only| only.kind() == NodeKind::ReturnStmt);
                let value = returned.and_then(|returned| returned.children().nth(1));
                value.filter(|value| punct(Some(value)).is_none())?
            }
            value => value,
        };
        let parameters = nodes(parameters)
            .map(|parameter| match parameter.children().next() {
                Some(Element::Token(name)) => {
                    declared_at.get(&name.token().span.byte_start).copied()
                }
                _ => None,
            })
            .collect();
        Some(Self { parameters, body })
    }
}

/// Makes each call of a predicate a test of what it is passed: of each path the
/// predicate tests from its parameter, read from the argument.
pub(crate) fn told_by_calls<'t>(
    names: &Names<'_>,
    predicates: &BTreeMap<usize, Predicate<'t>>,
    calls: &[NodeRef<'t>],
    tests: &mut BTreeMap<Range, Vec<Test<'t>>>,
) {
    // Once for each predicate inside another, as deep as they are followed.
    for _ in 0..DEPTH {
        for call in calls {
            let parts: Vec<Element<'t>> = call.children().collect();
            let callee = match parts.first() {
                Some(Element::Token(name)) => names.declared(name.token().span.byte_start),
                _ => None,
            };
            let Some(predicate) = callee.and_then(|callee| predicates.get(&callee)) else {
                continue;
            };
            let span = element_span_of(&predicate.body);
            let inside = tests.range((span.byte_start, 0)..(span.byte_end, 0));
            let inside: Vec<&Test<'t>> = inside.flat_map(|(_, tests)| tests).collect();
            let passed = arguments(&parts);
            let mut told: BTreeSet<(Path, usize, String)> = BTreeSet::new();
            for (argument, parameter) in passed.iter().zip(&predicate.parameters) {
                let Some(((root, keys), parameter)) = names.path(argument).zip(*parameter) else {
                    continue;
                };
                let of_parameter = inside.iter().filter(|test| test.path.0 == parameter);
                told.extend(of_parameter.map(|test| {
                    let read = format!("{keys}{}", test.path.1);
                    ((root, read), parameter, test.path.1.clone())
                }));
            }
            let told: Vec<Test<'t>> = told
                .into_iter()
                .map(|(path, parameter, keys)| Test {
                    path,
                    check: Check::Predicate(predicate.body, parameter, keys),
                    negated: false,
                })
                .collect();
            if !told.is_empty() {
                tests.insert(range_of(*call), told);
            }
        }
    }
}

impl Typer<'_> {
    /// What is left of a `ty`, the type of `path`, when the condition `asked` holds,
    /// or when it does not: `!`, `&&` and `||` over the tests of the path.
    pub(crate) fn told(
        &mut self,
        asked: Element<'_>,
        path: &Path,
        ty: TypeId,
        holds: bool,
    ) -> TypeId {
        if self.narrowing.depth >= DEPTH {
            return ty;
        }
        self.narrowing.depth += 1;
        let left = self.told_within(asked, path, ty, holds);
        self.narrowing.depth -= 1;
        left
    }

    fn told_within(&mut self, asked: Element<'_>, path: &Path, ty: TypeId, holds: bool) -> TypeId {
        let parts: Vec<Element<'_>> = match asked {
            Element::Node(node) => node.children().collect(),
            Element::Token(_) => Vec::new(),
        };
        let sign = punct(parts.get(1));
        match parts.as_slice() {
            [open, inside, _] if punct(Some(open)) == Some(Punct::LParen) => {
                self.told_within(*inside, path, ty, holds)
            }
            [not, operand] if punct(Some(not)) == Some(Punct::Bang) => {
                self.told_within(*operand, path, ty, !holds)
            }
            [left, _, right] if matches!(sign, Some(Punct::AmpAmp | Punct::PipePipe)) => {
                // `a && b` holds, and `a || b` does not, when both sides answer so:
                // the right is asked of what the left let through.
                let both = (sign == Some(Punct::AmpAmp)) == holds;
                let first = self.told_within(*left, path, ty, holds);
                if both {
                    return self.told_within(*right, path, first, holds);
                }
                // Otherwise one side answers so: the left, or the right once the
                // left has not.
                let past = self.told_within(*left, path, ty, !holds);
                let second = self.told_within(*right, path, past, holds);
                self.types.union([first, second])
            }
            _ => {
                let span = element_span_of(&asked);
                let tests = self.narrowing.tests.get(&(span.byte_start, span.byte_end));
                let tests = tests.into_iter().flatten();
                let tests: Vec<Test<'_>> =
                    tests.filter(|test| test.path == *path).cloned().collect();
                let mut ty = ty;
                for test in tests {
                    ty = self.apply(ty, &test.check, holds != test.negated);
                }
                ty
            }
        }
    }

    /// What the elements of `item` that `filter` keeps are, when it is passed a
    /// predicate: by name, or an arrow written there.
    pub(crate) fn sifted(&mut self, by: Option<&Element<'_>>, item: TypeId) -> TypeId {
        let predicate = by.and_then(|by| match by {
            Element::Token(name) => {
                let start = name.token().span.byte_start;
                let reference = self.references.get(&start);
                let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
                declared.and_then(|declared| self.narrowing.predicates.get(&declared))
            }
            Element::Node(arrow) => self.narrowing.arrows.get(&arrow.span().byte_start),
        });
        let tested = predicate.and_then(|predicate| {
            let first = predicate.parameters.first().copied().flatten();
            first.map(|first| (predicate.body, first))
        });
        match tested {
            Some((body, parameter)) => self.told(body, &(parameter, String::new()), item, true),
            None => item,
        }
    }
}
