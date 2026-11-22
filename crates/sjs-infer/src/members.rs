//! Property reads and writes (SPEC.md §3.2, §4.6): `o.k`, `o?.k`, `o[k]`, `o.k = v`.
//! A read needs the key on every member of the receiver's type (E137) and gives the
//! union of what the members hold; `?.` on a receiver that is never nullish is E121;
//! a write needs the key there already (E129) and a value the key takes (E138); a
//! method is called, not read (E138).

use crate::expressions::{element_span_of, punct, Typer};
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, TokenKind};
use sjs_types::{Assignability, ClassId, Literal, Printer, Property, Type, TypeId};

/// What a member of a receiver's type holds at a key.
enum Held {
    /// A value of this type.
    Value(TypeId),
    /// Nothing: the key is not there.
    Missing,
    /// A method, which is called and not read.
    Method,
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
            Type::Class(class) => {
                // The class and the classes it extends, the nearest first.
                let mut at = Some(class);
                let mut seen = Vec::new();
                while let Some(class) = at.filter(|class| !seen.contains(class)) {
                    seen.push(class);
                    let field = self
                        .facts
                        .fields
                        .get(&class)
                        .and_then(|fields| fields.get(key));
                    if let Some(&field) = field {
                        return Held::Value(field);
                    }
                    if self
                        .facts
                        .methods
                        .get(&class)
                        .is_some_and(|methods| methods.contains(key))
                    {
                        return Held::Method;
                    }
                    if !self.facts.fields.contains_key(&class) {
                        return Held::Unknown;
                    }
                    at = self.facts.superclass.get(&class).copied();
                }
                Held::Missing
            }
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
            Type::Object(_) | Type::Class(_) => {
                let Some(keys) = self.keys(key) else {
                    return Indexed::Unreadable;
                };
                let mut values = Vec::new();
                for key in keys {
                    match self.held(member, &key) {
                        Held::Value(ty) => values.push(ty),
                        Held::Missing => return Indexed::Missing(key),
                        Held::Method | Held::Unknown => return Indexed::Unknown,
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
    fn receiver(&mut self, node: NodeRef<'_>, first: bool) -> Option<(TypeId, bool)> {
        let children: Vec<Element<'_>> = node.children().collect();
        let first_child = children.first().copied();
        let (receiver, short) = first_child.and_then(|child| match child {
            Element::Node(inner) if is_member(inner) => self.chain(inner),
            other => self.type_of(other).map(|ty| (ty, false)),
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

    /// The key of `o.key`, and where it is written; `None` for a private name.
    fn key_of(&self, node: NodeRef<'_>) -> Option<(String, Span)> {
        let named = |key: &&sjs_syntax::Token| !matches!(key.kind, TokenKind::PrivateName(_));
        let key = node.tokens().last().filter(named);
        key.map(|key| {
            let text = self.source.slice(&key.span).unwrap_or_default();
            (text.to_owned(), key.span)
        })
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
            let (key, span) = self.key_of(node)?;
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
                    Held::Method => {
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
                        }
                        return None;
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
            let key = self.key_of(target);
            (
                key.clone().map(|(key, _)| vec![key]),
                key.map(|(_, span)| span),
            )
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
                match self.held(member, key) {
                    Held::Value(ty) => takes.push(ty),
                    Held::Missing => {
                        let message = format!(
                            "`{key}` is not a key of `{}`: a key is not added after the object is made (SPEC.md §3.2)",
                            self.print(member)
                        );
                        self.report(first, Code::E129, span, message);
                        return;
                    }
                    Held::Method | Held::Unknown => return,
                }
            }
        }
        let Some((value, given)) = value.and_then(|value| Some((value, self.type_of(value)?)))
        else {
            return;
        };
        for taken in takes {
            // `int32` is inferred, never asked for: such a key takes any number.
            let members = self.members(taken);
            let wider = members.into_iter().map(|m| {
                if m == TypeId::INT32 {
                    TypeId::NUMBER
                } else {
                    m
                }
            });
            let wider = self.types.union(wider);
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
