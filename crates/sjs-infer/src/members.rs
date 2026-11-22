//! Property reads and writes (SPEC.md §3.2, §4.6): `o.k`, `o?.k`, `o[k]`, `o.k = v`.
//! A read needs the key on every member of the receiver's type (E137) and gives the
//! union of what the members hold; `?.` on a receiver that is never nullish is E121;
//! a write needs the key there already (E129) and a value the key takes (E138); a
//! method is called, not read (E138). A member of a class instance is looked for in
//! its class and then in the classes that one extends; a key with a getter and no
//! setter is not written (E138).

use crate::classes::statics_of;
use crate::expressions::{element_span_of, punct, Typer};
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct};
use sjs_types::{Assignability, ClassId, Literal, Printer, Property, Type, TypeId, VarId};
use std::collections::BTreeMap;

/// What a member of a receiver's type holds at a key.
enum Held {
    /// A value of this type.
    Value(TypeId),
    /// Nothing: the key is not there.
    Missing,
    /// A method, which is called and not read: its type, when that is known.
    Method(Option<TypeId>),
    /// Not known yet: a type a later pass gives members.
    Unknown,
}

/// What a computed read gives from one member, or why it gives nothing.
enum Indexed {
    Value(TypeId),
    /// This key is not there.
    Missing(String),
    /// The key is no literal of the member's keys, nor a place of it.
    Unreadable,
    Unknown,
}

/// Where a key of a class instance is declared.
pub(crate) enum Owner {
    /// In this class, whose type parameters are these there.
    At(ClassId, BTreeMap<VarId, TypeId>),
    /// Nowhere.
    Missing,
    /// Not known: a class nothing is known of is on the way.
    Unknown,
}

fn is_member(node: NodeRef<'_>) -> bool {
    matches!(node.kind(), NodeKind::MemberExpr | NodeKind::IndexExpr)
}

impl Typer<'_> {
    /// Whether a value of the type `from` may go where a `to` is taken (§3.3).
    pub(crate) fn assignable(&self, from: TypeId, to: TypeId) -> bool {
        let superclass = |class: ClassId| self.facts.superclass.get(&class).copied();
        Assignability::new(self.types, &superclass).assignable(from, to)
    }

    pub(crate) fn print_type(&self, id: TypeId) -> String {
        self.print(id)
    }

    pub(crate) fn members_of(&self, id: TypeId) -> Vec<TypeId> {
        self.members(id)
    }

    fn print(&self, id: TypeId) -> String {
        let name = |class: ClassId| self.class_name(class);
        Printer::new(self.types, &name).print(id)
    }

    fn members(&self, id: TypeId) -> Vec<TypeId> {
        match self.types.get(id) {
            Type::Union(members) => members.clone(),
            _ => vec![id],
        }
    }

    /// What `member` holds at `key`.
    fn held(&mut self, member: TypeId, key: &str) -> Held {
        match self.types.get(member).clone() {
            Type::Object(properties) => {
                let property = properties.iter().find(|property| property.key == key);
                property.map_or(Held::Missing, |property| Held::Value(property.ty))
            }
            Type::Class(class, arguments) => match self.owner(class, &arguments, key) {
                Owner::Missing => Held::Missing,
                Owner::Unknown => Held::Unknown,
                Owner::At(owner, bound) => {
                    let shape = self.facts.classes.get(&owner);
                    let fields = self.facts.fields.get(&owner);
                    let field = fields.and_then(|fields| fields.get(key)).copied();
                    let getter = shape.and_then(|shape| shape.getters.get(key));
                    let signature = shape.and_then(|shape| shape.signatures.get(key)).copied();
                    let methods = self.facts.methods.get(&owner);
                    let method = methods.is_some_and(|methods| methods.contains(key));
                    match field.or(getter.copied().flatten()) {
                        Some(ty) => Held::Value(self.substituted(ty, &bound)),
                        None if method => {
                            let signature = signature.map(|ty| self.substituted(ty, &bound));
                            Held::Method(signature)
                        }
                        // Declared, with no type yet.
                        None => Held::Unknown,
                    }
                }
            },
            Type::Array { .. }
            | Type::Tuple(_)
            | Type::String
            | Type::Literal(Literal::String(_))
                if key == "length" =>
            {
                Held::Value(TypeId::INT32)
            }
            Type::Dictionary(value) => Held::Value(self.types.union([value, TypeId::UNDEFINED])),
            Type::Unknown | Type::Undefined | Type::Null => Held::Missing,
            _ => Held::Unknown,
        }
    }

    /// Where `key` is declared for an instance of `class` with the type arguments
    /// `arguments`: in the class or in the classes it extends, the nearest first.
    pub(crate) fn owner(&mut self, class: ClassId, arguments: &[TypeId], key: &str) -> Owner {
        let mut at = class;
        let mut arguments = arguments.to_vec();
        let mut seen = Vec::new();
        while !seen.contains(&at) {
            seen.push(at);
            let shape = self.facts.classes.get(&at);
            let params = shape.map(|shape| shape.params.clone());
            let params = params.unwrap_or_default().into_iter();
            let bound: BTreeMap<VarId, TypeId> = params.zip(arguments).collect();
            let fields = self.facts.fields.get(&at);
            let methods = self.facts.methods.get(&at);
            let declared = fields.is_some_and(|fields| fields.contains_key(key))
                || methods.is_some_and(|methods| methods.contains(key))
                || shape.is_some_and(|shape| shape.declared.contains(key));
            if declared {
                return Owner::At(at, bound);
            }
            if fields.is_none() {
                return Owner::Unknown;
            }
            let Some(base) = self.facts.superclass.get(&at).copied() else {
                break;
            };
            // What the class extended is given for its type parameters.
            let passed = shape.map_or(Some(Vec::new()), |shape| shape.base_arguments.clone());
            let Some(passed) = passed else {
                return Owner::Unknown;
            };
            arguments = Vec::new();
            for ty in passed {
                arguments.push(self.substituted(ty, &bound));
            }
            at = base;
        }
        Owner::Missing
    }

    /// What `member` takes at `key` when it is written: what its setter takes, when
    /// it has one; `None` when it has a getter alone, and is not written.
    fn taking(&mut self, member: TypeId, key: &str) -> Option<Held> {
        if let Type::Class(class, arguments) = self.types.get(member).clone() {
            if let Owner::At(owner, bound) = self.owner(class, &arguments, key) {
                let shape = self.facts.classes.get(&owner);
                let setter = shape.and_then(|shape| shape.setters.get(key)).copied();
                let got = shape.is_some_and(|shape| shape.getters.contains_key(key));
                match (setter, got) {
                    (Some(Some(taken)), _) => {
                        return Some(Held::Value(self.substituted(taken, &bound)))
                    }
                    (Some(None), _) => return Some(Held::Unknown),
                    (None, true) => return None,
                    (None, false) => {}
                }
            }
        }
        Some(self.held(member, key))
    }

    /// What is read from a value of the type `from` at `key`, when it holds a value
    /// there.
    pub(crate) fn read(&mut self, from: TypeId, key: &str) -> Option<TypeId> {
        match self.held(from, key) {
            Held::Value(ty) => Some(ty),
            _ => None,
        }
    }

    /// The type of the method `name` of a value of the type `instance`, when it has
    /// that method and its type is known.
    pub(crate) fn method_of(&mut self, instance: TypeId, name: &str) -> Option<TypeId> {
        match self.held(instance, name) {
            Held::Method(signature) => signature,
            _ => None,
        }
    }

    /// A type that takes any number where it takes an `int32`: `int32` is inferred,
    /// never asked for.
    pub(crate) fn any_number(&mut self, taken: TypeId) -> TypeId {
        let members = self.members(taken);
        let wider = members.into_iter().map(|member| {
            if member == TypeId::INT32 {
                TypeId::NUMBER
            } else {
                member
            }
        });
        self.types.union(wider)
    }

    /// The class a name names, when `element` is the name of one.
    fn named_class(&self, element: &Element<'_>) -> Option<ClassId> {
        let Element::Token(name) = element else {
            return None;
        };
        let reference = self.references.get(&name.token().span.byte_start);
        let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
        declared.and_then(|declared| self.facts.class_of.get(&declared).copied())
    }

    /// The keys a type names, when it is a literal or a union of literals.
    fn keys(&self, id: TypeId) -> Option<Vec<String>> {
        let key = |member: &TypeId| match self.types.get(*member) {
            Type::Literal(Literal::String(value)) => Some(value.clone()),
            Type::Literal(Literal::Number(value)) => Some(value.text()),
            Type::Literal(Literal::BigInt(value)) => Some(value.decimal()),
            Type::Literal(Literal::Boolean(value)) => Some(value.to_string()),
            _ => None,
        };
        self.members(id).iter().map(key).collect()
    }

    /// What `member[key]` gives, for a key of type `key`.
    fn indexed(&mut self, member: TypeId, key: TypeId) -> Indexed {
        let wide = self.widened(key);
        let is_number = [TypeId::NUMBER, TypeId::INT32].contains(&wide);
        let or_absent = |typer: &mut Self, ty: TypeId| {
            Indexed::Value(typer.types.union([ty, TypeId::UNDEFINED]))
        };
        match self.types.get(member).clone() {
            Type::Object(_) | Type::Class(..) => {
                let Some(keys) = self.keys(key) else {
                    return Indexed::Unreadable;
                };
                let mut values = Vec::new();
                for key in keys {
                    match self.held(member, &key) {
                        Held::Value(ty) => values.push(ty),
                        Held::Missing => return Indexed::Missing(key),
                        Held::Method(_) | Held::Unknown => return Indexed::Unknown,
                    }
                }
                Indexed::Value(self.types.union(values))
            }
            Type::Tuple(elements) if is_number => {
                // A literal place in range is that element; any other may be absent.
                let places = self.keys(key).unwrap_or_default();
                let exact: Option<Vec<TypeId>> = places
                    .iter()
                    .map(|place| {
                        place
                            .parse::<usize>()
                            .ok()
                            .and_then(|at| elements.get(at).copied())
                    })
                    .collect();
                if let Some(exact) = exact.filter(|exact| !exact.is_empty()) {
                    Indexed::Value(self.types.union(exact))
                } else {
                    let any = self.types.union(elements);
                    or_absent(self, any)
                }
            }
            Type::Array { element, .. } if is_number => or_absent(self, element),
            Type::String | Type::Literal(Literal::String(_)) if is_number => {
                or_absent(self, TypeId::STRING)
            }
            Type::Dictionary(value) if wide == TypeId::STRING => or_absent(self, value),
            Type::Tuple(_)
            | Type::Array { .. }
            | Type::String
            | Type::Literal(Literal::String(_))
            | Type::Dictionary(_)
            | Type::Unknown
            | Type::Undefined
            | Type::Null => Indexed::Unreadable,
            _ => Indexed::Unknown,
        }
    }

    /// The type of what a member access reads from, without `undefined` and `null`
    /// when it is read by `?.`, and whether a `?.` so far may have found nothing.
    /// E121 for a `?.` on a receiver that is never nullish.
    pub(crate) fn receiver(&mut self, node: NodeRef<'_>, first: bool) -> Option<(TypeId, bool)> {
        let children: Vec<Element<'_>> = node.children().collect();
        let first_child = children.first().copied();
        let (receiver, short) = first_child.and_then(|child| match child {
            Element::Node(inner) if is_member(inner) => self.chain(inner),
            // A class by name: what its static members are members of.
            other => {
                let class = self.named_class(&other);
                let side = class.map(|class| self.instance(statics_of(class)));
                side.or_else(|| self.type_of(other)).map(|ty| (ty, false))
            }
        })?;
        if punct(children.get(1)) != Some(Punct::QuestionDot) {
            return Some((receiver, short));
        }
        let present = self.present(receiver);
        if present == receiver {
            let message = format!(
                "`?.` on a `{}`, which is never `undefined` or `null`: write `.` (SPEC.md §8.1)",
                self.print(receiver)
            );
            self.report(first, Code::E121, element_span_of(&children[1]), message);
        }
        // Only a receiver that may be absent ends the chain early.
        Some((present, short || present != receiver))
    }

    /// The key of `o.key`, and where it is written: a private name with its `#`.
    pub(crate) fn key_of(&self, node: NodeRef<'_>) -> (String, Span) {
        let key = node.tokens().last();
        let span = key.map_or(node.span(), |key| key.span);
        let text = self.source.slice(&span).unwrap_or_default();
        (text.to_owned(), span)
    }

    /// The type a member access gives, before a `?.` earlier in its chain is counted
    /// in, and whether one may have found nothing: then the whole chain is
    /// `undefined`, which the end of the chain adds, so `a?.b.c` reads `c` from `b`.
    pub(crate) fn chain(&mut self, node: NodeRef<'_>) -> Option<(TypeId, bool)> {
        let first = self.first_time(node);
        let (receiver, short) = self.receiver(node, first)?;
        let members = self.members(receiver);
        let mut values = Vec::new();
        let mut known = true;
        if node.kind() == NodeKind::MemberExpr {
            let (key, span) = self.key_of(node);
            for member in members {
                match self.held(member, &key) {
                    Held::Value(ty) => values.push(ty),
                    Held::Unknown => known = false,
                    Held::Missing => {
                        let message = format!(
                            "`{key}` is not a key of `{}`: every value this may be must have it (SPEC.md §3.2)",
                            self.print(member)
                        );
                        // Of a name: with what says what the name is.
                        let read = node.children().next();
                        let name = read.filter(|read| matches!(read, Element::Token(_)));
                        let operands: Vec<Element<'_>> = name.into_iter().collect();
                        self.report_about(first, Code::E137, span, message, &operands);
                        return None;
                    }
                    Held::Method(signature) => {
                        // A method is read only to be called on the spot.
                        let parent = node
                            .parent()
                            .filter(|parent| parent.kind() == NodeKind::CallExpr);
                        let called = parent
                            .is_some_and(|call| call.span().byte_start == node.span().byte_start);
                        if !called {
                            let message = format!(
                                "the method `{key}` is read as a value: call it, or wrap the call in an arrow (SPEC.md §12)"
                            );
                            self.report(first, Code::E138, span, message);
                            return None;
                        }
                        known &= signature.is_some();
                        values.extend(signature);
                    }
                }
            }
        } else {
            let children: Vec<Element<'_>> = node.children().collect();
            let written = children.iter().rev().nth(1).copied();
            let key = written.and_then(|written| self.type_of(written))?;
            let at = written.as_ref().map_or(node.span(), element_span_of);
            for member in members {
                let why = match self.indexed(member, key) {
                    Indexed::Value(ty) => {
                        values.push(ty);
                        continue;
                    }
                    Indexed::Unknown => {
                        known = false;
                        continue;
                    }
                    Indexed::Missing(key) => format!(
                        "`{key}` is not a key of `{}`: every key this may be must be one (SPEC.md §4.6)",
                        self.print(member)
                    ),
                    Indexed::Unreadable => format!(
                        "a `{}` is not read by a `{}`: a computed read takes a literal of the keys, or a number for an array (SPEC.md §4.6)",
                        self.print(member),
                        self.print(key)
                    ),
                };
                self.report(first, Code::E137, at, why);
                return None;
            }
        }
        known.then(|| (self.types.union(values), short))
    }

    /// `Object.keys(shape)`: an array of its keys, each a literal;
    /// `Object.entries(shape)`: an array of `[key, value]` tuples.
    pub(crate) fn keys_of(&mut self, shape: TypeId, entries: bool) -> Option<TypeId> {
        let Type::Object(properties) = self.types.get(shape).clone() else {
            return None;
        };
        let mut elements = Vec::new();
        for Property { key, ty, .. } in properties {
            let key = self.types.literal(Literal::String(key));
            let entry = if entries {
                self.types.intern(Type::Tuple(vec![key, ty]))
            } else {
                key
            };
            elements.push(entry);
        }
        let element = self.types.union(elements);
        Some(self.types.intern(Type::Array {
            element,
            mutated: false,
        }))
    }

    /// Judges `target = value` and the like, where `target` is `o.k` or `o[k]`: the
    /// key must be there on every member (E129), and for `=` the value must be one
    /// the key takes (E138; a key that holds an `int32` takes any number).
    fn write(&mut self, target: NodeRef<'_>, value: Option<Element<'_>>) {
        let first = self.first_time(target);
        let Some((receiver, _)) = self.receiver(target, first) else {
            return;
        };
        let children: Vec<Element<'_>> = target.children().collect();
        let (keys, span) = if target.kind() == NodeKind::MemberExpr {
            let (key, span) = self.key_of(target);
            (Some(vec![key]), Some(span))
        } else {
            let written = children.iter().rev().nth(1).copied();
            let key = written.and_then(|written| self.type_of(written));
            (
                key.and_then(|key| self.keys(key)),
                written.as_ref().map(element_span_of),
            )
        };
        let (Some(keys), Some(span)) = (keys, span) else {
            return;
        };
        // The length of an array is what is in it (SPEC.md §4.6).
        let sized = self
            .members(receiver)
            .into_iter()
            .any(|member| matches!(self.types.get(member), Type::Array { .. } | Type::Tuple(_)));
        if sized && keys == ["length"] {
            let message =
                "assigning `length` is removed: use `splice` or `toSpliced` (SPEC.md §4.6)";
            self.report(first, Code::E133, span, message.to_owned());
            return;
        }
        let mut takes = Vec::new();
        for member in self.members(receiver) {
            for key in &keys {
                match self.taking(member, key) {
                    Some(Held::Value(ty)) => takes.push(ty),
                    None => {
                        let message = format!(
                            "`{key}` has a getter and no setter: it is read, not written (SPEC.md §4.5)"
                        );
                        self.report(first, Code::E138, span, message);
                        return;
                    }
                    Some(Held::Missing) => {
                        let message = format!(
                            "`{key}` is not a key of `{}`: a key is not added after the object is made (SPEC.md §3.2)",
                            self.print(member)
                        );
                        self.report(first, Code::E129, span, message);
                        return;
                    }
                    Some(Held::Method(_) | Held::Unknown) => return,
                }
            }
        }
        let Some((value, given)) = value.and_then(|value| Some((value, self.type_of(value)?)))
        else {
            return;
        };
        for taken in takes {
            let wider = self.any_number(taken);
            if !self.assignable(given, wider) {
                let message = format!(
                    "a `{}` is assigned to `{}`, which holds a `{}` (SPEC.md §3.2)",
                    self.print(given),
                    keys.join("` or `"),
                    self.print(taken)
                );
                self.report(first, Code::E138, element_span_of(&value), message);
                return;
            }
        }
    }

    /// Judges `node` when it reads or writes a member: a write when it is the target
    /// of an assignment or of `++`/`--`, a read anywhere else.
    pub(crate) fn member_access(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        let writes = matches!(
            node.kind(),
            NodeKind::AssignExpr | NodeKind::UpdateExpr | NodeKind::ForUpdate
        );
        let target = children.iter().find_map(|child| match child {
            Element::Node(target) if writes && is_member(*target) => Some(*target),
            _ => None,
        });
        // The target is what comes before the operator, or all there is with `++`.
        let operator = children
            .iter()
            .position(|child| punct(Some(child)).is_some());
        let target = target.filter(|target| {
            let first = children
                .first()
                .is_some_and(|first| element_span_of(first) == target.span());
            first || children.len() == 2
        });
        if let Some(target) = target {
            let plain = operator.and_then(|at| punct(children.get(at))) == Some(Punct::Eq);
            let value = children.last().copied().filter(|_| plain);
            self.write(target, value);
        } else if is_member(node) {
            self.chain(node);
        }
    }
}
