//! Contextual typing of callbacks (SPEC.md §3.2, "bidirectional"). An arrow that is a
//! value takes the types of its parameters from what it is expected to be, where
//! something says: passed for a parameter its function calls, it is given what those
//! calls pass; passed or assigned where a function type is taken, what that type
//! takes; returned beside other functions, what they take. A parameter nothing is
//! ever passed for is E136; one nothing says anything of is typed from the body alone.

use crate::expressions::{element_span_of, nodes, Typer};
use crate::functions::{is_function, is_spread, returns_of, Flavor};
use crate::mutation::arguments;
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, NodeKind, NodeRef};
use sjs_types::{Function, Param, Type, TypeId};

/// What is passed at one place of an arrow.
#[derive(Default)]
struct Place {
    /// The types of what is passed.
    types: Vec<TypeId>,
    /// Where that is said, each with a note.
    origins: Vec<(Span, String)>,
}

/// What an arrow is expected to be.
struct Expected {
    /// What it is passed, place by place.
    places: Vec<Place>,
    /// Whether more may be passed than that: by a spread, or to a rest.
    open: bool,
}

/// Whether an arrow is a function declared by name: `const f = () => …`, which is
/// typed from its calls.
fn declared(arrow: NodeRef<'_>) -> bool {
    let holder = arrow
        .parent()
        .filter(|parent| parent.kind() == NodeKind::Initializer);
    let declaration = holder.and_then(NodeRef::parent);
    declaration.is_some_and(|declaration| {
        matches!(declaration.kind(), NodeKind::ConstDecl | NodeKind::LetDecl)
    })
}

impl Typer<'_> {
    /// The type of an arrow that is a value. One inside another is typed after it,
    /// for it may name the other's parameters; one asked for while it is being typed
    /// has no type.
    pub(crate) fn arrow(&mut self, node: NodeRef<'_>) -> Option<TypeId> {
        let mut around = std::iter::successors(node.parent(), |inner| inner.parent());
        if let Some(around) = around.find(|around| around.kind() == NodeKind::ArrowFunction) {
            self.arrow(around);
        }
        let start = node.span().byte_start;
        if declared(node) {
            return None;
        }
        if let Some(typed) = self.arrows.get(&start) {
            return *typed;
        }
        self.arrows.insert(start, None);
        let typed = self.contextual(node);
        self.arrows.insert(start, typed);
        typed
    }

    /// The type of an arrow: each parameter what the arrow is expected to be passed
    /// there, or what the body alone says of it.
    fn contextual(&mut self, node: NodeRef<'_>) -> Option<TypeId> {
        let expected = self.expected(node);
        // An arrow begins with its parameters.
        let list: Vec<NodeRef<'_>> = nodes(node).take(1).flat_map(nodes).collect();
        let mut params = Vec::new();
        let mut type_params = Vec::new();
        for (place, parameter) in list.into_iter().enumerate() {
            let inside: Vec<Element<'_>> = parameter.children().collect();
            let name = match inside.as_slice() {
                [Element::Token(name)] => Some(name.token().span.byte_start),
                _ => None,
            };
            let declarations = &self.bindings.declarations;
            let declaration =
                name.and_then(|name| declarations.iter().position(|d| d.span.byte_start == name))?;
            let passed = expected.as_ref().map(|expected| &expected.places);
            let closed = expected.as_ref().is_some_and(|expected| !expected.open);
            if let Some(passed) = passed.filter(|passed| closed && place >= passed.len()) {
                let message = format!(
                    "nothing is ever passed for `{}`: this function is given {} argument(s) (SPEC.md §4.4)",
                    self.bindings.declarations[declaration].name,
                    passed.len()
                );
                let related: Vec<(Span, String)> = passed
                    .iter()
                    .flat_map(|place| place.origins.clone())
                    .collect();
                self.report_related(Code::E136, parameter.span(), message, &related);
                return None;
            }
            let told = passed
                .and_then(|passed| passed.get(place))
                .filter(|told| !told.types.is_empty());
            let ty = if let Some(told) = told {
                self.origins.insert(declaration, told.origins.clone());
                self.types.union(told.types.clone())
            } else {
                self.alone(node, declaration, &mut type_params)?
            };
            self.locals.insert(declaration, ty);
            params.push(Param {
                ty,
                optional: false,
            });
        }
        let body = node.children().last();
        let first = node.tokens().first().map(|token| token.span);
        let asynchronous = first.and_then(|span| self.source.slice(&span)) == Some("async");
        let flavor = if asynchronous {
            Flavor::Async
        } else {
            Flavor::Plain
        };
        let returns = body.and_then(|body| self.returned_from(body, flavor))?;
        Some(self.types.intern(Type::Function(Function {
            type_params,
            params,
            rest: None,
            returns,
        })))
    }

    /// What an arrow is expected to be, by where it is written.
    fn expected(&mut self, arrow: NodeRef<'_>) -> Option<Expected> {
        let parent = arrow.parent();
        let inside = parent.into_iter().flat_map(NodeRef::children);
        let children: Vec<Element<'_>> = inside.collect();
        let me = |element: &Element<'_>| element_span_of(element) == arrow.span();
        match parent.map(NodeRef::kind) {
            Some(NodeKind::CallExpr) => {
                let passed = arguments(&children);
                // With a spread, which argument is which is not known.
                let place = passed.iter().position(me);
                let place = place.filter(|_| !passed.iter().any(is_spread))?;
                let callee = children[0];
                let parameter = self.parameter_of(&callee, place);
                let calls = parameter.and_then(|parameter| self.called.get(&parameter));
                if let Some(calls) = calls.cloned() {
                    return Some(self.as_called(&callee, &passed, &calls));
                }
                let function = self.type_of(callee)?;
                let mut taken = Vec::new();
                for member in self.members_of(function) {
                    if let Type::Function(function) = self.types.get(member) {
                        taken.extend(function.params.get(place).map(|param| param.ty));
                    }
                }
                let taken = self.types.union(taken);
                self.as_typed(taken, element_span_of(&callee))
            }
            Some(NodeKind::AssignExpr) => {
                let target = children[0];
                let held = self.type_of(target)?;
                self.as_typed(held, element_span_of(&target))
            }
            Some(NodeKind::ReturnStmt) => {
                // Beside the other functions its function returns.
                let mut around = std::iter::successors(parent, |inner| inner.parent());
                let function = around.find(|around| is_function(*around));
                let body = function.and_then(|function| nodes(function).last());
                let mut beside = Vec::new();
                let mut at = arrow.span();
                for (_, value) in body.into_iter().flat_map(returns_of) {
                    // The arrow itself, being typed, has no type to give.
                    let ty = value.and_then(|other| self.type_of(other));
                    if let Some((other, ty)) = value.zip(ty) {
                        beside.push(ty);
                        at = element_span_of(&other);
                    }
                }
                let beside = self.types.union(beside);
                self.as_typed(beside, at)
            }
            _ => None,
        }
    }

    /// What an arrow is expected to be where a value of the type `slot` is taken:
    /// each parameter what the functions of that type take at its place. `None` for
    /// a type that holds no function, or only generic ones.
    fn as_typed(&mut self, slot: TypeId, at: Span) -> Option<Expected> {
        let mut places: Vec<Place> = Vec::new();
        let mut open = false;
        let mut any = false;
        for member in self.members_of(slot) {
            let Type::Function(function) = self.types.get(member).clone() else {
                continue;
            };
            if !function.type_params.is_empty() {
                continue;
            }
            any = true;
            open |= function.rest.is_some();
            for (place, param) in function.params.iter().enumerate() {
                if places.len() <= place {
                    places.push(Place::default());
                }
                places[place].types.push(param.ty);
            }
        }
        for place in &mut places {
            let taken = self.types.union(place.types.clone());
            let note = format!("expected to take a `{}` here", self.print_type(taken));
            place.origins.push((at, note));
        }
        any.then_some(Expected { places, open })
    }

    /// What an arrow is expected to be when it is passed for a parameter that the
    /// function `callee` calls, at `calls`: each parameter what those calls pass at
    /// its place, the type parameters of `callee` being what this call of it, which
    /// passes `passed`, makes them.
    fn as_called(
        &mut self,
        callee: &Element<'_>,
        passed: &[Element<'_>],
        calls: &[NodeRef<'_>],
    ) -> Expected {
        // What the other arguments say the type parameters are.
        let mut pairs = Vec::new();
        for (place, argument) in passed.iter().enumerate() {
            let parameter = self.parameter_of(callee, place);
            let pattern =
                parameter.and_then(|parameter| self.facts.declarations.get(&parameter).copied());
            // The arrow itself, being typed, has no type to give.
            let given = self.type_of(*argument);
            pairs.extend(pattern.zip(given));
        }
        let bound = self.bound(&pairs);
        let mut places: Vec<Place> = Vec::new();
        let mut open = false;
        // In the order of the text, whatever order the calls were found in.
        let mut calls = calls.to_vec();
        calls.sort_by_key(|call| call.span().byte_start);
        for call in calls {
            let children: Vec<Element<'_>> = call.children().collect();
            let given = arguments(&children);
            if given.iter().any(is_spread) {
                open = true;
                continue;
            }
            for (place, argument) in given.iter().enumerate() {
                if places.len() <= place {
                    places.push(Place::default());
                }
                let ty = self.type_of(*argument).map(|ty| self.loosened(ty));
                let ty = ty.map(|ty| self.substituted(ty, &bound));
                // One that is still a type parameter says nothing yet.
                let mut free = Vec::new();
                if let Some(ty) = ty {
                    self.variables(ty, &mut free);
                }
                if let Some(ty) = ty.filter(|_| free.is_empty()) {
                    let note = format!("passed a `{}` here", self.print_type(ty));
                    places[place].types.push(ty);
                    places[place]
                        .origins
                        .push((element_span_of(argument), note));
                }
            }
        }
        Expected { places, open }
    }
}
