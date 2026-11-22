//! Iteration (SPEC.md §4.7, §3.1). What iterating a value gives: an array its
//! elements, a string strings (its code points), a `Map` its `[key, value]` pairs, a
//! `Set` its members, an iterator or a generator what it yields, an instance of a
//! class with `[Symbol.iterator]()` what that method's result does. `for…of` binds
//! that (`for await` awaits what a sync iterable gives, and takes async ones too), a
//! spread in an array literal holds it, and `Iterator.from` and `Array.from` make an
//! iterator and an array of it. A generator yields the union of its `yield`s, with
//! what a `yield*` delegates to. The helpers of §4.7 are typed on every iterator.
//! What is no iterable in a `for…of` is E138.

use crate::expressions::{element_span_of, punct, Typer};
use crate::functions::is_function;
use crate::mutation::arguments;
use sjs_diag::Code;
use sjs_syntax::{Element, NodeKind, NodeRef, Punct};
use sjs_types::{Builtin, Literal, Type, TypeId};

/// The helpers every iterator has.
const HELPERS: [&str; 11] = [
    "map", "filter", "take", "drop", "flatMap", "reduce", "toArray", "forEach", "some", "every",
    "find",
];

/// What iterating a value of a type gives.
pub(crate) enum Each {
    /// Values of this type.
    Item(TypeId),
    /// Nothing: this member of the type is not iterable.
    Not(TypeId),
    /// Not known yet.
    Unknown,
}

/// A call of an iterator helper.
pub(crate) struct Helped {
    /// The helper.
    name: String,
    /// What the iterator gives.
    item: TypeId,
    /// Whether it is an async iterator.
    asynchronous: bool,
}

/// Whether a loop or a generator is an async one: `for await`, `async function*`.
pub(crate) fn awaits(typer: &Typer<'_>, node: NodeRef<'_>) -> bool {
    node.children().any(|child| match child {
        Element::Token(word) => typer.source.slice(&word.token().span) == Some("await"),
        Element::Node(_) => false,
    })
}

impl Typer<'_> {
    /// What iterating a value of the type `ty` gives; `awaited` for `for await`,
    /// which takes async iterables too and awaits what a sync one gives.
    pub(crate) fn each(&mut self, ty: TypeId, awaited: bool) -> Each {
        self.each_within(ty, awaited, &mut Vec::new())
    }

    /// [`Typer::each`]; `seen` holds the types being asked already, for a class whose
    /// `[Symbol.iterator]()` gives an instance of itself is not followed round.
    fn each_within(&mut self, ty: TypeId, awaited: bool, seen: &mut Vec<TypeId>) -> Each {
        let mut items = Vec::new();
        for member in self.members_of(ty) {
            let item = match self.types.get(member).clone() {
                Type::Array { element, .. } => element,
                Type::Tuple(elements) => self.types.union(elements),
                Type::String | Type::Literal(Literal::String(_)) => TypeId::STRING,
                Type::Builtin(Builtin::Map, entry) => self.types.intern(Type::Tuple(entry)),
                Type::Builtin(Builtin::Set, members) => self.types.union(members),
                Type::Iterator { item, asynchronous }
                | Type::Generator {
                    yields: item,
                    asynchronous,
                    ..
                } if awaited || !asynchronous => {
                    items.push(item);
                    continue;
                }
                Type::Class(..) => {
                    let asynchronous = awaited && !self.lacks(member, "[Symbol.asyncIterator]");
                    let key = if asynchronous {
                        "[Symbol.asyncIterator]"
                    } else {
                        "[Symbol.iterator]"
                    };
                    if self.lacks(member, key) {
                        return Each::Not(member);
                    }
                    let method = self.method_of(member, key);
                    let method = method.and_then(|method| self.types.function(method));
                    let made = method.map(|method| method.returns);
                    let Some(made) = made.filter(|made| !seen.contains(made)) else {
                        return Each::Unknown;
                    };
                    seen.push(made);
                    match self.each_within(made, awaited, seen) {
                        Each::Item(item) => {
                            items.push(item);
                            continue;
                        }
                        other => return other,
                    }
                }
                _ => return Each::Not(member),
            };
            // `for await` awaits what a sync iterable gives.
            let given = self.members_of(item).into_iter();
            let settled = given.map(|given| match self.types.get(given) {
                Type::Promise(value) if awaited => *value,
                _ => given,
            });
            let settled: Vec<TypeId> = settled.collect();
            items.push(self.types.union(settled));
        }
        Each::Item(self.types.union(items))
    }

    /// The type iterating a value of the type `ty` gives, when it gives one.
    pub(crate) fn item_of(&mut self, ty: TypeId, awaited: bool) -> Option<TypeId> {
        match self.each(ty, awaited) {
            Each::Item(item) => Some(item),
            Each::Not(_) | Each::Unknown => None,
        }
    }

    /// What a generator's body yields: the union of its `yield`s without their
    /// literals, a `yield*` giving what the iterable it delegates to does; `never`
    /// for a body that yields nothing. `None` while a `yield` has no type yet.
    pub(crate) fn yields(&mut self, body: Element<'_>, asynchronous: bool) -> Option<TypeId> {
        let mut types = Vec::new();
        let mut pending = vec![body];
        while let Some(element) = pending.pop() {
            let Element::Node(node) = element else {
                continue;
            };
            // Its own: not those of a function inside it.
            let inside = node.children();
            pending.extend(
                inside
                    .filter(|inner| !matches!(inner, Element::Node(inner) if is_function(*inner))),
            );
            if node.kind() != NodeKind::YieldExpr {
                continue;
            }
            let parts: Vec<Element<'_>> = node.children().collect();
            let delegates = punct(parts.get(1)) == Some(Punct::Star);
            let value = parts.get(1 + usize::from(delegates)).copied();
            let ty = match value {
                Some(value) if delegates => {
                    let iterable = self.type_of(value)?;
                    self.item_of(iterable, asynchronous)?
                }
                Some(value) => {
                    let ty = self.type_of(value)?;
                    self.widened(ty)
                }
                None => TypeId::UNDEFINED,
            };
            types.push(ty);
        }
        Some(self.types.union(types))
    }

    /// E138 for a `for…of` over what is no iterable.
    pub(crate) fn loop_rule(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        // The iterable is what comes before the `)` and the body.
        let iterable = children.iter().rev().nth(2).copied();
        let iterable = iterable.filter(|_| node.kind() == NodeKind::ForOfStmt);
        let Some((iterable, ty)) = iterable.and_then(|it| Some((it, self.type_of(it)?))) else {
            return;
        };
        let first = self.first_time(node);
        let awaited = awaits(self, node);
        if let Each::Not(member) = self.each(ty, awaited) {
            let message = format!(
                "a `{}` is not iterable: `for…of` takes an array, a string, a `Map`, a `Set`, an iterator, or an object with `[Symbol.iterator]` (SPEC.md §4.7)",
                self.print_type(member)
            );
            self.report(first, Code::E138, element_span_of(&iterable), message);
        }
    }

    /// The call of an iterator helper `callee` is the callee of, when it is one:
    /// `it.map`, where `it` is an iterator or a generator.
    pub(crate) fn helped(&mut self, callee: &Element<'_>) -> Option<Helped> {
        let Element::Node(target) = *callee else {
            return None;
        };
        if target.kind() != NodeKind::MemberExpr {
            return None;
        }
        let (name, _) = self.key_of(target);
        let (receiver, _) = self.receiver(target, false)?;
        let mut items = Vec::new();
        let mut asynchronous = false;
        for member in self.members_of(receiver) {
            match self.types.get(member) {
                Type::Iterator {
                    item,
                    asynchronous: flag,
                }
                | Type::Generator {
                    yields: item,
                    asynchronous: flag,
                    ..
                } => {
                    items.push(*item);
                    asynchronous |= *flag;
                }
                _ => return None,
            }
        }
        HELPERS.contains(&name.as_str()).then(|| Helped {
            name,
            item: self.types.union(items),
            asynchronous,
        })
    }

    /// What the callback of a helper is passed, place by place: the element and its
    /// place in the iteration, and before them for `reduce` what was reduced so far,
    /// which is what it starts from. `None` for a place nothing is known of.
    pub(crate) fn helper_takes(
        &mut self,
        helped: &Helped,
        passed: &[Element<'_>],
    ) -> Vec<Option<TypeId>> {
        let mut taken = vec![Some(helped.item), Some(TypeId::INT32)];
        if helped.name == "reduce" {
            let start = match passed.get(1) {
                Some(start) => self.type_of(*start).map(|ty| self.loosened(ty)),
                None => Some(helped.item),
            };
            taken.insert(0, start);
        }
        taken
    }

    /// The type of a call of an iterator helper.
    pub(crate) fn helper_call(
        &mut self,
        helped: &Helped,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let passed = arguments(children);
        let item = helped.item;
        let asynchronous = helped.asynchronous;
        // What the callback returns.
        let callback = passed.first().and_then(|callback| self.type_of(*callback));
        let function = callback.and_then(|callback| self.types.function(callback));
        let returned = function.map(|function| function.returns);
        // What ends the iteration gives its result at once, or in time.
        let result = match helped.name.as_str() {
            "map" => return returned.map(|item| self.iterator(item, asynchronous)),
            "filter" | "take" | "drop" => return Some(self.iterator(item, asynchronous)),
            "flatMap" => {
                let inner = returned.and_then(|inner| self.item_of(inner, false))?;
                return Some(self.iterator(inner, asynchronous));
            }
            "reduce" => {
                let start = self.helper_takes(helped, &passed)[0]?;
                self.types.union([start, returned?])
            }
            "toArray" => self.types.intern(Type::Array {
                element: item,
                mutated: false,
            }),
            "forEach" => TypeId::UNDEFINED,
            "some" | "every" => TypeId::BOOLEAN,
            _ => self.types.union([item, TypeId::UNDEFINED]),
        };
        Some(if asynchronous {
            self.types.intern(Type::Promise(result))
        } else {
            result
        })
    }

    /// An iterator over values of the type `item`.
    pub(crate) fn iterator(&mut self, item: TypeId, asynchronous: bool) -> TypeId {
        self.types.intern(Type::Iterator { item, asynchronous })
    }
}
