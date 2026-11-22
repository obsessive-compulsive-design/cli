//! The types of bindings (SPEC.md §3.2): a `const` has the type of its value; a `let`
//! the union of every value assigned to it, without their literals; a name a pattern
//! declares, what the pattern takes from the value it is given.

use crate::classes::{classes_in, Class};
use crate::expressions::{element_span_of, nodes, punct, Facts, Typer};
use crate::functions::{declared_in, Call, Declared, Passed, Site};
use crate::generics::Says;
use crate::mutation::{arguments, settled, uses_of, Fill};
use sjs_bind::{Bindings, DeclKind};
use sjs_diag::SourceFile;
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{Builtin, ClassId, Property, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

/// How many times the types of the bindings are worked out anew from one another
/// before a binding whose type still changes is given up on (`let x = 1; x = [x]`).
const ROUNDS: usize = 8;

/// One step from a value into what a pattern takes from it.
#[derive(Clone)]
enum Step<'t> {
    /// `{ key }`, with its default.
    Key(String, Option<Element<'t>>),
    /// The element at a place of `[…]`, with its default.
    Index(usize, Option<Element<'t>>),
    /// `{ …rest }`, after these keys.
    RestOfKeys(Vec<String>),
    /// `[ …rest ]`, from this place.
    RestFrom(usize),
    /// What iterating the value gives: the binding of a `for…of`, of a `for await`
    /// when the flag is set.
    Each(bool),
}

/// Something a binding is given.
enum Given<'t> {
    /// The value it is declared with, through the pattern that declares it.
    Declared(Element<'t>, Vec<Step<'t>>),
    /// `name = value`.
    Assigned(Element<'t>),
    /// `name ??= value`.
    Defaulted(Element<'t>),
    /// `name += value` and the like; whether it is `+=`.
    Combined(bool, Element<'t>),
    /// `name++`, `name--`.
    Stepped,
}

struct Collector<'a, 't> {
    source: &'a SourceFile,
    bindings: &'a Bindings,
    /// The declaration of each declared name, by where the name begins.
    declared_at: BTreeMap<usize, usize>,
    /// The reference each name is, by where the name begins.
    referenced_at: BTreeMap<usize, usize>,
    given: BTreeMap<usize, Vec<Given<'t>>>,
    /// The writes that are accounted for, by where the name begins.
    handled: BTreeSet<usize>,
}

fn value_of(node: NodeRef<'_>) -> Option<Element<'_>> {
    let initializer = nodes(node).find(|inner| inner.kind() == NodeKind::Initializer);
    initializer.and_then(|initializer| initializer.children().last())
}

impl<'t> Collector<'_, 't> {
    /// Records what `target`, a name or a pattern, takes from `value` by `path`.
    fn declare(&mut self, target: Element<'t>, value: Element<'t>, path: &[Step<'t>]) {
        let pattern = match target {
            Element::Token(token) => {
                let declaration = self.declared_at.get(&token.token().span.byte_start);
                declaration.copied().into_iter().for_each(|at| {
                    let given = Given::Declared(value, path.to_vec());
                    self.given.entry(at).or_default().push(given);
                });
                return;
            }
            Element::Node(pattern) => pattern,
        };
        let step = |step: Step<'t>| {
            let mut longer = path.to_vec();
            longer.push(step);
            longer
        };
        let mut keys: Vec<String> = Vec::new();
        let mut place = 0;
        for child in pattern.children() {
            let part = match child {
                Element::Token(_) => {
                    place += usize::from(punct(Some(&child)) == Some(Punct::Comma));
                    continue;
                }
                Element::Node(part) => part,
            };
            let inside: Vec<Element<'t>> = part.children().collect();
            let default = value_of(part);
            // What the part binds: the last thing in it but its default.
            let bound = inside
                .iter()
                .rev()
                .find(|inner| !matches!(inner, Element::Node(node) if node.kind() == NodeKind::Initializer));
            // The key of a property is its first token.
            let key = part.tokens().first().map(|key| key.span);
            let key = key
                .and_then(|span| self.source.slice(&span))
                .unwrap_or_default();
            let next = match (pattern.kind(), part.kind()) {
                (NodeKind::ObjectPattern, NodeKind::PatternProperty) => {
                    keys.push(key.to_owned());
                    Step::Key(key.to_owned(), default)
                }
                (NodeKind::ObjectPattern, _) => Step::RestOfKeys(keys.clone()),
                (_, NodeKind::PatternElement) => Step::Index(place, default),
                _ => Step::RestFrom(place),
            };
            let path = step(next);
            bound
                .copied()
                .into_iter()
                .for_each(|bound| self.declare(bound, value, &path));
        }
    }

    /// Finds every declaration with a value and every assignment to a name.
    fn collect(&mut self, root: NodeRef<'t>) {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node));
            let children: Vec<Element<'t>> = node.children().collect();
            let name = |child: &Element<'t>| match child {
                Element::Token(name) if name.token().kind == TokenKind::Ident => {
                    Some(name.token().span.byte_start)
                }
                _ => None,
            };
            // The name written: the target of `name = …`, the one name of `name++`.
            let written = match (node.kind(), children.as_slice()) {
                (NodeKind::UpdateExpr | NodeKind::ForUpdate, [one, other]) => {
                    name(one).or(name(other))
                }
                _ => children.first().and_then(name),
            };
            let declared = |start: usize| {
                let reference = self.referenced_at.get(&start);
                reference.and_then(|&at| self.bindings.references[at].declaration)
            };
            let declaration = written.and_then(declared);
            match node.kind() {
                NodeKind::ConstDecl | NodeKind::LetDecl => {
                    let bound = children.get(1).copied();
                    if let Some((bound, value)) = bound.zip(value_of(node)) {
                        self.declare(bound, value, &[]);
                    }
                }
                NodeKind::ForOfStmt => {
                    // The iterable is what comes before the `)` and the body.
                    let iterable = children.iter().rev().nth(2).copied();
                    // What is declared comes first.
                    let declaration = nodes(node).next();
                    let bound = declaration.and_then(|declaration| declaration.children().nth(1));
                    let awaited = children.iter().any(|child| match child {
                        Element::Token(word) => {
                            self.source.slice(&word.token().span) == Some("await")
                        }
                        Element::Node(_) => false,
                    });
                    let each = [Step::Each(awaited)];
                    let declared = bound.zip(iterable).into_iter();
                    declared.for_each(|(bound, iterable)| self.declare(bound, iterable, &each));
                }
                NodeKind::AssignExpr | NodeKind::UpdateExpr | NodeKind::ForUpdate => {
                    let value = children.last().copied();
                    let operator = children.iter().find_map(|child| punct(Some(child)));
                    let given = match operator {
                        Some(Punct::Eq) => value.map(Given::Assigned),
                        Some(Punct::QuestionQuestionEq) => value.map(Given::Defaulted),
                        Some(Punct::PlusPlus | Punct::MinusMinus) => Some(Given::Stepped),
                        operator => {
                            let plus = operator == Some(Punct::PlusEq);
                            value.map(|value| Given::Combined(plus, value))
                        }
                    };
                    if let Some(((at, given), start)) = declaration.zip(given).zip(written) {
                        self.given.entry(at).or_default().push(given);
                        self.handled.insert(start);
                    }
                }
                _ => {}
            }
        }
    }
}

/// What a step takes from a value of type `from`.
fn taken(typer: &mut Typer<'_>, from: TypeId, step: &Step<'_>) -> Option<TypeId> {
    if let Step::Each(awaited) = step {
        return typer.item_of(from, *awaited);
    }
    let (found, default) = match (typer.types().get(from).clone(), step) {
        (Type::Object(properties), Step::Key(key, default)) => {
            let property = properties.iter().find(|property| &property.key == key);
            (property.map(|property| property.ty), default)
        }
        (Type::Class(..), Step::Key(key, default)) => (typer.read(from, key), default),
        (Type::Dictionary(value), Step::Key(_, default)) => (
            Some(typer.types_mut().union([value, TypeId::UNDEFINED])),
            default,
        ),
        (Type::Tuple(elements), Step::Index(place, default)) => (
            Some(elements.get(*place).copied().unwrap_or(TypeId::UNDEFINED)),
            default,
        ),
        (Type::Array { element, .. }, Step::Index(_, default)) => (
            Some(typer.types_mut().union([element, TypeId::UNDEFINED])),
            default,
        ),
        (Type::Object(properties), Step::RestOfKeys(keys)) => {
            let rest: Vec<Property> = properties
                .into_iter()
                .filter(|property| !keys.contains(&property.key))
                .collect();
            return Some(typer.types_mut().intern(Type::Object(rest)));
        }
        (Type::Tuple(elements), Step::RestFrom(place)) => {
            let rest: Vec<TypeId> = elements.into_iter().skip(*place).collect();
            return Some(typer.types_mut().intern(Type::Tuple(rest)));
        }
        (Type::Array { element, .. }, Step::RestFrom(_)) => {
            let rest = Type::Array {
                element,
                mutated: false,
            };
            return Some(typer.types_mut().intern(rest));
        }
        _ => return None,
    };
    let found = found?;
    // A default stands in for `undefined` only: where none can come, it is dead.
    let present = typer.present(found);
    match default {
        Some(default) if present != found => {
            let default = typer.type_of(*default)?;
            Some(typer.types_mut().union([present, default]))
        }
        _ => Some(found),
    }
}

/// What holds a value: how the binding it is given to is declared and used.
struct Holder<'a, 't> {
    /// Whether it is a `const`.
    constant: bool,
    /// Whether the program mutates what it holds.
    mutated: bool,
    /// What is put into the collection it holds.
    fills: &'a [Fill<'t>],
}

/// An empty collection, which takes its type from what is put into it.
enum Empty {
    Array,
    Map,
    Set,
}

/// Which empty collection `value` is, if it is one: `[]`, `new Map()`, `new Set()`.
pub(crate) fn empty_collection(typer: &Typer<'_>, value: &Element<'_>) -> Option<&'static str> {
    let Element::Node(node) = value else {
        return None;
    };
    let children: Vec<Element<'_>> = node.children().collect();
    let has_parts = children.iter().any(|child| punct(Some(child)).is_none());
    match (node.kind(), children.get(1)) {
        (NodeKind::ArrayLit, _) => (!has_parts).then_some("[]"),
        (NodeKind::NewExpr, Some(Element::Token(class))) => {
            // `Map` and `Set` of the ambient library, not names the file declares.
            let reference = typer.references.get(&class.token().span.byte_start);
            let global =
                reference.is_some_and(|&at| typer.bindings.references[at].declaration.is_none());
            let name = typer.source.slice(&class.token().span).unwrap_or_default();
            let empty = arguments(&children).is_empty() && global;
            match name {
                "Map" if empty => Some("new Map()"),
                "Set" if empty => Some("new Set()"),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The type of a value where a binding holds it. An empty collection is typed by
/// what is put into it; an array literal is a tuple for a `const` nothing mutates and
/// else an array, mutated or not as its binding is; any other array is mutated when
/// its binding is.
fn held(typer: &mut Typer<'_>, value: Element<'_>, holder: &Holder<'_, '_>) -> Option<TypeId> {
    let empty = empty_collection(typer, &value).map(|empty| match empty {
        "[]" => Empty::Array,
        "new Map()" => Empty::Map,
        _ => Empty::Set,
    });
    let all = |typer: &mut Typer<'_>, elements: Vec<Element<'_>>| -> Option<TypeId> {
        let mut found = Vec::new();
        for element in elements {
            let ty = typer.type_of(element)?;
            found.push(typer.loosened(ty));
        }
        // Nothing put in: no type.
        (!found.is_empty()).then(|| typer.types_mut().union(found))
    };
    if let Some(empty) = empty {
        let mut elements = Vec::new();
        let (mut keys, mut values, mut members) = (Vec::new(), Vec::new(), Vec::new());
        for fill in holder.fills {
            match fill {
                Fill::Elements(filled) => elements.extend(filled.iter().copied()),
                Fill::Entry(key, value) => {
                    keys.push(*key);
                    values.push(*value);
                }
                Fill::Member(member) => members.push(*member),
            }
        }
        let ty = match empty {
            Empty::Array => Type::Array {
                element: all(typer, elements)?,
                mutated: true,
            },
            Empty::Map => Type::Builtin(Builtin::Map, vec![all(typer, keys)?, all(typer, values)?]),
            Empty::Set => Type::Builtin(Builtin::Set, vec![all(typer, members)?]),
        };
        return Some(typer.types_mut().intern(ty));
    }
    // A function declared by name is typed from its calls, not as a value.
    if matches!(value, Element::Node(node) if node.kind() == NodeKind::ArrowFunction) {
        return None;
    }
    let literal = match value {
        Element::Node(node) if node.kind() == NodeKind::ArrayLit => Some(node),
        _ => None,
    };
    let elements: Option<Vec<Element<'_>>> = literal.map(|literal| {
        literal
            .children()
            .filter(|child| punct(Some(child)).is_none())
            .collect()
    });
    let spreads = |element: &Element<'_>| matches!(element, Element::Node(node) if node.kind() == NodeKind::SpreadElement);
    let Some(elements) = elements.filter(|elements| !elements.iter().any(spreads)) else {
        let ty = typer.type_of(value)?;
        return Some(if holder.mutated {
            as_mutated(typer, ty)
        } else {
            ty
        });
    };
    let mut found = Vec::new();
    for element in elements {
        found.push(typer.type_of(element)?);
    }
    let ty = if holder.constant && !holder.mutated {
        Type::Tuple(found)
    } else {
        let element = typer.types_mut().union(found);
        Type::Array {
            element: typer.widened(element),
            mutated: holder.mutated,
        }
    };
    Some(typer.types_mut().intern(ty))
}

/// A type whose arrays are mutated ones.
fn as_mutated(typer: &mut Typer<'_>, id: TypeId) -> TypeId {
    let members: Vec<TypeId> = match typer.types().get(id) {
        Type::Union(members) => members.clone(),
        _ => vec![id],
    };
    let mut mutated = Vec::new();
    for member in members {
        let ty = match typer.types().get(member).clone() {
            Type::Array { element, .. } => typer.types_mut().intern(Type::Array {
                element,
                mutated: true,
            }),
            _ => member,
        };
        mutated.push(ty);
    }
    typer.types_mut().union(mutated)
}

/// The type one thing gives a binding whose type so far is `own`.
fn contributed(
    typer: &mut Typer<'_>,
    given: &Given<'_>,
    holder: &Holder<'_, '_>,
    own: Option<TypeId>,
) -> Option<TypeId> {
    // A `let` and an object written into hold no literal types.
    let loose = !holder.constant || holder.mutated;
    match given {
        Given::Declared(value, path) if path.is_empty() => {
            let ty = held(typer, *value, holder);
            ty.map(|ty| if loose { typer.loosened(ty) } else { ty })
        }
        Given::Declared(value, path) => {
            // What a pattern takes from is a value of its own, nothing's to mutate.
            let apart = Holder {
                constant: true,
                mutated: false,
                fills: &[],
            };
            let mut ty = held(typer, *value, &apart);
            for step in path {
                ty = ty.and_then(|from| taken(typer, from, step));
            }
            // What a pattern takes is no literal any more.
            let ty = ty.map(|ty| typer.widened(ty));
            ty.map(|ty| if loose { typer.loosened(ty) } else { ty })
        }
        Given::Assigned(value) => {
            let assigned = Holder {
                constant: false,
                mutated: holder.mutated,
                fills: holder.fills,
            };
            held(typer, *value, &assigned).map(|ty| typer.loosened(ty))
        }
        Given::Defaulted(value) => {
            let value = typer.type_of(*value).map(|ty| typer.loosened(ty));
            let present = own.map(|own| typer.present(own));
            present
                .zip(value)
                .map(|(present, value)| typer.types_mut().union([present, value]))
        }
        Given::Combined(plus, value) => {
            let value = typer.type_of(*value);
            own.zip(value)
                .and_then(|(own, value)| typer.arithmetic(*plus, own, value))
        }
        Given::Stepped => own.and_then(|own| typer.arithmetic(false, own, own)),
    }
}

/// What every binding of the file is given.
fn collected<'a, 't>(
    source: &'a SourceFile,
    tree: &'t SyntaxTree,
    bindings: &'a Bindings,
) -> Collector<'a, 't> {
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
        given: BTreeMap::new(),
        handled: BTreeSet::new(),
    };
    collector.collect(tree.root());
    collector
}

/// Works out one function for a round: its parameters from their call sites, what it
/// returns, and its type once both are known, from `known` into `next`.
fn typed_function(
    judge: &mut Typer<'_>,
    function: &Declared<'_>,
    calls: &[Call<'_>],
    (known, unknown): (&Facts, &BTreeSet<usize>),
    (next, partial): (&mut Facts, &mut BTreeSet<usize>),
) {
    for typed in judge.parameter_types(function, calls, known) {
        let parameter = typed.declaration;
        next.sites.insert(parameter, typed.sites);
        if unknown.contains(&parameter) {
            continue;
        }
        match typed.says {
            Says::Nothing => next.unconstrained.extend([parameter]),
            Says::AnyOrder => next.ordered.extend([parameter]),
            Says::Pinned => next.pinned.extend([parameter]),
            Says::Typed => {}
        }
        // From the calls that have a type so far: one that has none yet may be a
        // call of the function by itself, which the next round types.
        let is_rest = function
            .parameters
            .iter()
            .any(|p| p.rest && p.declaration == Some(parameter));
        let held = typed.ty.map(|ty| {
            if is_rest {
                next.rest.insert(parameter, ty);
                let rest = Type::Array {
                    element: ty,
                    mutated: false,
                };
                judge.types_mut().intern(rest)
            } else {
                ty
            }
        });
        next.declarations.extend(held.map(|held| (parameter, held)));
        if !typed.complete {
            partial.insert(parameter);
        }
    }
    // How many arguments it takes is plain from how it is written.
    let (ty, arity) = judge.signature(function, known, calls);
    next.arity.insert(function.name, arity);
    if unknown.contains(&function.name) {
        return;
    }
    // A body of no type yet keeps what it was taken to return, for the rounds to go
    // on; if it has none in the end, the function is given up on.
    let returned = judge.returned(function);
    if returned.is_none() {
        partial.insert(function.name);
    }
    let returned = returned.or(known.returns.get(&function.name).copied());
    next.returns
        .extend(returned.map(|returned| (function.name, returned)));
    next.declarations.extend(ty.map(|ty| (function.name, ty)));
}

/// What a file's declarations are typed from.
struct Inputs<'a, 't> {
    source: &'a SourceFile,
    tree: &'a SyntaxTree,
    bindings: &'a Bindings,
    facts: &'a Facts,
    given: &'a BTreeMap<usize, Vec<Given<'t>>>,
    followed: &'a [usize],
    mutated: &'a BTreeSet<usize>,
    fills: &'a BTreeMap<usize, Vec<Fill<'t>>>,
    functions: &'a [Declared<'t>],
    calls: &'a [Call<'t>],
    classes: &'a [Class<'t>],
    /// What stands for each method and setter among the declarations.
    keys: &'a BTreeMap<(ClassId, String), usize>,
    /// One past the last of the declarations and of what stands for members.
    extent: usize,
}

/// Types every declaration but the `unknown` ones, round after round until nothing
/// changes: each from what the round before knew. Returns what is known, and the
/// declarations to give up on: a parameter some call of which has no type, and what
/// still changed in the last round (a type that grows without end).
fn rounds(
    inputs: &Inputs<'_, '_>,
    unknown: &BTreeSet<usize>,
    types: &mut Types,
) -> (Facts, BTreeSet<usize>) {
    let bindings = inputs.bindings;
    // Enough rounds for a chain of declarations each known from the next.
    let last = bindings.declarations.len().max(ROUNDS);
    let mut known = inputs.facts.clone();
    // A function that calls itself returns, to begin with, nothing at all.
    let names = inputs
        .functions
        .iter()
        .map(|function| (function.name, TypeId::NEVER));
    known
        .returns
        .extend(names.filter(|(name, _)| !unknown.contains(name)));
    let mut dropped = BTreeSet::new();
    for round in 0..=last {
        let mut next = inputs.facts.clone();
        let mut partial = BTreeSet::new();
        let mut judge = Typer::new(inputs.source, inputs.tree, bindings, &known, types);
        for &at in inputs.followed.iter().filter(|at| !unknown.contains(at)) {
            let holder = Holder {
                constant: bindings.declarations[at].kind == DeclKind::Const,
                mutated: inputs.mutated.contains(&at),
                fills: inputs.fills.get(&at).map_or(&[][..], Vec::as_slice),
            };
            let own = known.declarations.get(&at).copied();
            // In the first round a binding is what it is declared with.
            let counted = inputs.given[&at]
                .iter()
                .filter(|given| round > 0 || matches!(given, Given::Declared(..)));
            let parts: Option<Vec<TypeId>> = counted
                .map(|given| contributed(&mut judge, given, &holder, own))
                .collect();
            // A binding given something of no type yet has none yet either.
            let ty = parts.map(|parts| judge.types_mut().union(parts));
            next.declarations.extend(ty.map(|ty| (at, ty)));
        }
        // The calls of the methods are found by what their receivers are so far.
        let mut calls = inputs.calls.to_vec();
        calls.extend(judge.member_calls(inputs.tree.root(), inputs.keys));
        for function in inputs.functions {
            typed_function(
                &mut judge,
                function,
                &calls,
                (&known, unknown),
                (&mut next, &mut partial),
            );
        }
        judge.classes(inputs.classes, (&known, unknown), (&mut next, &mut partial));
        let all = 0..inputs.extent;
        let mut changed: Vec<usize> = all
            .filter(|at| {
                next.declarations.get(at) != known.declarations.get(at)
                    || next.returns.get(at) != known.returns.get(at)
            })
            .collect();
        // A class changes with what is known of it beyond its type.
        let reshaped =
            |class: &&Class<'_>| next.classes.get(&class.id) != known.classes.get(&class.id);
        let reshaped = inputs.classes.iter().filter(reshaped);
        changed.extend(reshaped.map(|class| class.declaration));
        known = next;
        if changed.is_empty() || round == last {
            // Still changing in the last round: a type that grows without end.
            dropped = partial;
            dropped.extend(changed);
            break;
        }
    }
    (known, dropped)
}

/// Records in `known` what the file passes to the functions it imports, for the
/// other files of the program.
fn outgoing(
    source: &SourceFile,
    tree: &SyntaxTree,
    bindings: &Bindings,
    known: &mut Facts,
    types: &mut Types,
    calls: &[Call<'_>],
) {
    let mut passed = Vec::new();
    let facts = known.clone();
    let mut judge = Typer::new(source, tree, bindings, &facts, types);
    let imported = calls
        .iter()
        .filter(|call| bindings.declarations[call.callee].kind == DeclKind::Import);
    for call in imported {
        for (position, argument) in call.passed.iter().enumerate() {
            let ty = judge.type_of(*argument).map(|ty| judge.loosened(ty));
            passed.extend(ty.map(|ty| Passed {
                callee: call.callee,
                position,
                site: Site {
                    path: source.path().to_owned(),
                    span: element_span_of(argument),
                    ty,
                },
            }));
        }
    }
    known.passed = passed;
}

/// The type of each `const` and `let` of the file that has a value, added to `facts`:
/// the declarations whose type can be told from the expressions [`Typer`] types and
/// from one another.
#[must_use]
pub fn infer_bindings(
    source: &SourceFile,
    tree: &SyntaxTree,
    bindings: &Bindings,
    facts: &Facts,
    types: &mut Types,
) -> Facts {
    let mut collector = collected(source, tree, bindings);
    // What the file mutates, with what `facts` says is mutated from elsewhere.
    let uses = uses_of(source, tree, bindings);
    let outright: BTreeSet<usize> = uses.direct.union(&facts.mutated).copied().collect();
    let parameter = |callee: usize, place: usize| {
        let parameters = uses.functions.get(&callee);
        parameters.and_then(|parameters| parameters.get(place).copied().flatten())
    };
    let mutated = settled(outright, &uses.calls, &parameter);
    // A binding written in a way that is not followed here has no type yet.
    let unfollowed = bindings.references.iter().filter(|reference| {
        reference.write && !collector.handled.contains(&reference.span.byte_start)
    });
    let mut unknown: BTreeSet<usize> = unfollowed
        .filter_map(|reference| reference.declaration)
        .collect();
    // So has one that is assigned without a value it is declared with (a loop's).
    let undeclared = collector.given.iter().filter(|(_, given)| {
        !given
            .iter()
            .any(|given| matches!(given, Given::Declared(..)))
    });
    unknown.extend(undeclared.map(|(at, _)| *at));
    // What `facts` holds already is not worked out again.
    collector
        .given
        .retain(|at, _| !facts.declarations.contains_key(at));
    let followed: Vec<usize> = collector
        .given
        .keys()
        .copied()
        .filter(|at| !unknown.contains(at))
        .collect();
    let (mut functions, mut calls) = declared_in(source, tree, bindings);
    // The classes of the file: each a class of its own, with its static side after
    // it, past every class `facts` knows already.
    let mut facts = facts.clone();
    let known = [
        facts.fields.keys().max(),
        facts.methods.keys().max(),
        facts.to_string.keys().max(),
        facts.class_names.keys().max(),
        facts.classes.keys().max(),
        facts.superclass.keys().max(),
        facts.superclass.values().max(),
        facts.class_of.values().max(),
    ];
    let last = known.into_iter().flatten().max();
    let mut free = last.map_or(0, |last| last.0 + 2);
    for (at, declaration) in bindings.declarations.iter().enumerate() {
        if declaration.kind == DeclKind::Class && !facts.class_of.contains_key(&at) {
            facts.class_of.insert(at, ClassId(free));
            free += 2;
        }
    }
    let collected = classes_in(source, tree, bindings, &facts.class_of);
    functions.extend(collected.functions);
    calls.extend(collected.calls);
    // The parameters of the file's functions: for one passed on to another, and for
    // the other files of the program.
    let listed = |function: &Declared<'_>| -> Vec<Option<usize>> {
        let plain = function
            .parameters
            .iter()
            .filter(|parameter| !parameter.rest);
        plain.map(|parameter| parameter.declaration).collect()
    };
    facts.parameters = functions
        .iter()
        .map(|function| (function.name, listed(function)))
        .collect();
    let facts = &facts;
    let inputs = Inputs {
        source,
        tree,
        bindings,
        facts,
        given: &collector.given,
        followed: &followed,
        mutated: &mutated,
        fills: &uses.fills,
        functions: &functions,
        calls: &calls,
        classes: &collected.classes,
        keys: &collected.keys,
        extent: collected.extent,
    };
    // What cannot be settled is given up on, and the rest is settled again without
    // it, so that nothing is typed from a type that was dropped.
    let mut known = loop {
        let (settled, dropped) = rounds(&inputs, &unknown, types);
        let before = unknown.len();
        unknown.extend(dropped);
        if unknown.len() == before {
            break settled;
        }
    };
    outgoing(source, tree, bindings, &mut known, types, &calls);
    for at in unknown {
        known.declarations.remove(&at);
    }
    known.filled = uses.fills.keys().copied().collect();
    known.mutated = mutated;
    known
}
