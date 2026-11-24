//! The built-ins `Object` and `Array` (SPEC.md §6.1). The kept statics of `Object`
//! (`keys`, `values`, `entries`, `fromEntries`, `hasOwn`, `groupBy`, `freeze`,
//! `isFrozen`, `assign`) and of `Array` (`isArray`, `of`, `from`, `fromAsync`), and
//! every ES2025 method of an array, are typed; a callback they take is passed the
//! element, its place and the array. `sort()` and `toSorted()` without a comparator
//! on what is no array of strings is E133; `indexOf`, `lastIndexOf` and `includes`
//! of what the array cannot hold is E138.

use crate::collections::keyed_takes;
use crate::expressions::{element_span_of, Typer};
use crate::functions::is_spread;
use crate::mutation::arguments;
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, NodeKind, NodeRef};
use sjs_types::{Literal, Type, TypeId};

/// The methods of an array.
pub(crate) const ARRAY_METHODS: [&str; 38] = [
    "at",
    "concat",
    "copyWithin",
    "entries",
    "every",
    "fill",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flat",
    "flatMap",
    "forEach",
    "includes",
    "indexOf",
    "join",
    "keys",
    "lastIndexOf",
    "map",
    "pop",
    "push",
    "reduce",
    "reduceRight",
    "reverse",
    "shift",
    "slice",
    "some",
    "sort",
    "splice",
    "toLocaleString",
    "toReversed",
    "toSorted",
    "toSpliced",
    "toString",
    "unshift",
    "values",
    "with",
];

/// The methods that take a callback of an element, its place and the array.
const VISITORS: [&str; 10] = [
    "every",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "some",
];

/// How deep `flat` is followed at most.
const DEPTH: usize = 8;

/// A call of a method of an array.
pub(crate) struct Listed {
    /// The method.
    name: String,
    /// Where it is named.
    at: Span,
    /// What the array holds, without its literals.
    element: TypeId,
    /// The array.
    receiver: TypeId,
    /// Whether a `?.` before it may have found nothing.
    short: bool,
}

impl Typer<'_> {
    /// What a call at the end of a chain gives: with `undefined`, when a `?.` before
    /// it may have found nothing.
    pub(crate) fn after_chain(&mut self, given: TypeId, short: bool) -> TypeId {
        if short {
            self.types.union([given, TypeId::UNDEFINED])
        } else {
            given
        }
    }

    /// An array of `element`s that nothing else holds yet.
    fn list(&mut self, element: TypeId) -> TypeId {
        self.types.intern(Type::Array {
            element,
            mutated: false,
        })
    }

    /// What an array or a tuple holds, without its literals; `None` for any other.
    fn held_by(&mut self, list: TypeId) -> Option<TypeId> {
        let mut held = Vec::new();
        for member in self.members_of(list) {
            match self.types.get(member).clone() {
                Type::Array { element, .. } => held.push(element),
                Type::Tuple(elements) => held.extend(elements),
                _ => return None,
            }
        }
        let held = self.types.union(held);
        Some(self.widened(held))
    }

    /// What a callback passed first in a call returns.
    fn returned_by(&mut self, passed: &[Element<'_>]) -> Option<TypeId> {
        let callback = passed.first().and_then(|callback| self.type_of(*callback));
        let function = callback.and_then(|callback| self.types.function(callback));
        function.map(|function| function.returns)
    }

    /// A type one array less deep: what the arrays among its members hold, with
    /// the members that are none.
    fn flattened(&mut self, element: TypeId) -> TypeId {
        let mut flat = Vec::new();
        for member in self.members_of(element) {
            flat.push(self.held_by(member).unwrap_or(member));
        }
        self.types.union(flat)
    }

    /// The call of a method of an array `callee` is the callee of, when it is one.
    pub(crate) fn listed(&mut self, callee: &Element<'_>) -> Option<Listed> {
        let Element::Node(target) = *callee else {
            return None;
        };
        if target.kind() != NodeKind::MemberExpr {
            return None;
        }
        let (name, at) = self.key_of(target);
        let (receiver, short) = self.receiver(target, false)?;
        let element = self.held_by(receiver)?;
        ARRAY_METHODS.contains(&name.as_str()).then_some(Listed {
            name,
            at,
            element,
            receiver,
            short,
        })
    }

    /// What a `reduce` starts from: what it is given to start with, or an element.
    fn start(&mut self, listed: &Listed, passed: &[Element<'_>]) -> Option<TypeId> {
        match passed.get(1) {
            Some(start) => self.type_of(*start).map(|ty| self.loosened(ty)),
            None => Some(listed.element),
        }
    }

    /// What a callback at `place` of a call of a built-in is passed, place by place,
    /// when the call is one that takes a callback there: a method of an array, or
    /// `Object.groupBy`, `Array.from` and `Array.fromAsync`.
    pub(crate) fn callback_of(
        &mut self,
        callee: &Element<'_>,
        place: usize,
        passed: &[Element<'_>],
    ) -> Option<Vec<Option<TypeId>>> {
        let index = Some(TypeId::INT32);
        let text = self.source.slice(&element_span_of(callee));
        let statics = [
            "Object.groupBy",
            "Map.groupBy",
            "Array.from",
            "Array.fromAsync",
        ];
        let over = text.is_some_and(|text| statics.contains(&text)) && self.is_global(callee);
        if over && place == 1 {
            let iterable = self.type_of(passed[0]);
            let item = iterable.and_then(|iterable| self.item_of(iterable, false));
            return Some(vec![item.map(|item| self.widened(item)), index]);
        }
        if let Some(taken) = self.replacer_takes(callee, place, passed) {
            return Some(taken);
        }
        if let Some(keyed) = self.keyed(callee).filter(|_| place == 0) {
            return keyed_takes(&keyed);
        }
        let listed = self.listed(callee).filter(|_| place == 0)?;
        let (element, receiver) = (Some(listed.element), Some(listed.receiver));
        let name = listed.name.as_str();
        if VISITORS.contains(&name) {
            Some(vec![element, index, receiver])
        } else if ["reduce", "reduceRight"].contains(&name) {
            Some(vec![self.start(&listed, passed), element, index, receiver])
        } else if ["sort", "toSorted"].contains(&name) {
            Some(vec![element, element])
        } else {
            None
        }
    }

    /// The type of a call of a method of an array, and what is wrong with it.
    pub(crate) fn array_call(
        &mut self,
        listed: &Listed,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let first = self.first_time(node);
        let passed = arguments(children);
        let item = listed.element;
        let name = listed.name.as_str();
        let sorts = ["sort", "toSorted"].contains(&name);
        if sorts && passed.is_empty() && item != TypeId::STRING {
            let message = format!(
                "`{name}()` without a comparator orders by text, and this is a `{}`: pass one, `(a, b) => a - b` for numbers (SPEC.md §8.1)",
                self.print_type(listed.receiver)
            );
            self.report(first, Code::E133, listed.at, message);
        }
        let searches = ["indexOf", "lastIndexOf", "includes"].contains(&name);
        let sought = passed.first().copied().filter(|_| searches);
        let sought = sought.and_then(|sought| Some((sought, self.type_of(sought)?)));
        if let Some((sought, given)) = sought {
            let given = self.widened(given);
            let held = self.any_number(item);
            if !self.assignable(given, held) {
                let message = format!(
                    "a `{}` is looked for in a `{}`, which holds none (SPEC.md §6.1)",
                    self.print_type(given),
                    self.print_type(listed.receiver)
                );
                self.report(first, Code::E138, element_span_of(&sought), message);
            }
        }
        let given = match name {
            "at" | "find" | "findLast" | "pop" | "shift" => {
                self.types.union([item, TypeId::UNDEFINED])
            }
            "concat" => {
                let mut held = vec![item];
                for added in passed.iter().filter(|added| !is_spread(added)) {
                    let added = self.type_of(*added)?;
                    let added = self.widened(added);
                    held.push(self.flattened(added));
                }
                let held = self.types.union(held);
                self.list(held)
            }
            "copyWithin" | "fill" | "reverse" | "sort" => listed.receiver,
            "entries" => {
                let entry = self.types.intern(Type::Tuple(vec![TypeId::INT32, item]));
                self.iterator(entry, false)
            }
            "keys" => self.iterator(TypeId::INT32, false),
            "values" => self.iterator(item, false),
            "every" | "some" | "includes" => TypeId::BOOLEAN,
            // What a predicate lets through is what it tells its argument is.
            "filter" => {
                let kept = self.sifted(passed.first(), item);
                self.list(kept)
            }
            "slice" | "splice" | "toReversed" | "toSorted" | "toSpliced" | "with" => {
                self.list(item)
            }
            "findIndex" | "findLastIndex" | "indexOf" | "lastIndexOf" | "push" | "unshift" => {
                TypeId::INT32
            }
            "flat" => {
                // As deep as it is told to go, when it is told by a literal.
                let depth = match passed.first() {
                    Some(depth) => {
                        let depth = self.type_of(*depth)?;
                        let Type::Literal(Literal::Number(depth)) = self.types.get(depth) else {
                            return None;
                        };
                        depth.text().parse::<usize>().ok()?
                    }
                    None => 1,
                };
                let mut flat = item;
                for _ in 0..depth.min(DEPTH) {
                    flat = self.flattened(flat);
                }
                self.list(flat)
            }
            "flatMap" => {
                let returned = self.returned_by(&passed)?;
                let flat = self.flattened(returned);
                self.list(flat)
            }
            "forEach" => TypeId::UNDEFINED,
            "join" | "toString" | "toLocaleString" => TypeId::STRING,
            "map" => {
                let returned = self.returned_by(&passed)?;
                self.list(returned)
            }
            _ => {
                let start = self.start(listed, &passed)?;
                let returned = self.returned_by(&passed)?;
                self.types.union([start, returned])
            }
        };
        Some(self.after_chain(given, listed.short))
    }

    /// The type of `Object.name(…)`, the `Object` of the ambient library.
    pub(crate) fn object_call(&mut self, name: &str, children: &[Element<'_>]) -> Option<TypeId> {
        let passed = arguments(children);
        let first = passed.first().and_then(|first| self.type_of(*first));
        match name {
            "hasOwn" | "isFrozen" => Some(TypeId::BOOLEAN),
            "freeze" | "assign" => first,
            "groupBy" => {
                let item = self.item_of(first?, false)?;
                let item = self.widened(item);
                let group = self.list(item);
                Some(self.types.intern(Type::Dictionary(group)))
            }
            "fromEntries" => {
                let entry = self.item_of(first?, false)?;
                // Each entry is a key and a value: a pair, or an array of both.
                let mut values = Vec::new();
                for entry in self.members_of(entry) {
                    values.push(match self.types.get(entry).clone() {
                        Type::Tuple(parts) => parts.get(1).copied()?,
                        Type::Array { element, .. } => element,
                        _ => return None,
                    });
                }
                let value = self.types.union(values);
                let value = self.widened(value);
                Some(self.types.intern(Type::Dictionary(value)))
            }
            "keys" | "values" | "entries" => {
                let from = first?;
                let element = match self.types.get(from).clone() {
                    Type::Object(properties) if name != "values" => {
                        return Some(self.keys_of(properties, name == "entries"));
                    }
                    Type::Object(properties) => {
                        let held: Vec<TypeId> = properties.iter().map(|p| p.ty).collect();
                        self.types.union(held)
                    }
                    Type::Dictionary(_) if name == "keys" => TypeId::STRING,
                    Type::Dictionary(value) if name == "values" => value,
                    Type::Dictionary(value) => {
                        self.types.intern(Type::Tuple(vec![TypeId::STRING, value]))
                    }
                    _ => return None,
                };
                Some(self.list(element))
            }
            _ => None,
        }
    }

    /// The type of `Array.name(…)`, the `Array` of the ambient library.
    pub(crate) fn array_static(&mut self, name: &str, children: &[Element<'_>]) -> Option<TypeId> {
        let passed = arguments(children);
        match name {
            "isArray" => Some(TypeId::BOOLEAN),
            "of" => {
                let mut held = Vec::new();
                for item in passed.iter().filter(|item| !is_spread(item)) {
                    held.push(self.type_of(*item)?);
                }
                // An array of nothing has no type of its own.
                let held = (!held.is_empty()).then(|| self.types.union(held))?;
                let held = self.widened(held);
                Some(self.list(held))
            }
            "from" | "fromAsync" => {
                let iterable = passed.first().and_then(|first| self.type_of(*first))?;
                let item = self.item_of(iterable, name == "fromAsync")?;
                // What is taken out of a tuple is no literal any more.
                let item = self.widened(item);
                let mapped = match passed.get(1) {
                    Some(_) => self.returned_by(&passed[1..])?,
                    None => item,
                };
                let made = self.list(mapped);
                Some(if name == "from" {
                    made
                } else {
                    self.types.intern(Type::Promise(made))
                })
            }
            _ => None,
        }
    }
}
