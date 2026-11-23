//! Regular expressions and dates (SPEC.md §6.1, §2). A regex literal is a `RegExp`
//! that carries its capture groups, by number and by name, and its flags; `exec`
//! gives a match of those groups or `null`, with `indices` under the `d` flag, and
//! the function given to `replace` is passed the match and each group. `new RegExp`
//! is E133, as are `new Date()` with no argument and a `Date` made from text that is
//! not ISO 8601.

use crate::ambient::intl;
use crate::classes::UNKNOWN_BASE;
use crate::expressions::{element_span_of, Typer};
use crate::mutation::arguments;
use crate::primitives::is_string;
use crate::typed_arrays::ambient_class;
use sjs_diag::Code;
use sjs_syntax::{Element, NodeKind, NodeRef, Regex, TokenKind};
use sjs_types::{Literal, Property, Type, TypeId};

/// How many capture groups the body of a regex has: named ones too, and none of
/// what only groups (`(?:…)`) or looks around. The body is in the `v`-mode grammar,
/// where a parenthesis that is no group is written with a backslash, in a class too.
pub(crate) fn captures(body: &str) -> usize {
    let mut count = 0;
    let mut rest = body.chars();
    while let Some(c) = rest.next() {
        let ahead = rest.as_str();
        match c {
            '\\' => {
                rest.next();
            }
            '(' => {
                let looks = ahead.starts_with("?<=") || ahead.starts_with("?<!");
                let named = ahead.starts_with("?<") && !looks;
                count += usize::from(named || !ahead.starts_with('?'));
            }
            _ => {}
        }
    }
    count
}

/// Whether a text is a date and time as ISO 8601 writes one, in the forms a `Date`
/// reads everywhere alike: `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, then perhaps
/// `THH:mm`, `THH:mm:ss` or `THH:mm:ss.sss`, with `Z` or `±HH:mm`.
pub(crate) fn is_iso_8601(text: &str) -> bool {
    let digits = |part: &str, count: usize| {
        part.len() == count && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    let (date, time) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let parts: Vec<&str> = date.split('-').collect();
    let dated =
        parts.len() <= 3 && digits(parts[0], 4) && parts[1..].iter().all(|part| digits(part, 2));
    let timed = time.is_none_or(|time| {
        let zone_at = time.find(['Z', '+', '-']).unwrap_or(time.len());
        let (clock, zone) = time.split_at(zone_at);
        let offset: Vec<&str> = zone.get(1..).unwrap_or_default().split(':').collect();
        let offset = offset.len() == 2 && offset.iter().all(|part| digits(part, 2));
        let zoned = zone.is_empty() || zone == "Z" || offset;
        let (clock, fraction) = match clock.split_once('.') {
            Some((clock, fraction)) => (clock, Some(fraction)),
            None => (clock, None),
        };
        let units: Vec<&str> = clock.split(':').collect();
        let fractioned = fraction.is_none_or(|fraction| {
            let all = fraction.bytes().all(|byte| byte.is_ascii_digit());
            !fraction.is_empty() && all && units.len() == 3
        });
        let clocked = (2..=3).contains(&units.len()) && units.iter().all(|unit| digits(unit, 2));
        clocked && zoned && fractioned
    });
    dated && timed
}

impl Typer<'_> {
    /// The type of a regex literal: a `RegExp` of its capture groups, each by its
    /// number and the named ones by their names too, and of its flags.
    pub(crate) fn regex_type(&mut self, regex: &Regex) -> TypeId {
        let class = ambient_class("RegExp").unwrap_or(UNKNOWN_BASE);
        let group = self.types.union([TypeId::STRING, TypeId::UNDEFINED]);
        let numbered = (1..=captures(&regex.body)).map(|at| at.to_string());
        let named = regex.groups.iter().map(ToString::to_string);
        let groups = numbered.chain(named).map(|key| Property {
            key,
            ty: group,
            literal: false,
        });
        let groups = self.types.intern(Type::Object(groups.collect()));
        let flags = Literal::String(regex.flags.to_string());
        let flags = self.types.literal(flags);
        self.types.intern(Type::Class(class, vec![groups, flags]))
    }

    /// The groups and the flags of a `RegExp` type that carries them: its groups by
    /// number, those by name, and its flags.
    fn regex_parts(&self, regex: TypeId) -> Option<(Vec<Property>, Vec<Property>, String)> {
        let Type::Class(class, parts) = self.types.get(regex) else {
            return None;
        };
        let [groups, flags] = parts.as_slice() else {
            return None;
        };
        let (Type::Object(groups), Type::Literal(Literal::String(flags))) =
            (self.types.get(*groups), self.types.get(*flags))
        else {
            return None;
        };
        let is_number = |group: &&Property| group.key.parse::<usize>().is_ok();
        let numbered = groups.iter().filter(is_number).cloned().collect();
        let named = groups.iter().filter(|group| !is_number(group));
        let known = ambient_class("RegExp") == Some(*class);
        known.then(|| (numbered, named.cloned().collect(), flags.clone()))
    }

    /// The type of `regex.exec(text)`, when `callee` is the `exec` of a regex whose
    /// groups are known: a match, or `null`.
    pub(crate) fn exec_call(&mut self, callee: &Element<'_>) -> Option<TypeId> {
        let Element::Node(target) = *callee else {
            return None;
        };
        if target.kind() != NodeKind::MemberExpr || self.key_of(target).0 != "exec" {
            return None;
        }
        let (receiver, short) = self.receiver(target, false)?;
        let (numbered, named, flags) = self.regex_parts(receiver)?;
        let property = |key: &str, ty: TypeId| Property {
            key: key.to_owned(),
            ty,
            literal: false,
        };
        let groups = if named.is_empty() {
            TypeId::UNDEFINED
        } else {
            self.types.intern(Type::Object(named))
        };
        let mut found = vec![
            property("0", TypeId::STRING),
            property("index", TypeId::INT32),
            property("input", TypeId::STRING),
            property("groups", groups),
        ];
        found.extend(numbered);
        if flags.contains('d') {
            // Where each group was found, when it was.
            let span = self
                .types
                .intern(Type::Tuple(vec![TypeId::INT32, TypeId::INT32]));
            let span = self.types.union([span, TypeId::UNDEFINED]);
            let spans = self.types.intern(Type::Array {
                element: span,
                mutated: false,
            });
            found.push(property("indices", spans));
        }
        let found = self.types.intern(Type::Object(found));
        let given = self.types.union([found, TypeId::NULL]);
        Some(self.after_chain(given, short))
    }

    /// What the function given to `text.replace(pattern, …)` or `replaceAll` is
    /// passed, place by place: the match, each group of the pattern when it is a
    /// regex, where the match is, the text, and the named groups when there are any.
    pub(crate) fn replacer_takes(
        &mut self,
        callee: &Element<'_>,
        place: usize,
        passed: &[Element<'_>],
    ) -> Option<Vec<Option<TypeId>>> {
        let Element::Node(target) = *callee else {
            return None;
        };
        let replaces = ["replace", "replaceAll"].contains(&self.key_of(target).0.as_str());
        if target.kind() != NodeKind::MemberExpr || !replaces || place != 1 {
            return None;
        }
        let (receiver, _) = self.receiver(target, false)?;
        let text = self.members_of(receiver);
        if !text.iter().all(|member| is_string(self.types.get(*member))) {
            return None;
        }
        let pattern = self.type_of(passed[0]);
        let parts = pattern.and_then(|pattern| self.regex_parts(pattern));
        let (numbered, named, _) = parts.unwrap_or_default();
        let mut taken = vec![Some(TypeId::STRING)];
        taken.extend(numbered.iter().map(|group| Some(group.ty)));
        taken.extend([Some(TypeId::INT32), Some(TypeId::STRING)]);
        if !named.is_empty() {
            taken.push(Some(self.types.intern(Type::Object(named))));
        }
        Some(taken)
    }

    /// The type of `Intl.name(…)`: what `ambient/intl.toml` says a function of `Intl`
    /// gives.
    pub(crate) fn intl_static(&mut self, name: &str) -> Option<TypeId> {
        let tables = intl().iter().filter(|(table, _)| table == "Intl");
        let mut functions = tables.flat_map(|(_, functions)| functions);
        let (_, what) = functions.find(|(function, _)| function == name)?;
        Some(crate::ambient::named_type(self.types, what))
    }

    /// E133 for the removed ways of making a `RegExp` and a `Date`: `new RegExp(…)`
    /// and `RegExp(…)`; `new Date()` with no argument, and with a text written out
    /// that is not ISO 8601.
    pub(crate) fn made_rules(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        let made = node.kind() == NodeKind::NewExpr;
        if !made && node.kind() != NodeKind::CallExpr {
            return;
        }
        let Some(callee) = children
            .get(usize::from(made))
            .filter(|name| self.is_global(name))
        else {
            return;
        };
        let name = self
            .source
            .slice(&element_span_of(callee))
            .unwrap_or_default();
        let passed = arguments(children);
        let mut found = None;
        if name == "RegExp" {
            let message = "`RegExp` as a function is removed: write a regex literal, whose pattern and flags are checked (SPEC.md §6.1)";
            found = Some((node.span(), message));
        }
        if name == "Date" && made && passed.is_empty() {
            let message =
                "`new Date()` reads the clock unseen: write `new Date(Date.now())` (SPEC.md §6.1)";
            found = Some((node.span(), message));
        }
        let text = passed.first().and_then(|first| match first {
            Element::Token(text) => match &text.token().kind {
                TokenKind::String(value) => Some((text.token().span, value.to_string())),
                _ => None,
            },
            Element::Node(_) => None,
        });
        if let Some((at, _)) = text.filter(|(_, text)| name == "Date" && !is_iso_8601(text)) {
            let message = "a `Date` is made from ISO 8601 text, `2026-01-31T12:00:00Z`: any other is read differently from one runtime to the next (SPEC.md §6.1)";
            found = Some((at, message));
        }
        if let Some((at, message)) = found {
            self.report(true, Code::E133, at, message.to_owned());
        }
    }
}
