//! The values literal types are of.

use crate::TypeId;
use std::cmp::Ordering;

/// A `number` value, compared as the bits of its double: `0` and `-0` differ, and
/// every `NaN` is one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Number(u64);

impl Number {
    /// The number.
    #[must_use]
    pub fn new(value: f64) -> Self {
        Self(if value.is_nan() { f64::NAN } else { value }.to_bits())
    }

    /// Its value.
    #[must_use]
    pub fn value(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Whether it is an `int32`: an integer in [-2³¹, 2³¹), and not `-0`.
    #[must_use]
    pub fn is_int32(self) -> bool {
        let value = self.value();
        let range = f64::from(i32::MIN)..=f64::from(i32::MAX);
        range.contains(&value)
            && value.fract() == 0.0
            && !(value == 0.0 && value.is_sign_negative())
    }
}

impl Ord for Number {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value().total_cmp(&other.value())
    }
}

impl PartialOrd for Number {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// An integer of any size, as a `bigint` literal holds one.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Integer {
    negative: bool,
    /// Decimal digits without leading zeros; `0` for zero, which is never negative.
    digits: String,
}

impl Integer {
    /// The integer written in decimal, with `-` before it or not; `None` for text
    /// that is no such number. `007` is `7` and `-0` is `0`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(digits) => (true, digits),
            None => (false, text),
        };
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let significant = digits.trim_start_matches('0');
        Some(Self {
            negative: negative && !significant.is_empty(),
            digits: if significant.is_empty() {
                "0"
            } else {
                significant
            }
            .to_owned(),
        })
    }

    /// The integer in decimal.
    #[must_use]
    pub fn decimal(&self) -> String {
        format!("{}{}", if self.negative { "-" } else { "" }, self.digits)
    }
}

impl Ord for Integer {
    fn cmp(&self, other: &Self) -> Ordering {
        let size = |integer: &Self| (integer.digits.len(), integer.digits.clone());
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => size(self).cmp(&size(other)),
            (true, true) => size(other).cmp(&size(self)),
        }
    }
}

impl PartialOrd for Integer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The value of a literal type. As values they are ordered numbers by value, then
/// bigints by value, strings, `false`, `true`; the canonical order of types is the
/// same but for `true`, which comes before `false`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Literal {
    /// A number: `1`, `1.5`.
    Number(Number),
    /// A bigint: `1n`.
    BigInt(Integer),
    /// A string.
    String(String),
    /// `true` or `false`.
    Boolean(bool),
}

impl Literal {
    /// The primitive the literal is a value of, which absorbs it in a union: `int32`
    /// for a number that is one, else `number`.
    #[must_use]
    pub fn primitive(&self) -> TypeId {
        match self {
            Literal::Number(number) if number.is_int32() => TypeId::INT32,
            Literal::Number(_) => TypeId::NUMBER,
            Literal::BigInt(_) => TypeId::BIGINT,
            Literal::String(_) => TypeId::STRING,
            Literal::Boolean(_) => TypeId::BOOLEAN,
        }
    }
}
