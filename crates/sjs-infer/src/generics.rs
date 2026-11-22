//! Generalization and instantiation (SPEC.md §3.2, §4.4). A parameter the body treats
//! opaquely (passes on, returns, stores, compares with `===`) is a type parameter of
//! its function, and a call gives it the type of what is passed: each call its own.
//! What the body does with a parameter otherwise says what it must be: read by place,
//! an array; ordered with `<`, a number, bigint or string; passed to a function that
//! is not generic there, what that function takes.

use crate::classes::{is_constructor, this_key, within};
use crate::expressions::{punct, Typer};
use crate::mutation::arguments;
use sjs_diag::Span;
use sjs_syntax::{Element, NodeKind, NodeRef, Punct};
use sjs_types::{Function, Param, Property, Type, TypeId, VarId};
use std::collections::BTreeMap;

/// How a function's body uses one of its parameters.
#[derive(Default)]
pub(crate) struct Uses<'t> {
    /// Passed as an argument: the callee, and the place.
    pub passed: Vec<(Element<'t>, usize)>,
    /// Read by place: `xs[0]`.
    pub indexed: bool,
    /// Compared with `<`, `>`, `<=` or `>=`: what it is compared with.
    pub ordered: Vec<Element<'t>>,
    /// Used in any way that says what it is: a property read, an operator, a call of
    /// it, a condition, an assignment to it.
    pub constrained: bool,
    /// Stored into something, by a method of it or as a part of it: where the text
    /// of what it is stored into starts.
    pub into: Vec<usize>,
    /// Stored in a field of the instance, `this.key = it`: the keys.
    pub fields: Vec<Span>,
}

/// What is known of a parameter's type by how the body uses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Says {
    /// What the type says.
    Typed,
    /// Nothing says what it is (E112).
    Nothing,
    /// A number, a bigint or a string, whichever what it is ordered against is.
    AnyOrder,
    /// What the functions it is passed to take.
    Pinned,
}

impl Uses<'_> {
    /// Whether it is ordered against something.
    pub fn is_ordered(&self) -> bool {
        !self.ordered.is_empty()
    }

    /// Whether every use is opaque: the parameter may be anything.
    pub fn opaque(&self) -> bool {
        !(self.indexed || self.is_ordered() || self.constrained)
    }
}

/// How the body of `function` uses the declaration `parameter`.
pub(crate) fn uses_in<'t>(typer: &Typer<'_>, function: NodeRef<'t>, parameter: usize) -> Uses<'t> {
    let mut uses = Uses::default();
    for node in within(function) {
        for (at, child) in node.children().enumerate() {
            let Element::Token(token) = child else {
                continue;
            };
            let reference = typer.references.get(&token.token().span.byte_start);
            let declared = reference.and_then(|&at| typer.bindings.references[at].declaration);
            if declared == Some(parameter) {
                used(&mut uses, node, at);
            }
        }
    }
    // What a constructor stores in a field is used as the field is, all over the
    // class: a read of the field is a use of the parameter.
    let keys = uses.fields.iter();
    let keys: Vec<&str> = keys.filter_map(|key| typer.source.slice(key)).collect();
    let body = function.parent().filter(|_| !keys.is_empty());
    for node in body.into_iter().flat_map(within) {
        let key = this_key(&Element::Node(node)).and_then(|key| typer.source.slice(&key));
        let mine = typer.side_of(node) == typer.side_of(function);
        let read = key.is_some_and(|key| keys.contains(&key)) && mine;
        let Some(parent) = node.parent().filter(|_| read) else {
            continue;
        };
        let mut parts = parent.children();
        let place = parts
            .position(|part| matches!(part, Element::Node(inner) if inner.span() == node.span()));
        // Written, the field is stored in again, which says nothing more.
        used(&mut uses, parent, place.unwrap_or_default());
    }
    uses
}

/// Records the use of a parameter as the child at `at` of `node`.
fn used<'t>(uses: &mut Uses<'t>, node: NodeRef<'t>, at: usize) {
    let children: Vec<Element<'t>> = node.children().collect();
    let operator = punct(children.get(1));
    // Where the value of `node` goes, when the parameter is the value of `node`.
    let through = |uses: &mut Uses<'t>| {
        let places = node.parent().into_iter().flat_map(|parent| {
            let children = parent.children().enumerate();
            let me = children.filter(
                |(_, child)| matches!(child, Element::Node(inner) if inner.span() == node.span()),
            );
            me.map(move |(place, _)| (parent, place))
        });
        for (parent, place) in places.take(1).collect::<Vec<_>>() {
            used(uses, parent, place);
        }
    };
    match node.kind() {
        // Returned, stored in a literal or a binding, or a statement of its own.
        NodeKind::ReturnStmt
        | NodeKind::ArrayLit
        | NodeKind::Property
        | NodeKind::Initializer
        | NodeKind::ExprStmt
        | NodeKind::ArrowFunction => {}
        NodeKind::ParenExpr => through(uses),
        NodeKind::CondExpr if at > 0 => through(uses),
        NodeKind::AssignExpr if operator == Some(Punct::Eq) => match this_key(&children[0]) {
            Some(key) => uses.fields.push(key),
            None => uses.into.push(node.span().byte_start),
        },
        NodeKind::BinaryExpr => match operator {
            Some(Punct::EqEqEq | Punct::BangEqEq) => {}
            Some(Punct::Lt | Punct::Gt | Punct::LtEq | Punct::GtEq) => {
                // The other side: the first part or the last.
                let other = if at == 0 { children.len() - 1 } else { 0 };
                uses.ordered.push(children[other]);
            }
            _ => uses.constrained = true,
        },
        NodeKind::IndexExpr if at == 0 => {
            // What is read is used too: as the parameter may be, or it says more.
            let mut element = Uses::default();
            through(&mut element);
            let plain = element.opaque() && element.passed.is_empty() && element.into.is_empty();
            uses.indexed |= plain;
            uses.constrained |= !plain;
        }
        NodeKind::CallExpr | NodeKind::NewExpr => {
            let passed = arguments(&children);
            let me = children[at];
            let place = passed.iter().position(|argument| *argument == me);
            match place {
                Some(place) => {
                    // What is called comes first, or after `new`.
                    let callee = children[usize::from(node.kind() == NodeKind::NewExpr)];
                    uses.passed.push((callee, place));
                    // To a method: what it is a method of may keep it.
                    let method = matches!(callee, Element::Node(callee) if callee.kind() == NodeKind::MemberExpr);
                    uses.into.extend(method.then_some(node.span().byte_start));
                }
                // Called, or made with `new`.
                None => uses.constrained = true,
            }
        }
        _ => uses.constrained = true,
    }
}

impl Typer<'_> {
    /// The type variables free in a type, in the order they are met: not those a
    /// function in it is generic in.
    pub(crate) fn variables(&self, id: TypeId, found: &mut Vec<VarId>) {
        let inside: Vec<TypeId> = match self.types.get(id) {
            Type::Var(var) => {
                if !found.contains(var) {
                    found.push(*var);
                }
                Vec::new()
            }
            Type::Array { element, .. } | Type::Promise(element) => vec![*element],
            Type::Tuple(members)
            | Type::Union(members)
            | Type::Builtin(_, members)
            | Type::Class(_, members) => members.clone(),
            Type::Object(properties) => properties.iter().map(|property| property.ty).collect(),
            Type::Function(function) => {
                let mut inside: Vec<TypeId> =
                    function.params.iter().map(|param| param.ty).collect();
                inside.extend(function.rest);
                inside.push(function.returns);
                let mut free = Vec::new();
                for inner in inside {
                    self.variables(inner, &mut free);
                }
                free.retain(|var| !function.type_params.contains(var) && !found.contains(var));
                found.extend(free);
                Vec::new()
            }
            _ => Vec::new(),
        };
        for inner in inside {
            self.variables(inner, found);
        }
    }

    /// Finds what the type variables of `pattern` are when a value of type `given`
    /// is passed for it: a parameter that is generic is a type variable, or an
    /// array of one.
    fn bind(&self, pattern: TypeId, given: TypeId, bound: &mut BTreeMap<VarId, Vec<TypeId>>) {
        let held: Vec<TypeId> = match (self.types.get(pattern), self.types.get(given)) {
            (Type::Var(var), _) => {
                bound.entry(*var).or_default().push(given);
                Vec::new()
            }
            (Type::Array { .. }, Type::Array { element: held, .. }) => vec![*held],
            (Type::Array { .. }, Type::Tuple(held)) => held.clone(),
            _ => Vec::new(),
        };
        if let Type::Array { element, .. } = self.types.get(pattern) {
            for held in held {
                self.bind(*element, held, bound);
            }
        }
    }

    /// `id` with each type variable replaced by what `bound` says it is; one nothing
    /// is bound to stays.
    pub(crate) fn substituted(&mut self, id: TypeId, bound: &BTreeMap<VarId, TypeId>) -> TypeId {
        let each = |typer: &mut Self, ids: &[TypeId]| -> Vec<TypeId> {
            ids.iter().map(|&id| typer.substituted(id, bound)).collect()
        };
        let ty = match self.types.get(id).clone() {
            Type::Var(var) => return bound.get(&var).copied().unwrap_or(id),
            Type::Array { element, mutated } => Type::Array {
                element: self.substituted(element, bound),
                mutated,
            },
            Type::Promise(element) => Type::Promise(self.substituted(element, bound)),
            Type::Tuple(members) => Type::Tuple(each(self, &members)),
            Type::Union(members) => {
                let members = each(self, &members);
                return self.types.union(members);
            }
            Type::Builtin(builtin, members) => Type::Builtin(builtin, each(self, &members)),
            Type::Class(class, members) => Type::Class(class, each(self, &members)),
            Type::Object(properties) => {
                let replaced = properties.into_iter().map(|property| Property {
                    ty: self.substituted(property.ty, bound),
                    ..property
                });
                Type::Object(replaced.collect())
            }
            Type::Function(function) => {
                // What it is generic in itself stays as it is.
                let mut within = bound.clone();
                for var in &function.type_params {
                    within.remove(var);
                }
                let params = function.params.iter().map(|param| Param {
                    ty: self.substituted(param.ty, &within),
                    optional: param.optional,
                });
                let params = params.collect();
                Type::Function(Function {
                    type_params: function.type_params.clone(),
                    params,
                    rest: function.rest.map(|rest| self.substituted(rest, &within)),
                    returns: self.substituted(function.returns, &within),
                })
            }
            _ => return id,
        };
        self.types.intern(ty)
    }

    /// What each type variable of the first of each pair is when a value of the
    /// second is passed for it: the union of what is passed, without its literals.
    pub(crate) fn bound(&mut self, pairs: &[(TypeId, TypeId)]) -> BTreeMap<VarId, TypeId> {
        let mut found: BTreeMap<VarId, Vec<TypeId>> = BTreeMap::new();
        for &(pattern, given) in pairs {
            let given = self.loosened(given);
            self.bind(pattern, given, &mut found);
        }
        let mut bound = BTreeMap::new();
        for (var, types) in found {
            // A type parameter is no literal: what a tuple holds is widened.
            let types: Vec<TypeId> = types.into_iter().map(|ty| self.widened(ty)).collect();
            bound.insert(var, self.types.union(types));
        }
        bound
    }

    /// What a call of the generic `function` gives when it is passed values of the
    /// types `passed`: its return, with each type parameter the union of what is
    /// passed for it at this call.
    pub(crate) fn instantiated(
        &mut self,
        function: &Function,
        passed: &[Option<TypeId>],
    ) -> Option<TypeId> {
        let pairs = function.params.iter().zip(passed);
        let pairs: Vec<(TypeId, TypeId)> = pairs
            .filter_map(|(param, given)| given.map(|given| (param.ty, given)))
            .collect();
        let bound = self.bound(&pairs);
        // A type parameter nothing is passed for leaves the call without a type.
        let mut needed = Vec::new();
        self.variables(function.returns, &mut needed);
        let told = needed
            .iter()
            .all(|var| bound.contains_key(var) || !function.type_params.contains(var));
        told.then(|| self.substituted(function.returns, &bound))
    }

    /// The type of a parameter of an arrow that is a value, from the arrow's body
    /// alone: a parameter the body treats opaquely is what the functions it is passed
    /// to take, or a type parameter of the arrow, which is added to `type_params`;
    /// one read by place only an array of one; one ordered only a number, bigint or
    /// string. `None` for any other.
    pub(crate) fn alone(
        &mut self,
        node: NodeRef<'_>,
        declaration: usize,
        type_params: &mut Vec<VarId>,
    ) -> Option<TypeId> {
        let uses = uses_in(self, node, declaration);
        let variable = self.fresh_variable(node, type_params);
        if self.escapes(node, &uses) {
            None
        } else if uses.opaque() {
            let pins = self.pinned(declaration, &uses)?;
            Some(if pins.is_empty() {
                type_params.push(variable);
                self.types.intern(Type::Var(variable))
            } else {
                self.types.union(pins)
            })
        } else if uses.indexed && !uses.is_ordered() && !uses.constrained {
            type_params.push(variable);
            let element = self.types.intern(Type::Var(variable));
            Some(self.types.intern(Type::Array {
                element,
                mutated: false,
            }))
        } else if uses.is_ordered() && !uses.indexed && !uses.constrained {
            let (ty, any) = self.ordered_as(&uses);
            self.loose.extend(any.then_some(declaration));
            Some(ty)
        } else {
            None
        }
    }

    /// A type variable for a parameter of the function `node` that is none of
    /// `taken` nor of the variables of what the function's body names that is
    /// declared outside it: those of the functions it is in.
    pub(crate) fn fresh_variable(&self, node: NodeRef<'_>, taken: &[VarId]) -> VarId {
        let mut used: Vec<VarId> = taken.to_vec();
        let (from, to) = (node.span().byte_start, node.span().byte_end);
        for reference in &self.bindings.references {
            let inside = (from..to).contains(&reference.span.byte_start);
            let outside =
                |at: &usize| !(from..to).contains(&self.bindings.declarations[*at].span.byte_start);
            let declaration = reference.declaration.filter(|at| inside && outside(at));
            let ty = declaration.and_then(|declaration| {
                let local = self.locals.get(&declaration);
                local.or(self.facts.declarations.get(&declaration)).copied()
            });
            if let Some(ty) = ty {
                self.variables(ty, &mut used);
            }
        }
        let free = (0..u32::MAX)
            .map(VarId)
            .find(|variable| !used.contains(variable));
        free.unwrap_or(VarId(u32::MAX))
    }

    /// What a parameter that is only ordered is: what it is ordered against, when
    /// that is known to be numbers, bigints or strings; else any of the three, and
    /// the flag says so.
    pub(crate) fn ordered_as(&mut self, uses: &Uses<'_>) -> (TypeId, bool) {
        let mut kinds = Vec::new();
        for other in &uses.ordered {
            let ty = self.type_of(*other).map(|ty| self.widened(ty));
            let kind = match ty {
                Some(TypeId::INT32 | TypeId::NUMBER) => Some(TypeId::NUMBER),
                Some(kind @ (TypeId::BIGINT | TypeId::STRING)) => Some(kind),
                _ => None,
            };
            kinds.extend(kind);
        }
        let any = kinds.is_empty();
        if any {
            kinds = vec![TypeId::NUMBER, TypeId::BIGINT, TypeId::STRING];
        }
        (self.types.union(kinds), any)
    }

    /// Whether an operand is a parameter that is any of a number, a bigint or a
    /// string for want of a call: ordering it against another is no finding.
    pub(crate) fn any_order(&self, operand: &Element<'_>) -> bool {
        let Element::Token(token) = operand else {
            return false;
        };
        let reference = self.references.get(&token.token().span.byte_start);
        let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
        declared.is_some_and(|declared| {
            self.loose.contains(&declared) || self.facts.ordered.contains(&declared)
        })
    }

    /// Whether a parameter of `function` is stored into something that is not
    /// declared in it: then it is no type parameter, for what holds it is one thing
    /// whatever the call.
    pub(crate) fn escapes(&self, function: NodeRef<'_>, uses: &Uses<'_>) -> bool {
        let (from, to) = (function.span().byte_start, function.span().byte_end);
        // What a constructor stores in the instance it makes is that instance's
        // alone; what anything else stores in `this` is not.
        let kept = !uses.fields.is_empty() && !is_constructor(self.source, function);
        kept || uses.into.iter().any(|start| {
            let reference = self.references.get(start);
            let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
            let at = declared.map(|declared| self.bindings.declarations[declared].span.byte_start);
            !at.is_some_and(|at| (from..to).contains(&at))
        })
    }

    /// A function type with each type variable replaced by what `bound` says it is,
    /// generic in `type_params`, and taking and giving any number where it takes or
    /// gives an `int32`: `int32` is inferred, never asked for.
    fn rebuilt(
        &mut self,
        function: &Function,
        bound: &BTreeMap<VarId, TypeId>,
        type_params: Vec<VarId>,
    ) -> TypeId {
        let wider = |typer: &mut Self, ty: TypeId| {
            let ty = typer.substituted(ty, bound);
            typer.any_number(ty)
        };
        let mut params = Vec::new();
        for param in &function.params {
            params.push(Param {
                ty: wider(self, param.ty),
                optional: param.optional,
            });
        }
        let rebuilt = Function {
            params,
            rest: function.rest.map(|rest| wider(self, rest)),
            returns: wider(self, function.returns),
            type_params,
        };
        self.types.intern(Type::Function(rebuilt))
    }

    /// The types of the method `signature` and of the method `like`, as they are
    /// compared: the type parameters of the first named as those of the second are,
    /// place by place (two methods generic alike are then alike), and each taking and
    /// giving any number for an `int32`.
    pub(crate) fn alike(&mut self, signature: TypeId, like: TypeId) -> (TypeId, TypeId) {
        let function = self.types.function(signature).cloned();
        let other = self.types.function(like).cloned();
        function
            .zip(other)
            .map_or((signature, like), |(function, other)| {
                let mut bound = BTreeMap::new();
                for (from, to) in function.type_params.iter().zip(&other.type_params) {
                    bound.insert(*from, self.types.intern(Type::Var(*to)));
                }
                // Generic in more, it stays generic in what is left over.
                let mut named = other.type_params.clone();
                named.extend(function.type_params.iter().skip(named.len()));
                let first = self.rebuilt(&function, &bound, named);
                let nothing = BTreeMap::new();
                let second = self.rebuilt(&other, &nothing, other.type_params.clone());
                (first, second)
            })
    }

    /// The parameter a declared function has at `place`, when `callee` names one.
    pub(crate) fn parameter_of(&self, callee: &Element<'_>, place: usize) -> Option<usize> {
        let Element::Token(token) = callee else {
            return None;
        };
        let reference = self.references.get(&token.token().span.byte_start);
        let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
        let parameters = declared.and_then(|declared| self.facts.parameters.get(&declared));
        parameters.and_then(|parameters| parameters.get(place).copied().flatten())
    }

    /// What the functions the parameter `me` is passed to take at its place, where
    /// they say: a function that is generic there says nothing. `None` while a
    /// function declared here has no type for its parameter yet.
    pub(crate) fn pinned(&mut self, me: usize, uses: &Uses<'_>) -> Option<Vec<TypeId>> {
        let mut pins = Vec::new();
        for (callee, place) in &uses.passed {
            let taken = if let Some(parameter) = self.parameter_of(callee, *place) {
                // Passed on to itself, it says nothing of itself.
                let taken = self.facts.declarations.get(&parameter).copied();
                let taken = taken.filter(|_| parameter != me);
                // Passed for another parameter of its own function, which has
                // no type yet for waiting on this one, it says nothing either.
                let lists = self.facts.parameters.values();
                let together = |list: &Vec<Option<usize>>| {
                    list.contains(&Some(me)) && list.contains(&Some(parameter))
                };
                let beside = lists.into_iter().any(together);
                if taken.is_none() && !beside {
                    return None;
                }
                taken
            } else {
                let callee = self.called_type(callee);
                let function = callee.and_then(|callee| self.types.function(callee));
                function.and_then(|function| function.params.get(*place).map(|param| param.ty))
            };
            let mut variables = Vec::new();
            if let Some(taken) = taken {
                self.variables(taken, &mut variables);
            }
            pins.extend(taken.filter(|_| variables.is_empty()));
        }
        Some(pins)
    }
}
