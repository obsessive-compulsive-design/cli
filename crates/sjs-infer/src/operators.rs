//! Operators (SPEC.md §4.1): what each takes and what it gives. An operand of a kind
//! the operator does not take is E138, a bitwise operator on a number that is no
//! `int32` E104, `===` between types that share no value E105, and an operand
//! compared with or assigned to itself E120.

use crate::expressions::{element_span_of, punct, Typer};
use sjs_diag::Code;
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, TokenKind};
use sjs_types::{Integer, Literal, Number, Type, TypeId};

/// The kind of value an operator sees in a type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Int32,
    Number,
    BigInt,
    String,
    Boolean,
    Other,
}

/// What `typeof` may give: one string for each kind of value there is (SPEC.md §3.1).
const TYPEOF: [&str; 7] = [
    "number",
    "bigint",
    "string",
    "boolean",
    "undefined",
    "object",
    "function",
];

/// Whether `element` names a place: a name, `this`, or members of one (`a.b[0]`).
fn is_place(element: &Element<'_>) -> bool {
    match element {
        Element::Token(token) => matches!(
            token.token().kind,
            TokenKind::Ident | TokenKind::Keyword(Keyword::This)
        ),
        Element::Node(node) => {
            let member = matches!(node.kind(), NodeKind::MemberExpr | NodeKind::IndexExpr);
            let inner = |child: Element<'_>| match child {
                Element::Node(_) => is_place(&child),
                Element::Token(_) => true,
            };
            member && node.children().all(inner)
        }
    }
}

impl Typer<'_> {
    fn kind(&mut self, id: TypeId) -> Kind {
        match self.widened(id) {
            TypeId::INT32 => Kind::Int32,
            TypeId::NUMBER => Kind::Number,
            TypeId::BIGINT => Kind::BigInt,
            TypeId::STRING => Kind::String,
            TypeId::BOOLEAN => Kind::Boolean,
            _ => Kind::Other,
        }
    }

    fn text(&self, element: &Element<'_>) -> String {
        let text = self
            .source
            .slice(&element_span_of(element))
            .unwrap_or_default();
        text.split_whitespace().collect()
    }

    /// Whether two members of unions share a value.
    fn members_overlap(&self, a: TypeId, b: TypeId) -> bool {
        // What a member is as a primitive: a literal its primitive, `int32` itself.
        let primitive = |id: TypeId| match self.types.get(id) {
            Type::Literal(literal) => Some(literal.primitive()),
            Type::Number
            | Type::Int32
            | Type::BigInt
            | Type::String
            | Type::Boolean
            | Type::Undefined
            | Type::Null => Some(id),
            _ => None,
        };
        let literal = |id: TypeId| matches!(self.types.get(id), Type::Literal(_));
        let anything = |id: TypeId| matches!(self.types.get(id), Type::Unknown | Type::Var(_));
        let number = |id: TypeId| [TypeId::NUMBER, TypeId::INT32].contains(&id);
        match (primitive(a), primitive(b)) {
            _ if a == b || anything(a) || anything(b) => true,
            // Two literals are two values.
            (Some(_), Some(_)) if literal(a) && literal(b) => false,
            (Some(x), Some(y)) => {
                x == y
                    || (number(x)
                        && number(y)
                        && !(literal(a) && y == TypeId::INT32)
                        && !(literal(b) && x == TypeId::INT32))
            }
            // An object of any kind may be the same object as another.
            (None, None) => true,
            _ => false,
        }
    }

    /// Whether two types share a value: `===` between two that share none is never
    /// true.
    fn overlap(&self, a: TypeId, b: TypeId) -> bool {
        let members = |id: TypeId| match self.types.get(id) {
            Type::Union(members) => members.clone(),
            _ => vec![id],
        };
        let others = members(b);
        members(a)
            .iter()
            .any(|&x| others.iter().any(|&y| self.members_overlap(x, y)))
    }

    /// `-x`, `+x`, `~x`, `!x`, `typeof x`. The minus of a number or bigint literal is
    /// the literal of the negated value, so `-2147483648` is an `int32` and `-0` is
    /// not.
    pub(crate) fn unary(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) -> Option<TypeId> {
        let first = self.first_time(node);
        let operand = children.last().copied();
        let ty = operand.and_then(|operand| self.type_of(operand));
        let at = operand.as_ref().map_or(node.span(), element_span_of);
        // The operator is the first token of the expression.
        let operator = node.tokens().first();
        let symbol = operator.and_then(|operator| self.source.slice(&operator.span));
        let symbol = symbol.unwrap_or_default().to_owned();
        let operator = operator.map(|operator| operator.kind.clone());
        let mismatch = |typer: &mut Self, takes: &str, ty: TypeId| {
            let message = format!(
                "`{symbol}` takes {takes}: this is a `{}` (SPEC.md §4.1)",
                typer.print_type(ty)
            );
            typer.report(first, Code::E138, at, message);
        };
        match operator.unwrap_or(TokenKind::Eof) {
            TokenKind::Keyword(Keyword::Typeof) => {
                let members = ty.map_or(vec![TypeId::UNKNOWN], |ty| self.members_of(ty));
                let mut names: Vec<&str> = Vec::new();
                for member in members {
                    let given: &[&str] = match self.types.get(member) {
                        Type::Number | Type::Int32 | Type::Literal(Literal::Number(_)) => {
                            &["number"]
                        }
                        Type::BigInt | Type::Literal(Literal::BigInt(_)) => &["bigint"],
                        Type::String | Type::Literal(Literal::String(_)) => &["string"],
                        Type::Boolean | Type::Literal(Literal::Boolean(_)) => &["boolean"],
                        Type::Undefined => &["undefined"],
                        Type::Function(_) => &["function"],
                        Type::Unknown | Type::Var(_) | Type::Never => &TYPEOF,
                        _ => &["object"],
                    };
                    names.extend(given);
                }
                let literals: Vec<TypeId> = names
                    .into_iter()
                    .map(|name| self.types.literal(Literal::String(name.to_owned())))
                    .collect();
                Some(self.types.union(literals))
            }
            TokenKind::Punct(Punct::Bang) => {
                let wrong = ty.filter(|&ty| self.kind(ty) != Kind::Boolean);
                if let Some(wrong) = wrong {
                    mismatch(self, "a boolean", wrong);
                }
                Some(TypeId::BOOLEAN)
            }
            TokenKind::Punct(Punct::Tilde) => {
                let ty = ty?;
                match self.kind(ty) {
                    Kind::Int32 => Some(TypeId::INT32),
                    Kind::BigInt => Some(TypeId::BIGINT),
                    Kind::Number => {
                        let message = format!(
                            "`~` on a `{}`: a bitwise operator takes an `int32` or a bigint, and a number that may be no integer in range is neither (SPEC.md §3.5)",
                            self.print_type(ty)
                        );
                        self.report(first, Code::E104, at, message);
                        None
                    }
                    _ => {
                        mismatch(self, "an int32 or a bigint", ty);
                        None
                    }
                }
            }
            TokenKind::Punct(sign @ (Punct::Minus | Punct::Plus)) => {
                let ty = ty?;
                let negates = sign == Punct::Minus;
                match self.types.get(ty).clone() {
                    Type::Literal(Literal::Number(number)) if negates => Some(
                        self.types
                            .literal(Literal::Number(Number::new(-number.value()))),
                    ),
                    Type::Literal(Literal::BigInt(integer)) if negates => {
                        let decimal = integer.decimal();
                        let opposite = decimal
                            .strip_prefix('-')
                            .map_or(format!("-{decimal}"), str::to_owned);
                        Integer::parse(&opposite)
                            .map(|integer| self.types.literal(Literal::BigInt(integer)))
                    }
                    _ => match self.kind(ty) {
                        // The negation of the least `int32` is no `int32`.
                        Kind::Number | Kind::Int32 => Some(TypeId::NUMBER),
                        Kind::BigInt => Some(TypeId::BIGINT),
                        _ => {
                            mismatch(self, "a number or a bigint", ty);
                            None
                        }
                    },
                }
            }
            _ => None,
        }
    }

    /// `a ? b : c`: the condition is a boolean; the union of the branches.
    pub(crate) fn conditional(
        &mut self,
        node: NodeRef<'_>,
        children: &[Element<'_>],
    ) -> Option<TypeId> {
        let first = self.first_time(node);
        let part = |at: usize| children.get(at).copied();
        let condition = part(0).and_then(|condition| Some((condition, self.type_of(condition)?)));
        if let Some((condition, ty)) = condition.filter(|&(_, ty)| self.kind(ty) != Kind::Boolean) {
            let message = format!(
                "the condition of `?:` is a boolean: this is a `{}` (SPEC.md §4.1)",
                self.print_type(ty)
            );
            self.report(first, Code::E138, element_span_of(&condition), message);
        }
        let then = part(2).and_then(|then| self.type_of(then));
        let otherwise = part(4).and_then(|otherwise| self.type_of(otherwise));
        then.zip(otherwise)
            .map(|(then, otherwise)| self.types.union([then, otherwise]))
    }

    /// A binary operator: what it gives, and whether it takes what it is given.
    pub(crate) fn binary(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) -> Option<TypeId> {
        let first = self.first_time(node);
        // The operands are the first and the last part, the operator between them.
        let last = children.len() - 1;
        let (left, operator, right) = (children[0], children[last / 2], children[last]);
        let symbol = self
            .source
            .slice(&element_span_of(&operator))
            .unwrap_or_default()
            .to_owned();
        let (lt, rt) = (self.type_of(left), self.type_of(right));
        let at = element_span_of(&operator);
        let Some(sign) = punct(Some(&operator)) else {
            // `instanceof`: a typed value on the right that is no class.
            if let Some(ty) = rt.filter(|&ty| !self.is_class(ty)) {
                let message = format!(
                    "the right of `instanceof` is a class: this is a `{}` (SPEC.md §4.1)",
                    self.print_type(ty)
                );
                self.report(first, Code::E138, element_span_of(&right), message);
            }
            return Some(TypeId::BOOLEAN);
        };
        let compares = matches!(
            sign,
            Punct::EqEqEq | Punct::BangEqEq | Punct::Lt | Punct::Gt | Punct::LtEq | Punct::GtEq
        );
        if compares && is_place(&left) && self.text(&left) == self.text(&right) {
            let message = format!(
                "`{}` is compared with itself: the answer is always the same (SPEC.md §8.1)",
                self.text(&left)
            );
            self.report(first, Code::E120, node.span(), message);
            return Some(TypeId::BOOLEAN);
        }
        match sign {
            Punct::AmpAmp | Punct::PipePipe => {
                for (side, ty) in [(left, lt), (right, rt)] {
                    let wrong = ty.filter(|&ty| self.kind(ty) != Kind::Boolean);
                    if let Some(wrong) = wrong {
                        let message = format!(
                            "`{symbol}` takes booleans: this is a `{}`; use `??` for a default and `?:` to choose (SPEC.md §4.1)",
                            self.print_type(wrong)
                        );
                        self.report(first, Code::E138, element_span_of(&side), message);
                        break;
                    }
                }
                Some(TypeId::BOOLEAN)
            }
            Punct::QuestionQuestion => {
                let (lt, rt) = (lt?, rt?);
                let present = self.present(lt);
                if present == lt {
                    let message = format!(
                        "the left of `??` is a `{}`, which is never `undefined` or `null`: the right is never used (SPEC.md §4.1)",
                        self.print_type(lt)
                    );
                    self.report(first, Code::E138, element_span_of(&left), message);
                    return Some(lt);
                }
                Some(self.types.union([present, rt]))
            }
            Punct::EqEqEq | Punct::BangEqEq => {
                if let Some((lt, rt)) = lt.zip(rt).filter(|&(lt, rt)| !self.overlap(lt, rt)) {
                    let message = format!(
                        "a `{}` and a `{}` share no value: `{symbol}` between them always gives the same answer (SPEC.md §4.1)",
                        self.print_type(lt),
                        self.print_type(rt)
                    );
                    self.report(first, Code::E105, at, message);
                }
                Some(TypeId::BOOLEAN)
            }
            _ => {
                let Some(sides) = lt.zip(rt) else {
                    return compares.then_some(TypeId::BOOLEAN);
                };
                self.numeric(first, (&operator, sign), sides, &[left, right])
            }
        }
    }

    /// An arithmetic, bitwise or ordering operator on two typed operands.
    fn numeric(
        &mut self,
        first: bool,
        (operator, sign): (&Element<'_>, Punct),
        (lt, rt): (TypeId, TypeId),
        operands: &[Element<'_>],
    ) -> Option<TypeId> {
        let at = element_span_of(operator);
        let symbol = self.source.slice(&at).unwrap_or_default().to_owned();
        let compares = matches!(sign, Punct::Lt | Punct::Gt | Punct::LtEq | Punct::GtEq);
        // What never comes gives nothing: a function seen calling itself, at first.
        if lt == TypeId::NEVER || rt == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }
        let kinds = (self.kind(lt), self.kind(rt));
        let numbers = |kind: Kind| matches!(kind, Kind::Number | Kind::Int32);
        let both_numbers = numbers(kinds.0) && numbers(kinds.1);
        let both = |kind: Kind| kinds == (kind, kind);
        let bitwise = matches!(
            sign,
            Punct::Amp | Punct::Pipe | Punct::Caret | Punct::Shl | Punct::Shr | Punct::UShr
        );
        let (result, takes) = match sign {
            Punct::Lt | Punct::Gt | Punct::LtEq | Punct::GtEq => {
                // A parameter nothing calls is whichever the other side is.
                let any = operands.iter().any(|operand| self.any_order(operand));
                let fits = both_numbers || both(Kind::BigInt) || both(Kind::String) || any;
                (
                    fits.then_some(TypeId::BOOLEAN),
                    "two numbers, two bigints or two strings",
                )
            }
            Punct::Plus if both(Kind::String) => (Some(TypeId::STRING), ""),
            Punct::UShr => (both(Kind::Int32).then_some(TypeId::NUMBER), "two int32s"),
            _ if bitwise => {
                let result = if both(Kind::Int32) {
                    Some(TypeId::INT32)
                } else {
                    both(Kind::BigInt).then_some(TypeId::BIGINT)
                };
                (result, "two int32s or two bigints")
            }
            Punct::Percent if both(Kind::Int32) => (Some(TypeId::INT32), ""),
            // `+ - * / % **`: what is left of the operators there are.
            _ => {
                let result = if both_numbers {
                    Some(TypeId::NUMBER)
                } else {
                    both(Kind::BigInt).then_some(TypeId::BIGINT)
                };
                let takes = if sign == Punct::Plus {
                    "two numbers, two bigints or two strings"
                } else {
                    "two numbers or two bigints"
                };
                (result, takes)
            }
        };
        if result.is_none() {
            let (code, message) = if bitwise && both_numbers {
                (Code::E104, format!(
                            "`{symbol}` on a `{}` and a `{}`: a bitwise operator takes int32s, and a number that may be no integer in range is none (SPEC.md §3.5)",
                            self.print_type(lt), self.print_type(rt)
                        ))
            } else {
                (
                    Code::E138,
                    format!(
                        "`{symbol}` takes {takes}: these are a `{}` and a `{}` (SPEC.md §4.1)",
                        self.print_type(lt),
                        self.print_type(rt)
                    ),
                )
            };
            self.report_about(first, code, at, message, operands);
        }
        result.or(compares.then_some(TypeId::BOOLEAN))
    }

    /// E120 for `x = x` and `a.b = a.b`: a place assigned to itself.
    pub(crate) fn self_assignment(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        let [left, operator, right] = children else {
            return;
        };
        let plain = node.kind() == NodeKind::AssignExpr && punct(Some(operator)) == Some(Punct::Eq);
        if plain && is_place(left) && self.text(left) == self.text(right) {
            let message = format!("`{}` is assigned to itself (SPEC.md §8.1)", self.text(left));
            let span = node.span();
            let first = self.judged.insert((span.byte_start, span.byte_start));
            self.report(first, Code::E120, span, message);
        }
    }
}
