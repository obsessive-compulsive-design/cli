//! Numeric literals (SPEC.md §2): decimal, `0x`/`0o`/`0b`, exponents, `_` separators and
//! the `n` suffix, each with its value. The scanner follows the ES2025 grammar and names
//! the exact character that breaks it.

use std::fmt::Write as _;

use crate::token::{Number, TokenKind};

/// Why a literal was rejected: E000 at the character at `at`.
pub(crate) struct Rejection {
    pub at: usize,
    pub message: String,
}

/// Significant digits a `0x`/`0o`/`0b` literal may have (SPEC.md §2 sets no limit; this
/// keeps conversion time bounded, and 4 096 hexadecimal digits already exceed any
/// double). Decimal literals have no cap: they never need conversion.
pub const MAX_RADIX_DIGITS: usize = 4096;

/// `Number.MAX_SAFE_INTEGER`: doubles represent every integer up to here.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// Scans the literal at `start`, which begins with a digit or with `.` then a digit.
/// Returns where scanning stopped and the token, or the rejection.
pub(crate) fn scan(text: &str, start: usize) -> (usize, Result<TokenKind, Rejection>) {
    let mut scanner = Scanner { text, pos: start };
    let result = scanner.literal(start);
    (scanner.pos, result)
}

fn reject(at: usize, message: String) -> Rejection {
    Rejection { at, message }
}

fn number(value: f64) -> TokenKind {
    TokenKind::Number(Number {
        value,
        exceeds_safe_integer: value.trunc().to_bits() == value.to_bits()
            && value.abs() > MAX_SAFE_INTEGER,
    })
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

impl Scanner<'_> {
    fn at(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn literal(&mut self, start: usize) -> Result<TokenKind, Rejection> {
        let kind = match &self.text.as_bytes()[start..] {
            [b'0', b'x' | b'X', ..] => self.radix_literal(start, 16, "hexadecimal")?,
            [b'0', b'o' | b'O', ..] => self.radix_literal(start, 8, "octal")?,
            [b'0', b'b' | b'B', ..] => self.radix_literal(start, 2, "binary")?,
            _ => self.decimal_literal(start)?,
        };
        let next = self.text[self.pos..].chars().next();
        if next.is_some_and(|c| c == '_' || c == '$' || c == '\\' || c.is_alphanumeric()) {
            return Err(reject(
                self.pos,
                "a numeric literal must not run into an identifier or another digit".to_owned(),
            ));
        }
        Ok(kind)
    }

    /// `0x…`, `0o…` or `0b…`: the digits are converted exactly, then rounded once.
    fn radix_literal(
        &mut self,
        start: usize,
        radix: u32,
        name: &str,
    ) -> Result<TokenKind, Rejection> {
        self.pos = start + 2;
        let digits = self.digits(radix)?;
        if digits.is_empty() {
            let prefix = &self.text[start..self.pos];
            return Err(reject(
                self.pos,
                format!("expected a {name} digit after `{prefix}`"),
            ));
        }
        // Exact conversion is quadratic in the digit count, so a literal of any length
        // must not reach it: past the cap a double is infinite anyway, and a bigint that
        // long is rejected rather than converted.
        let digits = digits.trim_start_matches('0');
        if digits.len() > MAX_RADIX_DIGITS {
            if self.at() == Some(b'n') {
                return Err(reject(
                    start,
                    format!("a bigint literal in a radix may have at most {MAX_RADIX_DIGITS} significant digits"),
                ));
            }
            return Ok(number(f64::INFINITY));
        }
        Ok(self.finish_integer(to_decimal(digits, radix)))
    }

    fn decimal_literal(&mut self, start: usize) -> Result<TokenKind, Rejection> {
        let int = self.digits(10)?;
        if int.len() > 1 && int.starts_with('0') {
            return Err(self.leading_zero(start, &int));
        }
        let mut integer = true;
        if self.at() == Some(b'.') {
            integer = false;
            self.pos += 1;
            self.digits(10)?;
        }
        if matches!(self.at(), Some(b'e' | b'E')) {
            integer = false;
            self.pos += 1;
            if matches!(self.at(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits(10)?.is_empty() {
                return Err(reject(self.pos, "expected an exponent digit".to_owned()));
            }
        }
        if integer {
            return Ok(self.finish_integer(int));
        }
        if self.at() == Some(b'n') {
            return Err(reject(
                self.pos,
                "a bigint literal must be an integer: no fraction and no exponent".to_owned(),
            ));
        }
        let clean: String = self.text[start..self.pos]
            .chars()
            .filter(|c| *c != '_')
            .collect();
        Ok(number(clean.parse().unwrap_or(f64::INFINITY)))
    }

    /// `017` is a legacy octal literal and `08` a strict-mode error; `0_1` is a misplaced
    /// separator. Each is E000 at the exact character.
    fn leading_zero(&self, start: usize, int: &str) -> Rejection {
        let raw = &self.text[start..self.pos];
        if raw.as_bytes()[1] == b'_' {
            return reject(
                start + 1,
                "a numeric separator `_` must sit between two digits".to_owned(),
            );
        }
        let trimmed = int.trim_start_matches('0');
        if trimmed.is_empty() {
            reject(start, format!("`{raw}` has a leading zero; write `0`"))
        } else if trimmed.bytes().all(|b| b < b'8') {
            reject(
                start,
                format!("`{raw}` is a legacy octal literal; write `0o{trimmed}`"),
            )
        } else {
            reject(
                start,
                format!("`{raw}` has a leading zero; write `{trimmed}`"),
            )
        }
    }

    /// The `n` suffix makes a bigint of the decimal digits; otherwise they round to a double.
    fn finish_integer(&mut self, decimal: String) -> TokenKind {
        if self.at() == Some(b'n') {
            self.pos += 1;
            TokenKind::BigInt(decimal.into_boxed_str())
        } else {
            number(decimal.parse().unwrap_or(f64::INFINITY))
        }
    }

    /// Digits of `radix` with `_` separators, which must sit between two digits. Returns
    /// the digits without separators; empty when there are none.
    fn digits(&mut self, radix: u32) -> Result<String, Rejection> {
        const MESSAGE: &str = "a numeric separator `_` must sit between two digits";
        let mut out = String::new();
        let mut separator = None;
        loop {
            match self.at() {
                Some(b) if (b as char).is_digit(radix) => {
                    out.push(b as char);
                    separator = None;
                }
                Some(b'_') => {
                    if out.is_empty() || separator.is_some() {
                        return Err(reject(self.pos, MESSAGE.to_owned()));
                    }
                    separator = Some(self.pos);
                }
                _ => break,
            }
            self.pos += 1;
        }
        match separator {
            Some(at) => Err(reject(at, MESSAGE.to_owned())),
            None => Ok(out),
        }
    }
}

/// The decimal spelling of `digits` read in `radix`, with no leading zeros: arbitrary
/// precision in base 10⁹ limbs, so bigints and large doubles are exact before rounding.
fn to_decimal(digits: &str, radix: u32) -> String {
    const BASE: u64 = 1_000_000_000;
    let mut limbs: Vec<u32> = vec![0];
    for digit in digits.chars() {
        let mut carry = digit.to_digit(radix).unwrap_or(0);
        for limb in &mut limbs {
            let value = u64::from(*limb) * u64::from(radix) + u64::from(carry);
            *limb = u32::try_from(value % BASE).unwrap_or(u32::MAX);
            carry = u32::try_from(value / BASE).unwrap_or(u32::MAX);
        }
        if carry > 0 {
            limbs.push(carry);
        }
    }
    let mut out = String::new();
    for (index, limb) in limbs.iter().rev().enumerate() {
        if index == 0 {
            let _ = write!(out, "{limb}");
        } else {
            let _ = write!(out, "{limb:09}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_decimal;

    #[test]
    fn to_decimal_of_no_digits_is_zero() {
        assert_eq!(to_decimal("", 16), "0", "the caller strips leading zeros");
    }

    #[test]
    fn to_decimal_is_exact_at_every_width() {
        assert_eq!(to_decimal("0", 16), "0");
        assert_eq!(to_decimal("000", 2), "0");
        assert_eq!(to_decimal("ff", 16), "255");
        assert_eq!(to_decimal("FF", 16), "255");
        assert_eq!(to_decimal("17", 8), "15");
        assert_eq!(to_decimal("101", 2), "5");
        assert_eq!(to_decimal("3B9ACA00", 16), "1000000000", "exactly one limb");
        assert_eq!(
            to_decimal("3B9ACA05", 16),
            "1000000005",
            "a low limb is zero-padded"
        );
        assert_eq!(
            to_decimal("10000000000000000", 16),
            "18446744073709551616",
            "2^64"
        );
        assert_eq!(
            to_decimal("FFFFFFFFFFFFFFFFFFFF", 16),
            "1208925819614629174706175",
            "2^80 - 1"
        );
        let two_to_the_100 = format!("1{}", "0".repeat(100));
        assert_eq!(
            to_decimal(&two_to_the_100, 2),
            "1267650600228229401496703205376",
            "2^100"
        );
    }
}
