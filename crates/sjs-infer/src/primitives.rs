//! The built-ins of the primitives (SPEC.md §6.1): the methods of strings, numbers
//! and bigints, the statics of `String`, `Number`, `BigInt`, `Math` and `JSON`, and
//! `structuredClone`. The removed members are E133 where a type tells them: the
//! `substr`, `charCodeAt`, `localeCompare` and HTML wrappers of a string,
//! `Number.parseInt` without a radix, `JSON.stringify` with a replacer.
//! `structuredClone` gives what it is given, and is E138 for a class instance or a
//! function, which a clone is no longer.

use crate::expressions::{element_span_of, Typer};
use crate::mutation::arguments;
use sjs_diag::{Code, Span};
use sjs_syntax::{Element, NodeKind, NodeRef};
use sjs_types::{Literal, Type, TypeId};

/// The kept methods of a string.
pub(crate) const STRING_METHODS: [&str; 34] = [
    "at",
    "charAt",
    "codePointAt",
    "concat",
    "endsWith",
    "includes",
    "indexOf",
    "isWellFormed",
    "lastIndexOf",
    "match",
    "matchAll",
    "normalize",
    "padEnd",
    "padStart",
    "repeat",
    "replace",
    "replaceAll",
    "search",
    "slice",
    "split",
    "startsWith",
    "substring",
    "toLocaleLowerCase",
    "toLocaleUpperCase",
    "toLowerCase",
    "toString",
    "toUpperCase",
    "toWellFormed",
    "trim",
    "trimEnd",
    "trimStart",
    "valueOf",
    "trimLeft",
    "trimRight",
];

/// The removed methods of a string, each with what is written in its stead.
pub(crate) const REMOVED_STRING_METHODS: [(&str, &str); 16] = [
    ("substr", "`slice` or `substring`"),
    ("charCodeAt", "`codePointAt`"),
    ("localeCompare", "the `compare` of an `Intl.Collator`"),
    ("anchor", "a template"),
    ("big", "a template"),
    ("blink", "a template"),
    ("bold", "a template"),
    ("fixed", "a template"),
    ("fontcolor", "a template"),
    ("fontsize", "a template"),
    ("italics", "a template"),
    ("link", "a template"),
    ("small", "a template"),
    ("strike", "a template"),
    ("sub", "a template"),
    ("sup", "a template"),
];

/// The kept methods of a number.
pub(crate) const NUMBER_METHODS: [&str; 3] = ["toFixed", "toPrecision", "toString"];

/// The constants of `Math` and of `Number`.
const CONSTANTS: [&str; 16] = [
    "Math.E",
    "Math.LN10",
    "Math.LN2",
    "Math.LOG10E",
    "Math.LOG2E",
    "Math.PI",
    "Math.SQRT1_2",
    "Math.SQRT2",
    "Number.EPSILON",
    "Number.MAX_SAFE_INTEGER",
    "Number.MIN_SAFE_INTEGER",
    "Number.MAX_VALUE",
    "Number.MIN_VALUE",
    "Number.NaN",
    "Number.NEGATIVE_INFINITY",
    "Number.POSITIVE_INFINITY",
];

/// A call of a method of a string, a number or a bigint.
pub(crate) struct Spoken {
    /// The method.
    name: String,
    /// Where it is named.
    at: Span,
    /// Whether it is called on a string: else on a number or a bigint.
    text: bool,
    /// Whether a `?.` before it may have found nothing.
    short: bool,
}

/// Whether a type is a string, or a literal of one.
pub(crate) fn is_string(ty: &Type) -> bool {
    matches!(ty, Type::String | Type::Literal(Literal::String(_)))
}

/// Whether a type is a number or a bigint, or a literal of one.
pub(crate) fn is_numeric(ty: &Type) -> bool {
    let literal = matches!(ty, Type::Literal(Literal::Number(_) | Literal::BigInt(_)));
    literal || matches!(ty, Type::Number | Type::Int32 | Type::BigInt)
}

/// Whether the text of what is called names a static this module types.
pub(crate) fn is_primitive_static(text: &str) -> bool {
    let (global, _) = text.split_once('.').unwrap_or((text, ""));
    let typed = [
        "Math",
        "String",
        "BigInt",
        "Number",
        "JSON",
        "structuredClone",
    ];
    typed.contains(&global)
}

impl Typer<'_> {
    /// The type of `Math.PI` and the like, when `node` reads a constant of the
    /// ambient `Math` or `Number`.
    pub(crate) fn constant(&self, node: NodeRef<'_>, children: &[Element<'_>]) -> Option<TypeId> {
        let text = self.source.slice(&node.span()).unwrap_or_default();
        let ambient = children.first().is_some_and(|name| self.is_global(name));
        (ambient && CONSTANTS.contains(&text)).then_some(TypeId::NUMBER)
    }

    /// The call of a method of a string, a number or a bigint `callee` is the callee
    /// of, when it is one.
    pub(crate) fn spoken(&mut self, callee: &Element<'_>) -> Option<Spoken> {
        let Element::Node(target) = *callee else {
            return None;
        };
        if target.kind() != NodeKind::MemberExpr {
            return None;
        }
        let (name, at) = self.key_of(target);
        let (receiver, short) = self.receiver(target, false)?;
        let members = self.members_of(receiver);
        let text = members
            .iter()
            .all(|member| is_string(self.types.get(*member)));
        // A value that is either has the methods both have: those of a number.
        let either = members.iter().all(|member| {
            let member = self.types.get(*member);
            is_string(member) || is_numeric(member)
        });
        either.then_some(Spoken {
            name,
            at,
            text,
            short,
        })
    }

    /// The type of a call of a method of a string, a number or a bigint; E133 for a
    /// method of a string that is removed.
    pub(crate) fn spoken_call(&mut self, spoken: &Spoken, node: NodeRef<'_>) -> Option<TypeId> {
        let first = self.first_time(node);
        let name = spoken.name.as_str();
        let removed = REMOVED_STRING_METHODS
            .iter()
            .find(|(removed, _)| *removed == name);
        if let Some((_, instead)) = removed.filter(|_| spoken.text) {
            let message = format!("`{name}` is removed: write {instead} (SPEC.md §6.1)");
            self.report(first, Code::E133, spoken.at, message);
            return None;
        }
        let kept = if spoken.text {
            STRING_METHODS.contains(&name)
        } else {
            NUMBER_METHODS.contains(&name)
        };
        if !kept {
            return None;
        }
        let strings = self.types.intern(Type::Array {
            element: TypeId::STRING,
            mutated: false,
        });
        let given = match name {
            "at" => self.types.union([TypeId::STRING, TypeId::UNDEFINED]),
            "codePointAt" => self.types.union([TypeId::INT32, TypeId::UNDEFINED]),
            "endsWith" | "includes" | "startsWith" | "isWellFormed" => TypeId::BOOLEAN,
            "indexOf" | "lastIndexOf" | "search" => TypeId::INT32,
            "match" => self.types.union([strings, TypeId::NULL]),
            "matchAll" => self.iterator(strings, false),
            "split" => strings,
            _ => TypeId::STRING,
        };
        Some(self.after_chain(given, spoken.short))
    }

    /// The type of a call of a static of `String`, `Number`, `BigInt`, `Math` or
    /// `JSON`, or of `structuredClone`, by the text of what is called; E133 for
    /// `Number.parseInt` without a radix and `JSON.stringify` with a replacer, E138
    /// for a `structuredClone` of what a clone is not.
    pub(crate) fn primitive_static(
        &mut self,
        text: &str,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let first = self.first_time(node);
        let passed = arguments(children);
        let (global, member) = text.split_once('.').unwrap_or((text, ""));
        match (global, member) {
            ("Math", "imul" | "clz32") => Some(TypeId::INT32),
            ("String", "raw" | "fromCodePoint") => Some(TypeId::STRING),
            ("BigInt", "asIntN" | "asUintN") => Some(TypeId::BIGINT),
            ("Number", "isFinite" | "isInteger" | "isNaN" | "isSafeInteger") => {
                Some(TypeId::BOOLEAN)
            }
            ("Math", _) | ("Number", "parseFloat") => Some(TypeId::NUMBER),
            ("Number", "parseInt") => {
                if passed.len() < 2 {
                    let message = "`Number.parseInt` without a radix guesses one from the text: pass it, `Number.parseInt(text, 10)` (SPEC.md §6.1)";
                    self.report(first, Code::E133, node.span(), message.to_owned());
                }
                Some(TypeId::NUMBER)
            }
            ("JSON", "parse" | "rawJSON") => Some(TypeId::UNKNOWN),
            ("JSON", "isRawJSON") => Some(TypeId::BOOLEAN),
            ("JSON", "stringify") => {
                // A replacer is anything in its place but `undefined`, which is
                // written there to pass an indent after it.
                let replacer = passed.get(1).map(element_span_of);
                let named = replacer.and_then(|replacer| self.source.slice(&replacer));
                if let Some(replacer) = replacer.filter(|_| named != Some("undefined")) {
                    let message = "a replacer of `JSON.stringify` is removed: shape the value before it is written (SPEC.md §6.1)";
                    self.report(first, Code::E133, replacer, message.to_owned());
                }
                Some(TypeId::STRING)
            }
            ("structuredClone", "") => {
                let value = passed.first().copied()?;
                let ty = self.type_of(value)?;
                let lost = self.members_of(ty).into_iter().find(|member| {
                    matches!(self.types.get(*member), Type::Class(..) | Type::Function(_))
                });
                if let Some(lost) = lost {
                    let message = format!(
                        "a `{}` is cloned: a clone of a class instance is no instance of its class, and a function is not cloned (SPEC.md §6.1)",
                        self.print_type(lost)
                    );
                    self.report(first, Code::E138, element_span_of(&value), message);
                }
                Some(ty)
            }
            _ => None,
        }
    }
}
