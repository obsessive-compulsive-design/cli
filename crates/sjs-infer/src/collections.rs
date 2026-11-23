//! The keyed collections (SPEC.md §6.1): `Map<K, V>`, `Set<T>` and `WeakMap<K, V>`.
//! One made from an iterable holds what the iterable gives, with what is put into it
//! where its binding is in scope; an empty one, only that (US-087). Their methods are
//! typed, the ES2025 set methods and `Map.groupBy` among them: `get` may find
//! nothing. Asking a collection for a key it cannot hold is E138, as is a `WeakMap`
//! key that is no object.

use crate::expressions::{element_span_of, punct, Typer};
use crate::functions::is_spread;
use crate::mutation::arguments;
use sjs_diag::Code;
use sjs_syntax::{Element, NodeKind, NodeRef};
use sjs_types::{Builtin, Type, TypeId};

/// The methods of a `Map`.
const MAP_METHODS: [&str; 9] = [
    "get", "set", "has", "delete", "clear", "keys", "values", "entries", "forEach",
];

/// The methods of a `Set`.
const SET_METHODS: [&str; 15] = [
    "add",
    "has",
    "delete",
    "clear",
    "keys",
    "values",
    "entries",
    "forEach",
    "union",
    "intersection",
    "difference",
    "symmetricDifference",
    "isSubsetOf",
    "isSupersetOf",
    "isDisjointFrom",
];

/// The methods of a `WeakMap`.
const WEAK_METHODS: [&str; 4] = ["get", "set", "has", "delete"];

/// The methods a collection of this kind has.
pub(crate) fn methods_of(kind: Builtin) -> &'static [&'static str] {
    match kind {
        Builtin::Map => &MAP_METHODS,
        Builtin::Set => &SET_METHODS,
        Builtin::WeakMap => &WEAK_METHODS,
    }
}

/// A call of a method of a keyed collection.
pub(crate) struct Keyed {
    /// The method.
    name: String,
    /// The kind of collection.
    kind: Builtin,
    /// What it holds: keys and values, or members.
    held: Vec<TypeId>,
    /// The collection.
    receiver: TypeId,
    /// Whether a `?.` before it may have found nothing.
    short: bool,
}

/// What the callback of a collection's `forEach` is passed: the value, the key (the
/// member again, for a `Set`) and the collection.
pub(crate) fn keyed_takes(keyed: &Keyed) -> Option<Vec<Option<TypeId>>> {
    let value = keyed.held.last().copied();
    let key = keyed.held.first().copied();
    (keyed.name == "forEach").then(|| vec![value, key, Some(keyed.receiver)])
}

impl Typer<'_> {
    /// The type of `new Map(entries)`, `new Set(members)` or `new WeakMap(entries)`
    /// of the ambient library, by what the one argument gives; `None` for any other
    /// `new`, and for an empty one, which is typed by what is put into it.
    pub(crate) fn collection_made(&mut self, children: &[Element<'_>]) -> Option<TypeId> {
        let made = children.get(1).filter(|made| self.is_global(made))?;
        let kind = match self.source.slice(&element_span_of(made)) {
            Some("Map") => Builtin::Map,
            Some("Set") => Builtin::Set,
            Some("WeakMap") => Builtin::WeakMap,
            _ => return None,
        };
        let passed = arguments(children);
        let given = passed.first().copied().filter(|_| passed.len() == 1)?;
        if kind == Builtin::Set {
            let member = self.type_of(given)?;
            let member = self.item_of(member, false)?;
            let member = self.loosened(member);
            return Some(self.types.intern(Type::Builtin(kind, vec![member])));
        }
        // Entries written out are pairs, each a key and a value.
        let written = match given {
            Element::Node(list) => {
                let inside = list.children();
                inside.filter(|part| punct(Some(part)).is_none()).collect()
            }
            Element::Token(_) => Vec::new(),
        };
        let mut pairs = Vec::new();
        for entry in &written {
            let parts: Vec<Element<'_>> = match entry {
                Element::Node(pair) if pair.kind() == NodeKind::ArrayLit => {
                    let inside = pair.children();
                    inside.filter(|part| punct(Some(part)).is_none()).collect()
                }
                _ => Vec::new(),
            };
            if parts.len() == 2 && !parts.iter().any(is_spread) {
                pairs.push((self.type_of(parts[0])?, self.type_of(parts[1])?));
            }
        }
        let (keys, values) = if pairs.is_empty() || pairs.len() < written.len() {
            let entries = self.type_of(given)?;
            let entry = self.item_of(entries, false)?;
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for entry in self.members_of(entry) {
                let Type::Tuple(parts) = self.types.get(entry) else {
                    return None;
                };
                let [key, value] = parts.as_slice() else {
                    return None;
                };
                keys.push(*key);
                values.push(*value);
            }
            (keys, values)
        } else {
            pairs.into_iter().unzip()
        };
        let key = self.types.union(keys);
        let value = self.types.union(values);
        let held = vec![self.loosened(key), self.loosened(value)];
        Some(self.types.intern(Type::Builtin(kind, held)))
    }

    /// The call of a method of a keyed collection `callee` is the callee of, when it
    /// is one.
    pub(crate) fn keyed(&mut self, callee: &Element<'_>) -> Option<Keyed> {
        let Element::Node(target) = *callee else {
            return None;
        };
        if target.kind() != NodeKind::MemberExpr {
            return None;
        }
        let (name, _) = self.key_of(target);
        let (receiver, short) = self.receiver(target, false)?;
        let Type::Builtin(kind, held) = self.types.get(receiver).clone() else {
            return None;
        };
        methods_of(kind).contains(&name.as_str()).then_some(Keyed {
            name,
            kind,
            held,
            receiver,
            short,
        })
    }

    /// The type of a call of a method of a keyed collection, and what is wrong with
    /// it: a key the collection cannot hold, a `WeakMap` key that is no object (E138).
    pub(crate) fn keyed_call(
        &mut self,
        keyed: &Keyed,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let first = self.first_time(node);
        let passed = arguments(children);
        let name = keyed.name.as_str();
        let key = keyed.held[0];
        // What is held under a key: the member itself, for a `Set`.
        let value = keyed.held[keyed.held.len() - 1];
        let given = passed.first().copied();
        let given = given.and_then(|given| Some((given, self.type_of(given)?)));
        let asks = ["get", "has", "delete"].contains(&name);
        let weak = keyed.kind == Builtin::WeakMap;
        if let Some((sought, ty)) = given {
            let ty = self.widened(ty);
            let taken = self.any_number(key);
            let object = self.members_of(ty).into_iter().all(|member| {
                matches!(
                    self.types.get(member),
                    Type::Object(_)
                        | Type::Class(..)
                        | Type::Array { .. }
                        | Type::Tuple(_)
                        | Type::Function(_)
                        | Type::Builtin(..)
                        | Type::Dictionary(_)
                )
            });
            if weak && !object {
                let message = format!(
                    "a `{}` is a key of a `WeakMap`: its keys are objects, which it holds weakly (SPEC.md §6.1)",
                    self.print_type(ty)
                );
                self.report(first, Code::E138, element_span_of(&sought), message);
            } else if asks && !self.assignable(ty, taken) {
                let message = format!(
                    "a `{}` is looked for in a `{}`, which holds none (SPEC.md §6.1)",
                    self.print_type(ty),
                    self.print_type(keyed.receiver)
                );
                self.report(first, Code::E138, element_span_of(&sought), message);
            }
        }
        let given = match name {
            "get" => self.types.union([value, TypeId::UNDEFINED]),
            "set" | "add" | "intersection" | "difference" => keyed.receiver,
            "has" | "delete" | "isSubsetOf" | "isSupersetOf" | "isDisjointFrom" => TypeId::BOOLEAN,
            "clear" | "forEach" => TypeId::UNDEFINED,
            "keys" => self.iterator(key, false),
            "values" => self.iterator(value, false),
            "entries" => {
                let entry = self.types.intern(Type::Tuple(vec![key, value]));
                self.iterator(entry, false)
            }
            _ => {
                // `union`, `symmetricDifference`: with what the other set holds.
                let (_, other) = given?;
                let other = self.item_of(other, false)?;
                let other = self.loosened(other);
                let both = self.types.union([key, other]);
                self.types.intern(Type::Builtin(Builtin::Set, vec![both]))
            }
        };
        Some(self.after_chain(given, keyed.short))
    }

    /// The type of `Map.groupBy(items, by)`: a `Map` from what `by` gives to the
    /// items it gives it for.
    pub(crate) fn grouped(&mut self, children: &[Element<'_>]) -> Option<TypeId> {
        let passed = arguments(children);
        let items = passed.first().and_then(|items| self.type_of(*items))?;
        let item = self.item_of(items, false)?;
        let item = self.widened(item);
        let by = passed.get(1).and_then(|by| self.type_of(*by))?;
        let key = self.types.function(by).map(|by| by.returns)?;
        let group = self.types.intern(Type::Array {
            element: item,
            mutated: false,
        });
        Some(
            self.types
                .intern(Type::Builtin(Builtin::Map, vec![key, group])),
        )
    }
}
