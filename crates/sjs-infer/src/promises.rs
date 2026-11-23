//! Promises (SPEC.md §4.4, §8.1). `await p` is what `p` resolves to, through every
//! promise it is nested in; awaiting what is no promise is E119. An instance of a
//! class with a `then` method is awaited as a promise is. `Promise.resolve`,
//! `reject`, `all`, `allSettled`, `any`, `race`, `withResolvers` and `try` are typed,
//! `all` and `allSettled` place by place. Judged here too: a promise that is
//! discarded (E113), an `async` function that never awaits (E118), `new Promise`
//! (E127), and `.then`/`.catch`/`.finally` on a promise (E128).

use crate::classes::leading;
use crate::expressions::{element_span_of, nodes, punct, Typer};
use crate::functions::is_spread;
use crate::iteration::awaits;
use crate::members::Owner;
use crate::mutation::arguments;
use sjs_diag::Code;
use sjs_syntax::{Element, NodeKind, NodeRef};
use sjs_types::{Function, Literal, Param, Property, Type, TypeId};

/// Whether the child at `at` of `node` goes where a promise is used: awaited,
/// returned, passed on or stored.
pub(crate) fn consumes(node: NodeRef<'_>, at: usize) -> bool {
    match node.kind() {
        NodeKind::AwaitExpr
        | NodeKind::ReturnStmt
        | NodeKind::ArrowFunction
        | NodeKind::Initializer
        | NodeKind::ArrayLit
        | NodeKind::Property
        | NodeKind::SpreadElement
        | NodeKind::YieldExpr
        | NodeKind::ForOfStmt => true,
        // An argument, or what is assigned: not what is called or assigned to.
        NodeKind::CallExpr | NodeKind::NewExpr | NodeKind::AssignExpr => at > 0,
        // Where the value of the whole goes.
        NodeKind::ParenExpr | NodeKind::CondExpr => {
            let parent = node.parent();
            parent.is_some_and(|parent| {
                let mut parts = parent.children();
                let place = parts.position(
                    |part| matches!(part, Element::Node(inner) if inner.span() == node.span()),
                );
                consumes(parent, place.unwrap_or_default())
            })
        }
        _ => false,
    }
}

/// Whether a node is a function with a body of its own: a declaration, an arrow, a
/// method of a class or of an object literal.
fn has_body(node: NodeRef<'_>) -> bool {
    let method = node.kind() == NodeKind::Property
        && nodes(node).any(|part| part.kind() == NodeKind::Params);
    method
        || matches!(
            node.kind(),
            NodeKind::FunctionDecl
                | NodeKind::ArrowFunction
                | NodeKind::ClassMethod
                | NodeKind::ClassAccessor
        )
}

impl Typer<'_> {
    /// Whether a value of the type `ty` may be a promise.
    fn may_be_promise(&self, ty: TypeId) -> bool {
        let members = self.members_of(ty);
        members
            .iter()
            .any(|member| matches!(self.types.get(*member), Type::Promise(_)))
    }

    /// What awaiting a value of the type `ty` gives: what a promise resolves to,
    /// however deep, what the `then` of a class instance passes on, and anything
    /// else as it is. What a `then` passes on, when that cannot be told, is `unknown`.
    pub(crate) fn awaited(&mut self, ty: TypeId) -> TypeId {
        let mut settled = Vec::new();
        for member in self.members_of(ty) {
            let given = match self.types.get(member).clone() {
                Type::Promise(value) => self.awaited(value),
                Type::Class(class, arguments) => match self.owner(class, &arguments, "then") {
                    Owner::At(owner, bound) if self.has_method(member, "then") => {
                        let shape = self.facts.classes.get(&owner);
                        let resolves = shape.and_then(|shape| shape.resolves);
                        resolves.map_or(TypeId::UNKNOWN, |ty| self.substituted(ty, &bound))
                    }
                    _ => member,
                },
                _ => member,
            };
            settled.push(given);
        }
        self.types.union(settled)
    }

    /// The type of `await operand`; E119 when the operand may be no promise.
    pub(crate) fn await_of(
        &mut self,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let first = self.first_time(node);
        // What is awaited comes last.
        let operand = children.last().copied().unwrap_or(Element::Node(node));
        let ty = self.type_of(operand)?;
        for member in self.members_of(ty) {
            let thenable = self.has_method(member, "then");
            let waits = matches!(self.types.get(member), Type::Promise(_) | Type::Var(_));
            if !waits && !thenable {
                let message = format!(
                    "`await` on a `{}`, which is no promise: await a promise only (SPEC.md §4.1)",
                    self.print_type(member)
                );
                self.report(first, Code::E119, element_span_of(&operand), message);
                break;
            }
        }
        Some(self.awaited(ty))
    }

    /// The type of `Promise.name(…)`, the `Promise` of the ambient library.
    pub(crate) fn promise_call(
        &mut self,
        name: &str,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let passed = arguments(children);
        let first = passed.first().copied();
        let settled = match name {
            "resolve" => match first {
                Some(value) => {
                    let ty = self.type_of(value)?;
                    let ty = self.widened(ty);
                    self.awaited(ty)
                }
                None => TypeId::UNDEFINED,
            },
            "reject" => TypeId::NEVER,
            "try" => {
                let run = first.and_then(|run| self.type_of(run))?;
                let returned = self.types.function(run).map(|run| run.returns)?;
                self.awaited(returned)
            }
            "withResolvers" => return Some(self.resolvers(node)),
            "all" | "allSettled" | "any" | "race" => self.combined(name, first?)?,
            _ => return None,
        };
        Some(self.types.intern(Type::Promise(settled)))
    }

    /// What a `Promise.all`, `allSettled`, `any` or `race` of `given` resolves to.
    fn combined(&mut self, name: &str, given: Element<'_>) -> Option<TypeId> {
        // Place by place, for an array literal and for a tuple.
        let literal = match given {
            Element::Node(literal) if literal.kind() == NodeKind::ArrayLit => {
                let inside = literal.children();
                let elements: Vec<Element<'_>> =
                    inside.filter(|part| punct(Some(part)).is_none()).collect();
                Some(elements).filter(|elements| !elements.iter().any(is_spread))
            }
            _ => None,
        };
        let mut places = Vec::new();
        let mut fixed = true;
        if let Some(elements) = literal {
            for element in elements {
                places.push(self.type_of(element)?);
            }
        } else {
            let ty = self.type_of(given)?;
            if let Type::Tuple(elements) = self.types.get(ty) {
                places.clone_from(elements);
            } else {
                places.push(self.item_of(ty, false)?);
                fixed = false;
            }
        }
        let mut settled = Vec::new();
        for place in places {
            let ty = self.widened(place);
            let ty = self.awaited(ty);
            settled.push(if name == "allSettled" {
                self.outcome(ty)
            } else {
                ty
            });
        }
        Some(match (name, fixed) {
            ("any" | "race", _) => self.types.union(settled),
            (_, true) => self.types.intern(Type::Tuple(settled)),
            (_, false) => {
                let element = self.types.union(settled);
                self.types.intern(Type::Array {
                    element,
                    mutated: false,
                })
            }
        })
    }

    /// How a promise of a `value` ends, as `Promise.allSettled` tells it.
    fn outcome(&mut self, value: TypeId) -> TypeId {
        let status = |typer: &mut Self, status: &str| Property {
            key: "status".to_owned(),
            ty: typer.types.literal(Literal::String(status.to_owned())),
            literal: true,
        };
        let held = |key: &str, ty: TypeId| Property {
            key: key.to_owned(),
            ty,
            literal: false,
        };
        let fulfilled = vec![status(self, "fulfilled"), held("value", value)];
        let rejected = vec![status(self, "rejected"), held("reason", TypeId::UNKNOWN)];
        let fulfilled = self.types.intern(Type::Object(fulfilled));
        let rejected = self.types.intern(Type::Object(rejected));
        self.types.union([fulfilled, rejected])
    }

    /// The type of `Promise.withResolvers()`: a promise with the functions that
    /// settle it. It resolves to what the `resolve` taken from it by a pattern is
    /// called with, and to `unknown` when that cannot be told.
    fn resolvers(&mut self, node: NodeRef<'_>) -> TypeId {
        let declaration = node.parent().and_then(NodeRef::parent);
        let pattern = declaration.into_iter().flat_map(nodes).next();
        let pattern = pattern.filter(|pattern| pattern.kind() == NodeKind::ObjectPattern);
        let mut given = Vec::new();
        for property in pattern.into_iter().flat_map(nodes) {
            let key = property.tokens().first().map(|key| key.span);
            let name = property.tokens().last().map(|name| name.span.byte_start);
            let is_resolve = key.and_then(|key| self.source.slice(&key)) == Some("resolve");
            let declarations = &self.bindings.declarations;
            let bound = name
                .filter(|_| is_resolve)
                .and_then(|name| declarations.iter().position(|d| d.span.byte_start == name));
            let calls = bound.and_then(|bound| self.called.get(&bound).cloned());
            for call in calls.unwrap_or_default() {
                let parts: Vec<Element<'_>> = call.children().collect();
                let value = arguments(&parts).first().copied();
                let ty = value.and_then(|value| self.type_of(value));
                given.extend(ty.map(|ty| self.loosened(ty)));
            }
        }
        let value = if given.is_empty() {
            TypeId::UNKNOWN
        } else {
            self.types.union(given)
        };
        let settles = |typer: &mut Self, taken: TypeId| {
            typer.types.intern(Type::Function(Function {
                type_params: Vec::new(),
                params: vec![Param {
                    ty: taken,
                    optional: false,
                }],
                rest: None,
                returns: TypeId::UNDEFINED,
            }))
        };
        let property = |key: &str, ty: TypeId| Property {
            key: key.to_owned(),
            ty,
            literal: false,
        };
        let promise = self.types.intern(Type::Promise(value));
        let resolve = settles(self, value);
        let reject = settles(self, TypeId::UNKNOWN);
        self.types.intern(Type::Object(vec![
            property("promise", promise),
            property("resolve", resolve),
            property("reject", reject),
        ]))
    }

    /// Judges `node` by the rules of promises: a promise discarded (E113), an
    /// `async` function that never awaits (E118), `new Promise` (E127), a `then`
    /// chain (E128).
    pub(crate) fn promise_rules(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        // Each node is judged here once, whatever else has typed it.
        let first = true;
        match node.kind() {
            NodeKind::ExprStmt => {
                // What is stated and dropped: the expression, or each element of an
                // array that is.
                let stated = children.first().copied();
                let elements: Vec<Element<'_>> = match stated {
                    Some(Element::Node(list)) if list.kind() == NodeKind::ArrayLit => {
                        let inside = list.children();
                        inside.filter(|part| punct(Some(part)).is_none()).collect()
                    }
                    _ => stated.into_iter().collect(),
                };
                for element in elements {
                    self.floating(first, element, "is neither awaited, returned nor stored");
                }
            }
            NodeKind::CallExpr => {
                // Passed for a parameter its function does nothing with.
                let parameters = match children.first() {
                    Some(Element::Token(name)) => {
                        let reference = self.references.get(&name.token().span.byte_start);
                        let declared =
                            reference.and_then(|&at| self.bindings.references[at].declaration);
                        declared.and_then(|declared| self.facts.parameters.get(&declared))
                    }
                    _ => None,
                };
                let parameters = parameters.cloned().unwrap_or_default();
                let passed = arguments(children);
                let spread = passed.iter().any(is_spread);
                for (parameter, argument) in parameters.iter().zip(&passed) {
                    let dropped = parameter.is_some_and(|taken| !self.consumed.contains(&taken));
                    if dropped && !spread {
                        let why = "is passed for a parameter the function never awaits, returns or stores";
                        self.floating(first, *argument, why);
                    }
                }
            }
            NodeKind::NewExpr => {
                let made = children.get(1).filter(|made| self.is_global(made));
                let name = made.map(element_span_of);
                if name.and_then(|name| self.source.slice(&name)) == Some("Promise") {
                    let message = "`new Promise` is removed: write an `async` function, or take the promise and its `resolve` from `Promise.withResolvers()` (SPEC.md §12)";
                    self.report(first, Code::E127, node.span(), message.to_owned());
                }
            }
            NodeKind::MemberExpr => {
                let (key, at) = self.key_of(node);
                let chained = ["then", "catch", "finally"].contains(&key.as_str());
                let receiver = chained.then_some(node);
                let receiver = receiver.and_then(|node| self.receiver(node, false));
                if receiver.is_some_and(|(receiver, _)| self.may_be_promise(receiver)) {
                    let message = format!(
                        "`.{key}` on a promise is removed: `await` it, in a `try` for what it rejects with (SPEC.md §12)"
                    );
                    self.report(first, Code::E128, at, message);
                }
            }
            _ => {}
        }
        self.async_rule(first, node);
    }

    /// E113 for an expression that may be a promise and is dropped.
    fn floating(&mut self, first: bool, dropped: Element<'_>, why: &str) {
        let ty = self.type_of(dropped);
        if ty.is_some_and(|ty| self.may_be_promise(ty)) {
            let message = format!(
                "this promise {why}: `await` it, return it, or pass it to `Promise.all` and the like (SPEC.md §4.4)"
            );
            self.report(first, Code::E113, element_span_of(&dropped), message);
        }
    }

    /// E118 for an `async` function whose own body holds no `await`.
    fn async_rule(&mut self, first: bool, function: NodeRef<'_>) {
        if !has_body(function) {
            return;
        }
        // The words before the name: all of them, for what has none of its own.
        let mut words = leading(function);
        let symbol = nodes(function).any(|part| part.kind() == NodeKind::SymbolKey);
        let named = !symbol
            && matches!(
                function.kind(),
                NodeKind::ClassMethod | NodeKind::ClassAccessor | NodeKind::Property
            );
        if named {
            words.pop();
        }
        let source = self.source;
        let marked = words
            .into_iter()
            .find(|word| source.slice(word) == Some("async"));
        let Some(marked) = marked else {
            return;
        };
        let mut waits = false;
        let mut pending: Vec<NodeRef<'_>> = nodes(function).collect();
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node).filter(|inner| !has_body(*inner)));
            let looped = node.kind() == NodeKind::ForOfStmt && awaits(self, node);
            waits |= node.kind() == NodeKind::AwaitExpr || looped;
        }
        if !waits {
            let message = "this `async` function never awaits: drop `async`, or `await` the promises it makes (SPEC.md §8.1)";
            self.report(first, Code::E118, marked, message.to_owned());
        }
    }
}
