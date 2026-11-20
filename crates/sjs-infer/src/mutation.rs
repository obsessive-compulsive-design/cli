//! What a program mutates (SPEC.md §3.4): a binding is mutated when it is written
//! into (`o.k = v`, `xs[i] = v`), has a mutating method called on it, or is passed to
//! a parameter the function mutates. And what fills an empty `[]`, `new Map()` or
//! `new Set()` (§3.2): the `push`/`unshift` and index assignments, `set`, `add` on
//! the binding that holds it.

use crate::bindings::empty_collection;
use crate::expressions::{nodes, punct, Typer};
use sjs_bind::Bindings;
use sjs_diag::{Code, SourceFile};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use std::collections::{BTreeMap, BTreeSet};

/// The methods that change the array, `Map` or `Set` they are called on.
const MUTATING: [&str; 13] = [
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
    "set",
    "add",
    "delete",
    "clear",
];

/// Something put into the collection a binding holds.
pub(crate) enum Fill<'t> {
    /// `xs.push(a, b)`, `xs.unshift(a)`, `xs[i] = a`: elements of an array.
    Elements(Vec<Element<'t>>),
    /// `m.set(key, value)`.
    Entry(Element<'t>, Element<'t>),
    /// `s.add(member)`.
    Member(Element<'t>),
}

/// What one file does to its bindings, by declaration index.
#[derive(Default)]
pub(crate) struct Uses<'t> {
    /// The declarations mutated where they are named.
    pub direct: BTreeSet<usize>,
    /// What is put into each declaration's collection.
    pub fills: BTreeMap<usize, Vec<Fill<'t>>>,
    /// The parameters of each function declared by name, in order; `None` for one
    /// that is no plain name.
    pub functions: BTreeMap<usize, Vec<Option<usize>>>,
    /// A binding passed by name: the argument, the function called, the place.
    pub calls: Vec<(usize, usize, usize)>,
}

/// The arguments of a call: what is between its parentheses, commas apart.
pub(crate) fn arguments<'t>(children: &[Element<'t>]) -> Vec<Element<'t>> {
    let open = children
        .iter()
        .position(|child| punct(Some(child)) == Some(Punct::LParen));
    // From the parenthesis on: it and the commas are no arguments.
    let inside = open.map_or(&[][..], |open| &children[open..]);
    inside
        .iter()
        .copied()
        .filter(|child| punct(Some(child)).is_none())
        .collect()
}

struct Collector<'a, 't> {
    source: &'a SourceFile,
    bindings: &'a Bindings,
    declared_at: BTreeMap<usize, usize>,
    referenced_at: BTreeMap<usize, usize>,
    uses: Uses<'t>,
}

impl<'t> Collector<'_, 't> {
    /// The declaration a name refers to, when `element` is a name.
    fn named(&self, element: Option<&Element<'t>>) -> Option<usize> {
        let Some(Element::Token(name)) = element else {
            return None;
        };
        let reference = self.referenced_at.get(&name.token().span.byte_start);
        reference.and_then(|&at| self.bindings.references[at].declaration)
    }

    /// The declaration a name declares, when `element` is a name that declares.
    fn declares(&self, element: Option<&Element<'t>>) -> Option<usize> {
        let Some(Element::Token(name)) = element else {
            return None;
        };
        self.declared_at.get(&name.token().span.byte_start).copied()
    }

    /// The parameters of the function whose parts are `children`, in order.
    fn parameters(&self, children: &[Element<'t>]) -> Vec<Option<usize>> {
        // The list is the first node of a function: `(a, b)`, or `a` alone.
        let list = children.iter().find_map(|child| match child {
            Element::Node(list) => Some(*list),
            Element::Token(_) => None,
        });
        let parts = list.into_iter().flat_map(NodeRef::children);
        parts
            .filter(|part| punct(Some(part)).is_none())
            .map(|part| match part {
                Element::Node(parameter) => {
                    let plain = parameter.kind() == NodeKind::Param;
                    let name = parameter.children().next();
                    self.declares(name.as_ref()).filter(|_| plain)
                }
                // `x => …`: the one name, without parentheses.
                name @ Element::Token(_) => self.declares(Some(&name)),
            })
            .collect()
    }

    fn call(&mut self, children: &[Element<'t>]) {
        let passed = arguments(children);
        if let Some(callee) = self.named(children.first()) {
            for (place, argument) in passed.iter().enumerate() {
                let argument = self.named(Some(argument));
                self.uses
                    .calls
                    .extend(argument.map(|argument| (argument, callee, place)));
            }
        }
        // `name.method(…)`.
        let Some(Element::Node(callee)) = children.first() else {
            return;
        };
        let parts: Vec<Element<'t>> = callee.children().collect();
        let method = callee.tokens().last().map(|method| method.span);
        let method = method
            .and_then(|span| self.source.slice(&span))
            .unwrap_or_default();
        let plain = callee.kind() == NodeKind::MemberExpr;
        let Some(object) = self.named(parts.first()).filter(|_| plain) else {
            return;
        };
        if MUTATING.contains(&method) {
            self.uses.direct.insert(object);
        }
        let fill = match (method, passed.as_slice()) {
            ("push" | "unshift", [_, ..]) => Some(Fill::Elements(passed.clone())),
            ("set", [key, value]) => Some(Fill::Entry(*key, *value)),
            ("add", [member]) => Some(Fill::Member(*member)),
            _ => None,
        };
        if let Some(fill) = fill {
            self.uses.fills.entry(object).or_default().push(fill);
        }
    }

    fn collect(&mut self, root: NodeRef<'t>) {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node));
            let children: Vec<Element<'t>> = node.children().collect();
            match node.kind() {
                NodeKind::CallExpr => self.call(&children),
                NodeKind::FunctionDecl => {
                    let name = children.iter().find_map(|child| self.declares(Some(child)));
                    let parameters = self.parameters(&children);
                    self.uses
                        .functions
                        .extend(name.map(|name| (name, parameters)));
                }
                NodeKind::ConstDecl | NodeKind::LetDecl => {
                    // `const f = (a, b) => …`.
                    let value = nodes(node).find(|part| part.kind() == NodeKind::Initializer);
                    let arrow = value.and_then(|value| {
                        nodes(value).find(|part| part.kind() == NodeKind::ArrowFunction)
                    });
                    let name = self.declares(children.get(1));
                    let parameters = arrow.map(|arrow| {
                        let parts: Vec<Element<'t>> = arrow.children().collect();
                        self.parameters(&parts)
                    });
                    self.uses.functions.extend(name.zip(parameters));
                }
                NodeKind::AssignExpr | NodeKind::UpdateExpr | NodeKind::ForUpdate => {
                    // A write into a member, however deep, changes the object the
                    // name at its root holds.
                    let into = nodes(node).find(|target| {
                        matches!(target.kind(), NodeKind::MemberExpr | NodeKind::IndexExpr)
                    });
                    let root = into.and_then(|target| target.tokens().first());
                    let object = root
                        .and_then(|name| self.referenced_at.get(&name.span.byte_start))
                        .and_then(|&at| self.bindings.references[at].declaration);
                    self.uses.direct.extend(object);
                    // `name[i] = value` puts `value` into the array `name` holds.
                    let indexed = into.filter(|target| target.kind() == NodeKind::IndexExpr);
                    let holder = indexed.and_then(|target| {
                        let first = target.children().next();
                        let is_name = matches!(&first, Some(Element::Token(name)) if name.token().kind == TokenKind::Ident);
                        self.named(first.as_ref()).filter(|_| is_name)
                    });
                    let plain = punct(children.get(1)) == Some(Punct::Eq);
                    let value = children.last().copied().filter(|_| plain);
                    if let Some((holder, value)) = holder.zip(value) {
                        let fills = self.uses.fills.entry(holder).or_default();
                        fills.push(Fill::Elements(vec![value]));
                    }
                }
                _ => {}
            }
        }
    }
}

/// What the file does to its bindings.
pub(crate) fn uses_of<'t>(
    source: &SourceFile,
    tree: &'t SyntaxTree,
    bindings: &Bindings,
) -> Uses<'t> {
    let spans = |starts: Vec<usize>| -> BTreeMap<usize, usize> {
        starts
            .into_iter()
            .enumerate()
            .map(|(at, start)| (start, at))
            .collect()
    };
    let mut collector = Collector {
        source,
        bindings,
        declared_at: spans(
            bindings
                .declarations
                .iter()
                .map(|d| d.span.byte_start)
                .collect(),
        ),
        referenced_at: spans(
            bindings
                .references
                .iter()
                .map(|r| r.span.byte_start)
                .collect(),
        ),
        uses: Uses::default(),
    };
    collector.collect(tree.root());
    collector.uses
}

/// Every mutated binding, given the ones mutated outright: a binding passed to a
/// parameter that is mutated is mutated too, and so on until nothing is added.
/// `parameter` is the parameter at a place of a function.
pub(crate) fn settled<Id: Ord + Copy>(
    mut mutated: BTreeSet<Id>,
    calls: &[(Id, Id, usize)],
    parameter: &dyn Fn(Id, usize) -> Option<Id>,
) -> BTreeSet<Id> {
    loop {
        let before = mutated.len();
        for &(argument, callee, place) in calls {
            let taken = parameter(callee, place);
            if taken.is_some_and(|taken| mutated.contains(&taken)) {
                mutated.insert(argument);
            }
        }
        if mutated.len() == before {
            return mutated;
        }
    }
}

impl Typer<'_> {
    /// Whether something is put into the field `name` within the class body `body`:
    /// `this.name.push(…)` and the like for its kind of collection, `this.name[i] = …`.
    fn field_is_filled(&self, body: NodeRef<'_>, name: &str, empty: &str) -> bool {
        let methods: &[&str] = match empty {
            "[]" => &["push", "unshift"],
            "new Map()" => &["set"],
            _ => &["add"],
        };
        let text = |node: NodeRef<'_>| self.source.slice(&node.span()).unwrap_or_default();
        let mut pending = vec![body];
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node));
            let children: Vec<Element<'_>> = node.children().collect();
            let Some(Element::Node(target)) = children.first().copied() else {
                continue;
            };
            let put = match node.kind() {
                NodeKind::CallExpr => {
                    let called = methods
                        .iter()
                        .any(|method| text(target) == format!("this.{name}.{method}"));
                    called && !arguments(&children).is_empty()
                }
                NodeKind::AssignExpr => {
                    let holder = nodes(target)
                        .next()
                        .filter(|_| target.kind() == NodeKind::IndexExpr);
                    let indexed =
                        holder.is_some_and(|holder| text(holder) == format!("this.{name}"));
                    indexed && empty == "[]" && punct(children.get(1)) == Some(Punct::Eq)
                }
                _ => false,
            };
            if put {
                return true;
            }
        }
        false
    }

    /// E110 for an empty `[]`, `new Map()` or `new Set()` that nothing gives a type:
    /// one that is not the value of a binding or of a field, or whose binding or field
    /// has nothing put into it where it is in scope.
    pub(crate) fn empty_untyped(&mut self, node: NodeRef<'_>) {
        let Some(empty) = empty_collection(self, &Element::Node(node)) else {
            return;
        };
        let first = self.first_time(node);
        let parent = node.parent();
        let holder = parent.and_then(NodeRef::parent);
        let is_value = parent.is_some_and(|parent| parent.kind() == NodeKind::Initializer);
        // The declaration of `const name = []`, `let name = []` or `name = []`.
        let declared = holder.filter(|_| is_value).and_then(|declaration| {
            let name = declaration.children().nth(1);
            let start = match name {
                Some(Element::Token(name)) => Some(name.token().span.byte_start),
                _ => None,
            };
            let declarations = &self.bindings.declarations;
            start.and_then(|start| declarations.iter().position(|d| d.span.byte_start == start))
        });
        let assigned = parent
            .filter(|parent| parent.kind() == NodeKind::AssignExpr)
            .and_then(|assign| {
                let parts: Vec<Element<'_>> = assign.children().collect();
                let plain = punct(parts.get(1)) == Some(Punct::Eq);
                let start = match parts.first() {
                    Some(Element::Token(name)) if plain => Some(name.token().span.byte_start),
                    _ => None,
                };
                let reference = start.and_then(|start| self.references.get(&start));
                reference.and_then(|&at| self.bindings.references[at].declaration)
            });
        let binding = declared.or(assigned);
        let typed = binding.is_some_and(|at| {
            self.facts.declarations.contains_key(&at) || self.facts.filled.contains(&at)
        });
        // The field of `name = []` in a class, filled anywhere in the class body.
        let field = holder.filter(|holder| is_value && holder.kind() == NodeKind::ClassField);
        let filled = field.is_some_and(|field| {
            // The name is what comes before the `=` of the value.
            let before = field
                .tokens()
                .iter()
                .take_while(|token| token.kind != TokenKind::Punct(Punct::Eq));
            let name = before.last().and_then(|name| self.source.slice(&name.span));
            let name = name.unwrap_or_default();
            field
                .parent()
                .is_some_and(|body| self.field_is_filled(body, name, empty))
        });
        if !typed && !filled {
            let how = match empty {
                "[]" => "`push`, `unshift`, `name[i] = …`",
                "new Map()" => "`set`",
                _ => "`add`",
            };
            let message = format!(
                "an empty `{empty}` has no type: it takes one from what is put into it where its binding or field is in scope ({how}), and nothing is (SPEC.md §3.2)"
            );
            self.report(first, Code::E110, node.span(), message);
        }
    }
}
