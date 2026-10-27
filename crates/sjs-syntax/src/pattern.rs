//! Regex pattern syntax under the `v` flag (SPEC.md §2, §6.1 `RegExp`): the ES2025
//! grammar with `[+UnicodeMode, +UnicodeSetsMode, +NamedCaptureGroups]`, checked so a
//! malformed pattern is a compile error here rather than a `SyntaxError` at runtime.
//! The parser validates and collects the named groups; it builds no tree and matches
//! nothing.
//!
//! Group names approximate `ID_Start`/`ID_Continue` with the standard library's
//! letter and number classes plus `$`, `_`, ZWNJ and ZWJ.

use std::collections::BTreeSet;

use crate::unicode_property as property;

/// What a well-formed pattern declares.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pattern {
    /// Named groups in source order; a name appears twice when two alternatives
    /// declare it (ES2025 duplicate named groups).
    pub groups: Vec<String>,
    /// Capturing groups, named or not.
    pub capture_count: u32,
}

/// Why a pattern is rejected: E000 at the character at `offset` within the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternError {
    /// Byte offset within the body.
    pub offset: usize,
    /// What is wrong.
    pub message: String,
}

/// Parses `body`, the text between the slashes of a `v`-flagged literal.
///
/// # Errors
///
/// The first syntax error in source order, with its offset.
pub fn parse(body: &str) -> Result<Pattern, PatternError> {
    let mut parser = Parser {
        text: body,
        pos: 0,
        groups: Vec::new(),
        scopes: vec![BTreeSet::new()],
        capture_count: 0,
        references: Vec::new(),
    };
    parser.disjunction()?;
    if parser.pos < body.len() {
        return Err(error(parser.pos, "unmatched `)`"));
    }
    parser.check_references()?;
    Ok(Pattern {
        groups: parser.groups,
        capture_count: parser.capture_count,
    })
}

/// A back-reference, checked once every group is known.
enum Reference {
    Number(u64),
    Name(String),
}

/// A class element: an operand or a range.
struct Element {
    /// May match strings (sequences of several code points), which a negated class
    /// cannot hold.
    strings: bool,
    is_range: bool,
    at: usize,
}

/// A class operand: a nested class, a class escape, a string disjunction or one
/// character (whose code point matters for ranges).
struct Operand {
    strings: bool,
    single: Option<u32>,
}

#[derive(Clone, Copy)]
enum SetOperator {
    Intersection,
    Subtraction,
}

const BRACE_SHAPE: &str =
    "`{` must be escaped as `\\{` unless it is a quantifier `{n}`, `{n,}` or `{n,m}`";

fn error(offset: usize, message: impl Into<String>) -> PatternError {
    PatternError {
        offset,
        message: message.into(),
    }
}

fn is_syntax_character(ch: char) -> bool {
    "^$\\.*+?()[]{}|".contains(ch)
}

/// `ClassSetSyntaxCharacter`: never bare inside a class.
fn is_class_syntax_character(ch: char) -> bool {
    "()[]{}/-\\|".contains(ch)
}

/// `ClassSetReservedDoublePunctuator`: a pair of these is reserved inside a class.
fn is_reserved_double(ch: char) -> bool {
    "&!#$%*+,.:;<=>?@^`~".contains(ch)
}

/// `ClassSetReservedPunctuator`: escapable inside a class.
fn is_reserved_punctuator(ch: char) -> bool {
    "&-!#%,:;<=>@`~".contains(ch)
}

fn is_id_start(ch: char) -> bool {
    ch.is_alphabetic() || ch == '$' || ch == '_'
}

fn is_id_continue(ch: char) -> bool {
    is_id_start(ch) || ch.is_numeric() || ch == '\u{200C}' || ch == '\u{200D}'
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
    groups: Vec<String>,
    /// The enclosing alternatives' declared names, innermost last; the root scope is
    /// never popped.
    scopes: Vec<BTreeSet<String>>,
    capture_count: u32,
    references: Vec<(usize, Reference)>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn peek_second(&self) -> Option<char> {
        self.text[self.pos..].chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.pos += ch.len_utf8();
            true
        } else {
            false
        }
    }

    fn eat_str(&mut self, s: &str) -> bool {
        if self.text[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.text[self.pos..].starts_with(s)
    }

    /// `Alternative (| Alternative)*`; the names declared in it join the enclosing
    /// alternative's scope afterwards.
    fn disjunction(&mut self) -> Result<(), PatternError> {
        let mut names = BTreeSet::new();
        loop {
            self.scopes.push(BTreeSet::new());
            self.alternative()?;
            names.extend(self.scopes.pop().unwrap_or_default());
            if !self.eat('|') {
                break;
            }
        }
        let top = self.scopes.len().saturating_sub(1);
        self.scopes[top].extend(names);
        Ok(())
    }

    fn alternative(&mut self) -> Result<(), PatternError> {
        while let Some(ch) = self.peek() {
            if ch == '|' || ch == ')' {
                break;
            }
            self.term(ch)?;
        }
        Ok(())
    }

    /// One assertion or one atom with its quantifier; `ch` is the next character.
    fn term(&mut self, ch: char) -> Result<(), PatternError> {
        let start = self.pos;
        let quantifiable = match ch {
            '^' | '$' => {
                self.pos += 1;
                false
            }
            '\\' if matches!(self.peek_second(), Some('b' | 'B')) => {
                self.pos += 2;
                false
            }
            '(' if self.starts_with("(?=") || self.starts_with("(?!") => {
                self.pos += 3;
                self.disjunction()?;
                self.expect_close(start)?;
                false
            }
            '(' if self.starts_with("(?<=") || self.starts_with("(?<!") => {
                self.pos += 4;
                self.disjunction()?;
                self.expect_close(start)?;
                false
            }
            '(' => {
                self.group(start)?;
                true
            }
            '\\' => {
                self.pos += 1;
                self.atom_escape(start)?;
                true
            }
            '[' => {
                self.class()?;
                true
            }
            '*' | '+' | '?' => {
                return Err(error(start, format!("nothing to repeat before `{ch}`")));
            }
            '{' if self.quantifier_ahead() => {
                return Err(error(start, "nothing to repeat before `{`"));
            }
            '{' => return Err(error(start, BRACE_SHAPE)),
            '}' | ']' => {
                return Err(error(start, format!("`{ch}` must be escaped as `\\{ch}`")));
            }
            _ => {
                // `.` and any pattern character.
                self.pos += ch.len_utf8();
                true
            }
        };
        if quantifiable {
            self.quantifier()
        } else if matches!(self.peek(), Some('*' | '+' | '?'))
            || (self.peek() == Some('{') && self.quantifier_ahead())
        {
            Err(error(self.pos, "an assertion cannot be quantified"))
        } else {
            Ok(())
        }
    }

    fn expect_close(&mut self, start: usize) -> Result<(), PatternError> {
        if self.eat(')') {
            Ok(())
        } else {
            Err(error(start, "unterminated group: expected `)`"))
        }
    }

    /// `*`, `+`, `?`, `{n}`, `{n,}` or `{n,m}`, each optionally lazy with `?`.
    fn quantifier(&mut self) -> Result<(), PatternError> {
        let start = self.pos;
        match self.peek() {
            Some('*' | '+' | '?') => self.pos += 1,
            Some('{') => self.braces(start)?,
            _ => return Ok(()),
        }
        self.eat('?');
        Ok(())
    }

    /// True when a well-formed `{n}`, `{n,}` or `{n,m}` starts here.
    fn quantifier_ahead(&self) -> bool {
        let rest = &self.text[self.pos + 1..];
        let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        if digits == 0 {
            return false;
        }
        let after = &rest[digits..];
        let after = after.strip_prefix(',').map_or(after, |tail| {
            tail.trim_start_matches(|c: char| c.is_ascii_digit())
        });
        after.starts_with('}')
    }

    fn braces(&mut self, start: usize) -> Result<(), PatternError> {
        const SHAPE: &str = BRACE_SHAPE;
        self.pos += 1;
        let Some(min) = self.digits() else {
            return Err(error(start, SHAPE));
        };
        let max = if self.eat(',') {
            if self.peek() == Some('}') {
                None
            } else {
                let Some(max) = self.digits() else {
                    return Err(error(start, SHAPE));
                };
                Some(max)
            }
        } else {
            Some(min)
        };
        if !self.eat('}') {
            return Err(error(start, SHAPE));
        }
        if max.is_some_and(|max| max < min) {
            let text = &self.text[start..self.pos];
            return Err(error(start, format!("quantifier `{text}` is out of order")));
        }
        Ok(())
    }

    /// A run of decimal digits, saturating.
    fn digits(&mut self) -> Option<u64> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return None;
        }
        Some(self.text[start..self.pos].bytes().fold(0u64, |n, b| {
            n.saturating_mul(10).saturating_add(u64::from(b - b'0'))
        }))
    }

    /// `(…)`, `(?:…)`, `(?<name>…)` or `(?ims-ims:…)`, at `(`.
    fn group(&mut self, start: usize) -> Result<(), PatternError> {
        self.pos += 1;
        if self.eat('?') {
            if self.eat('<') {
                let at = self.pos;
                let name = self.group_name()?;
                self.declare(name, at)?;
                self.capture_count += 1;
            } else if !self.eat(':') {
                self.modifiers(start)?;
            }
        } else {
            self.capture_count += 1;
        }
        self.disjunction()?;
        self.expect_close(start)
    }

    /// `(?ims-ims:`: each of `i`, `m`, `s` at most once over both sets, at least one.
    fn modifiers(&mut self, start: usize) -> Result<(), PatternError> {
        let mut seen = String::new();
        let mut removing = false;
        loop {
            let at = self.pos;
            match self.bump() {
                Some(flag @ ('i' | 'm' | 's')) => {
                    if seen.contains(flag) {
                        return Err(error(at, format!("modifier `{flag}` is repeated")));
                    }
                    seen.push(flag);
                }
                Some('-') if !removing => removing = true,
                Some(':') => break,
                _ => {
                    return Err(error(
                        start,
                        "`(?` must continue with `:`, `<name>`, `=`, `!`, `<=`, `<!` or modifiers `i`, `m`, `s`",
                    ));
                }
            }
        }
        if seen.is_empty() {
            return Err(error(start, "`(?-:` sets no modifier"));
        }
        Ok(())
    }

    /// `name>` of a group name, after `<`.
    fn group_name(&mut self) -> Result<String, PatternError> {
        let name_start = self.pos;
        let mut name = String::new();
        loop {
            let at = self.pos;
            let ch = match self.bump() {
                Some('>') => break,
                Some('\\') => self.name_escape(at)?,
                Some(ch) => ch,
                None => {
                    return Err(error(name_start, "unterminated group name: expected `>`"));
                }
            };
            let permitted = if name.is_empty() {
                is_id_start(ch)
            } else {
                is_id_continue(ch)
            };
            if !permitted {
                return Err(error(
                    at,
                    format!("`{ch}` is not permitted in a group name"),
                ));
            }
            name.push(ch);
        }
        if name.is_empty() {
            return Err(error(name_start, "empty group name"));
        }
        Ok(name)
    }

    /// `\u…` inside a group name, after the backslash at `at`.
    fn name_escape(&mut self, at: usize) -> Result<char, PatternError> {
        if !self.eat('u') {
            return Err(error(
                at,
                "only `\\u` escapes are permitted in a group name",
            ));
        }
        let value = self.unicode_escape(at)?;
        char::from_u32(value)
            .ok_or_else(|| error(at, "a lone surrogate is not permitted in a group name"))
    }

    fn declare(&mut self, name: String, at: usize) -> Result<(), PatternError> {
        if self.scopes.iter().any(|scope| scope.contains(&name)) {
            return Err(error(
                at,
                format!("duplicate group name `{name}`: a name may recur only in different alternatives"),
            ));
        }
        let top = self.scopes.len().saturating_sub(1);
        self.scopes[top].insert(name.clone());
        self.groups.push(name);
        Ok(())
    }

    /// After a top-level backslash at `start`.
    fn atom_escape(&mut self, start: usize) -> Result<(), PatternError> {
        match self.peek() {
            Some('1'..='9') => {
                let number = self.digits().unwrap_or(0);
                self.references.push((start, Reference::Number(number)));
            }
            Some('k') => {
                self.pos += 1;
                if !self.eat('<') {
                    return Err(error(start, "`\\k` must be followed by `<name>`"));
                }
                let name = self.group_name()?;
                self.references.push((start, Reference::Name(name)));
            }
            Some('p' | 'P') => {
                self.property(start)?;
            }
            Some('d' | 'D' | 's' | 'S' | 'w' | 'W') => {
                self.bump();
            }
            _ => {
                self.character_escape(start)?;
            }
        }
        Ok(())
    }

    /// `CharacterEscape` after a backslash at `start`: its code point.
    fn character_escape(&mut self, start: usize) -> Result<u32, PatternError> {
        let value = match self.bump() {
            Some('f') => 0xC,
            Some('n') => 0xA,
            Some('r') => 0xD,
            Some('t') => 0x9,
            Some('v') => 0xB,
            Some('c') => match self.bump() {
                Some(letter) if letter.is_ascii_alphabetic() => u32::from(letter) % 32,
                _ => return Err(error(start, "`\\c` must be followed by a letter")),
            },
            Some('0') => {
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    return Err(error(
                        start,
                        "`\\0` cannot be followed by a digit: octal escapes are not permitted",
                    ));
                }
                0
            }
            Some('x') => self
                .hex(2)
                .ok_or_else(|| error(start, "`\\x` must be followed by two hexadecimal digits"))?,
            Some('u') => self.unicode_escape(start)?,
            Some(ch) if is_syntax_character(ch) || ch == '/' => u32::from(ch),
            Some(ch) => {
                return Err(error(
                    start,
                    format!(
                        "`\\{ch}` is not an escape: only syntax characters and `/` may be escaped"
                    ),
                ));
            }
            None => return Err(error(start, "`\\` at the end of the pattern")),
        };
        Ok(value)
    }

    /// Exactly `count` hexadecimal digits.
    fn hex(&mut self, count: usize) -> Option<u32> {
        let start = self.pos;
        for _ in 0..count {
            if !self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                return None;
            }
            self.pos += 1;
        }
        u32::from_str_radix(&self.text[start..self.pos], 16).ok()
    }

    /// After `\u`: `{…}` up to 10FFFF, or four hex digits, where a surrogate pair written
    /// as two escapes is one code point.
    fn unicode_escape(&mut self, start: usize) -> Result<u32, PatternError> {
        if self.eat('{') {
            let digits_start = self.pos;
            while self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.pos += 1;
            }
            let digits = &self.text[digits_start..self.pos];
            if digits.is_empty() || !self.eat('}') {
                return Err(error(
                    start,
                    "`\\u{` must hold hexadecimal digits and close with `}`",
                ));
            }
            let value = u32::from_str_radix(digits, 16).unwrap_or(u32::MAX);
            if value > 0x10_FFFF {
                return Err(error(start, "`\\u{…}` must be at most 10FFFF"));
            }
            return Ok(value);
        }
        let Some(unit) = self.hex(4) else {
            return Err(error(
                start,
                "`\\u` must be followed by four hexadecimal digits or `{…}`",
            ));
        };
        if (0xD800..0xDC00).contains(&unit) && self.starts_with("\\u") {
            let before = self.pos;
            self.pos += 2;
            match self.hex(4) {
                Some(low) if (0xDC00..0xE000).contains(&low) => {
                    return Ok(0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
                }
                _ => self.pos = before,
            }
        }
        Ok(unit)
    }

    /// `\p{…}` or `\P{…}` at `p`/`P`, the backslash at `start`. True for a property of
    /// strings.
    fn property(&mut self, start: usize) -> Result<bool, PatternError> {
        let negated = self.bump() == Some('P');
        if !self.eat('{') {
            return Err(error(start, "`\\p` must be followed by `{…}`"));
        }
        let Some(length) = self.text[self.pos..].find('}') else {
            return Err(error(start, "unterminated `\\p{`: expected `}`"));
        };
        let body = &self.text[self.pos..self.pos + length];
        self.pos += length + 1;
        let known = match body.split_once('=') {
            Some((name, value)) => property::is_pair(name, value),
            None => property::is_lone_name(body),
        };
        if known {
            return Ok(false);
        }
        if !property::is_string_property(body) {
            return Err(error(start, format!("unknown Unicode property `{body}`")));
        }
        if negated {
            return Err(error(
                start,
                format!("`\\P{{{body}}}` cannot negate a property of strings"),
            ));
        }
        Ok(true)
    }

    /// `[…]` or `[^…]` at `[`. True when the class may match strings.
    fn class(&mut self) -> Result<bool, PatternError> {
        let start = self.pos;
        self.pos += 1;
        let negated = self.eat('^');
        let strings = self.class_contents()?;
        if !self.eat(']') {
            return Err(error(start, "unterminated character class: expected `]`"));
        }
        if negated && strings {
            return Err(error(
                start,
                "a negated class `[^…]` cannot contain strings",
            ));
        }
        Ok(strings)
    }

    /// `ClassSetExpression`: a union, an intersection or a subtraction.
    fn class_contents(&mut self) -> Result<bool, PatternError> {
        if matches!(self.peek(), Some(']') | None) {
            return Ok(false);
        }
        let first = self.class_element()?;
        if self.starts_with("&&") {
            return self.set_operation(&first, SetOperator::Intersection);
        }
        if self.starts_with("--") {
            return self.set_operation(&first, SetOperator::Subtraction);
        }
        let mut strings = first.strings;
        while !matches!(self.peek(), Some(']') | None) {
            if self.starts_with("&&") || self.starts_with("--") {
                return Err(error(
                    self.pos,
                    "a set operator cannot follow a union: bracket the operands, as in `[[ab]&&[bc]]`",
                ));
            }
            strings |= self.class_element()?.strings;
        }
        Ok(strings)
    }

    /// `first && operand (&& operand)*` or the same with `--`.
    fn set_operation(
        &mut self,
        first: &Element,
        operator: SetOperator,
    ) -> Result<bool, PatternError> {
        let (token, symbol) = match operator {
            SetOperator::Intersection => ("&&", '&'),
            SetOperator::Subtraction => ("--", '-'),
        };
        if first.is_range {
            return Err(error(
                first.at,
                format!("a range cannot be an operand of `{token}`: bracket it, as in `[[a-z]--[aeiou]]`"),
            ));
        }
        let mut strings = first.strings;
        while self.eat_str(token) {
            if self.peek() == Some(symbol) {
                return Err(error(
                    self.pos,
                    format!("`{token}{symbol}` is not permitted"),
                ));
            }
            let operand = self.class_operand()?;
            strings = match operator {
                SetOperator::Intersection => strings && operand.strings,
                SetOperator::Subtraction => strings,
            };
        }
        match self.peek() {
            Some(']') | None => Ok(strings),
            _ => Err(error(
                self.pos,
                format!("expected `{token}` or `]`: operators cannot be mixed without brackets"),
            )),
        }
    }

    /// A `ClassSetRange` or a `ClassSetOperand`.
    fn class_element(&mut self) -> Result<Element, PatternError> {
        let at = self.pos;
        let operand = self.class_operand()?;
        if self.peek() != Some('-') || self.peek_second() == Some('-') {
            return Ok(Element {
                strings: operand.strings,
                is_range: false,
                at,
            });
        }
        let dash = self.pos;
        let Some(low) = operand.single else {
            return Err(error(
                dash,
                "a range needs a single character on each side of `-`",
            ));
        };
        self.pos += 1;
        if matches!(self.peek(), Some(']') | None) {
            return Err(error(
                dash,
                "`-` must be escaped as `\\-` unless it forms a range",
            ));
        }
        let high_at = self.pos;
        let Some(high) = self.class_operand()?.single else {
            return Err(error(
                high_at,
                "a range needs a single character on each side of `-`",
            ));
        };
        if low > high {
            let text = &self.text[at..self.pos];
            return Err(error(at, format!("range `{text}` is out of order")));
        }
        Ok(Element {
            strings: false,
            is_range: true,
            at,
        })
    }

    /// A nested class, a class escape, a string disjunction or one character.
    fn class_operand(&mut self) -> Result<Operand, PatternError> {
        let at = self.pos;
        match self.peek() {
            Some('[') => Ok(Operand {
                strings: self.class()?,
                single: None,
            }),
            Some('\\') => match self.peek_second() {
                Some('q') => {
                    self.pos += 2;
                    Ok(Operand {
                        strings: self.class_strings(at)?,
                        single: None,
                    })
                }
                Some('d' | 'D' | 's' | 'S' | 'w' | 'W') => {
                    self.pos += 2;
                    Ok(Operand {
                        strings: false,
                        single: None,
                    })
                }
                Some('p' | 'P') => {
                    self.pos += 1;
                    Ok(Operand {
                        strings: self.property(at)?,
                        single: None,
                    })
                }
                _ => {
                    self.pos += 1;
                    Ok(Operand {
                        strings: false,
                        single: Some(self.class_set_character(at, '\\')?),
                    })
                }
            },
            Some(']') => Err(error(at, "expected a class operand before `]`")),
            None => Err(error(
                at,
                "expected a class operand before the end of the pattern",
            )),
            Some(ch) => {
                self.pos += ch.len_utf8();
                Ok(Operand {
                    strings: false,
                    single: Some(self.class_set_character(at, ch)?),
                })
            }
        }
    }

    /// `\q{…|…}` after `\q`, the backslash at `at`. True when some alternative is not
    /// exactly one character.
    fn class_strings(&mut self, at: usize) -> Result<bool, PatternError> {
        if !self.eat('{') {
            return Err(error(at, "`\\q` must be followed by `{…}`"));
        }
        let mut strings = false;
        let mut length = 0;
        loop {
            match self.peek() {
                None | Some(']') => {
                    return Err(error(at, "unterminated `\\q{`: expected `}`"));
                }
                Some('}') => {
                    self.pos += 1;
                    return Ok(strings || length != 1);
                }
                Some('|') => {
                    self.pos += 1;
                    strings |= length != 1;
                    length = 0;
                }
                Some(ch) => {
                    let char_at = self.pos;
                    self.pos += ch.len_utf8();
                    self.class_set_character(char_at, ch)?;
                    length += 1;
                }
            }
        }
    }

    /// `ClassSetCharacter` at `at`, whose first character `ch` is already consumed: a
    /// bare character, or a backslash with a character escape, a reserved punctuator or
    /// `b` (backspace). Its code point.
    fn class_set_character(&mut self, at: usize, ch: char) -> Result<u32, PatternError> {
        if ch == '\\' {
            return match self.peek() {
                Some('b') => {
                    self.bump();
                    Ok(0x8)
                }
                Some(punctuator) if is_reserved_punctuator(punctuator) => {
                    self.bump();
                    Ok(u32::from(punctuator))
                }
                _ => self.character_escape(at),
            };
        }
        if is_class_syntax_character(ch) {
            return Err(error(
                at,
                format!("`{ch}` must be escaped as `\\{ch}` inside a class"),
            ));
        }
        if is_reserved_double(ch) && self.peek() == Some(ch) {
            return Err(error(
                at,
                format!("`{ch}{ch}` is reserved inside a class: escape it as `\\{ch}\\{ch}`"),
            ));
        }
        Ok(u32::from(ch))
    }

    /// Every `\N` must name an existing capturing group and every `\k<name>` a declared
    /// name; forward references are fine.
    fn check_references(&self) -> Result<(), PatternError> {
        for (at, reference) in &self.references {
            match reference {
                Reference::Number(number) if *number > u64::from(self.capture_count) => {
                    return Err(error(
                        *at,
                        format!(
                            "back-reference `\\{number}` to a group that does not exist: the pattern has {} capturing groups",
                            self.capture_count
                        ),
                    ));
                }
                Reference::Name(name) if !self.groups.contains(name) => {
                    return Err(error(
                        *at,
                        format!("`\\k<{name}>` names no group in the pattern"),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Pattern, PatternError};

    fn message(body: &str) -> String {
        parse(body).map_or_else(|e| e.message, |p| format!("parsed: {p:?}"))
    }

    /// Valid under `v`: every construct of the ES2025 pattern grammar, with the
    /// test262 syntax cases this parser must agree with.
    const VALID: &[&str] = &[
        // atoms, alternation, assertions
        "a",
        "abc",
        "",
        "a|b",
        "|",
        "a||b",
        "a|",
        "^a$",
        "^$",
        "\\bfoo\\B",
        ".",
        "é",
        "😀",
        "(a|b)c",
        // quantifiers
        "a*",
        "a+",
        "a?",
        "a{2}",
        "a{2,}",
        "a{2,3}",
        "a*?",
        "a+?",
        "a??",
        "a{2}?",
        "a{2,3}?",
        "a{0,0}",
        "a{10,20}",
        "a{2}b{3,}",
        "(?:a)*",
        "(a)+",
        "(?<n>a)?",
        ".*",
        "[a]+",
        "a{99999999999999999999,99999999999999999999}",
        // groups
        "(a)",
        "()",
        "(?:)",
        "(?:a)",
        "(?<n>a)",
        "(?<n>)",
        "(?<n>a)(?<m>b)",
        "((a)(b))",
        "(?<a>x)|(?<a>y)",
        "(?<a>x)|(?<a>y)|(?<a>z)",
        "(?:(?<a>x)|(?<a>y))|(?<a>z)",
        "(?:(?<a>x)|(?<a>y))\\k<a>",
        "(?<𝑎>x)",
        "(?<\\u0061>x)\\k<a>",
        "(?<\\u{61}>x)",
        "(?<\\uD835\\uDC4E>x)",
        "(?<a\\u200Cb>x)",
        "(?<$_1>x)",
        "(?<_>x)",
        // back-references, forward too
        "(a)\\1",
        "\\1(a)",
        "(?<n>a)\\k<n>",
        "\\k<n>(?<n>a)",
        "\\k<n>|(?<n>x)",
        "\\9(a)(b)(c)(d)(e)(f)(g)(h)(i)",
        "\\10(a)(b)(c)(d)(e)(f)(g)(h)(i)(j)",
        // lookaround
        "(?=a)",
        "(?!a)",
        "(?<=a)",
        "(?<!a)",
        "(?=a)b",
        "(?<=\\d)x",
        "a(?=b|c)",
        // modifiers
        "(?i:a)",
        "(?m:a)",
        "(?s:a)",
        "(?ims:a)",
        "(?-i:a)",
        "(?i-m:a)",
        "(?i-ms:a)",
        "(?i-:a)",
        "(?-ims:a)",
        "(?i:a)+",
        // escapes
        "\\.",
        "\\*",
        "\\+",
        "\\?",
        "\\(",
        "\\)",
        "\\[",
        "\\]",
        "\\{",
        "\\}",
        "\\|",
        "\\^",
        "\\$",
        "\\\\",
        "\\/",
        "\\d",
        "\\D",
        "\\s",
        "\\S",
        "\\w",
        "\\W",
        "\\n",
        "\\r",
        "\\t",
        "\\f",
        "\\v",
        "\\0",
        "\\0a",
        "\\cA",
        "\\cz",
        "\\cJ",
        "\\x41",
        "\\xff",
        "\\u0041",
        "\\u{41}",
        "\\u{10FFFF}",
        "\\u{0}",
        "\\uD83D\\uDE00",
        "\\uD83D",
        "\\uDE00",
        "\\uD83D\\u{DE00}",
        "\\uD83D\\u0041",
        // properties
        "\\p{L}",
        "\\p{Lu}",
        "\\p{Letter}",
        "\\P{L}",
        "\\p{General_Category=Lu}",
        "\\p{gc=L}",
        "\\p{Script=Latin}",
        "\\p{sc=Latn}",
        "\\p{Script_Extensions=Greek}",
        "\\p{scx=Grek}",
        "\\P{Script=Latin}",
        "\\p{ASCII}",
        "\\p{Alphabetic}",
        "\\p{Alpha}",
        "\\p{Any}",
        "\\p{Assigned}",
        "\\p{Emoji}",
        "\\p{White_Space}",
        "\\p{space}",
        "\\p{XID_Start}",
        "\\p{Lowercase_Letter}",
        "\\p{Decimal_Number}",
        "\\p{Nd}",
        "\\p{digit}",
        "\\p{punct}",
        "\\p{Script=Han}",
        "\\p{scx=Hani}",
        "\\p{RGI_Emoji}",
        "\\p{Basic_Emoji}",
        "\\p{Emoji_Keycap_Sequence}",
        // classes
        "[a]",
        "[abc]",
        "[a-z]",
        "[a-zA-Z0-9_]",
        "[^a]",
        "[]",
        "[^]",
        "[\\d]",
        "[\\w\\s]",
        "[\\p{L}]",
        "[\\P{L}]",
        "[^\\p{L}]",
        "[\\b]",
        "[\\-]",
        "[\\]]",
        "[\\[]",
        "[a\\-z]",
        "[\\&\\!\\#\\%\\,\\:\\;\\<\\=\\>\\@\\`\\~]",
        "[.*+?^$]",
        "[\\{\\}\\(\\)\\|\\/\\\\\\-]",
        "[\\x41-\\x5A]",
        "[\\u{61}-\\u{7A}]",
        "[\\0-\\x1F]",
        "[\\cA-\\cZ]",
        "[😀-😂]",
        "[a-a]",
        "[\\n\\r\\t\\f\\v]",
        "[\\0a]",
        "[a&b]",
        "[a-z&]",
        // v-mode set operations and strings
        "[[a-z]--[aeiou]]",
        "[\\p{L}&&\\p{Lu}]",
        "[[a-z]&&[^aeiou]]",
        "[a--b]",
        "[a&&b&&c]",
        "[[abc]--[b]--[c]]",
        "[[a]]",
        "[[^a]]",
        "[^[a]]",
        "[[a-z][0-9]]",
        "[a[b-c]d]",
        "[\\q{abc|def}]",
        "[\\q{a}]",
        "[\\q{}]",
        "[\\q{abc}&&\\q{abc|d}]",
        "[\\q{a|}]",
        "[\\q{😀}]",
        "[\\q{\\n\\-\\&}]",
        "[\\p{RGI_Emoji}]",
        "[\\p{RGI_Emoji}--\\q{😀}]",
        "[^\\q{ab}&&\\q{a}]",
        "[^\\q{a}]",
        "[^[\\q{ab}]&&[a]]",
        "[[a-z]--\\d]",
        "[\\d--[0-4]]",
        "[[\\q{ab}]--[\\q{ab}]]",
        "[^[a]--[\\q{bc}]]",
        "[aa]",
        "[\\q{aa|bb}]",
        "[\\cA-\\x01]",
        "[\\x1A-\\cZ]",
        "[\\x1A-\\cz]",
        "[^\\q{a|b}]",
        "[\\b-\\x08]",
        "[^\\q{\\&}]",
        "[^\\q{\\b}]",
    ];

    /// Invalid under `v`, with the offset each error is reported at.
    const INVALID: &[(&str, usize)] = &[
        // groups and parentheses
        ("(", 0),
        (")", 0),
        ("a)", 1),
        ("(a", 0),
        ("(?", 0),
        ("(?a)", 0),
        ("(?<", 3),
        ("(?<=", 0),
        ("(?P<a>x)", 0),
        ("(?i)", 0),
        ("(?ii:a)", 3),
        ("(?i-i:a)", 4),
        ("(?-:a)", 0),
        ("(?=a", 0),
        ("(?=*)", 3),
        ("(?<=a", 0),
        ("(?<=*)", 4),
        ("(?!(", 3),
        ("(?<!(", 4),
        ("{2,3}", 0),
        ("{2,}", 0),
        ("(?<\\u12>x)", 3),
        ("(?<\\u{110000}>x)", 3),
        ("[a-(]", 3),
        ("[a-\\e]", 3),
        ("[\\p{Foo}]", 1),
        ("[\\q{a|(}]", 6),
        ("[\\cZ-\\cA]", 1),
        ("[\\x02-\\cA]", 1),
        ("[^\\q{ab|c}]", 0),
        ("[^\\q{c|ab}]", 0),
        ("(?x:a)", 0),
        ("(?i-m-s:a)", 0),
        // group names
        ("(?<>a)", 3),
        ("(?<1>a)", 3),
        ("(?<1a>a)", 3),
        ("(?<a", 3),
        ("(?<a>", 0),
        ("(?<a-b>x)", 4),
        ("(?<a b>x)", 4),
        ("(?<\\u200Ca>x)", 3),
        ("(?<\\x61>x)", 3),
        ("(?<\\uD83D>x)", 3),
        ("(?<a>x)(?<a>y)", 10),
        ("(?<a>(?<a>x))", 8),
        ("(?:(?<a>x)|(?<a>y))(?<a>z)", 22),
        ("(?<a>x)|(?<a>y)(?<a>z)", 18),
        ("((?<a>x)|(?<a>y))(?<a>z)", 20),
        // quantifiers
        ("*", 0),
        ("+", 0),
        ("?", 0),
        ("a**", 2),
        ("a++", 2),
        ("a?*", 2),
        ("a{1}{2}", 4),
        ("a{2,1}", 1),
        ("{", 0),
        ("}", 0),
        ("a{", 1),
        ("a{1", 1),
        ("a{1,", 1),
        ("a{,1}", 1),
        ("a{1x}", 1),
        ("{2}", 0),
        ("a{2,3}{", 6),
        ("]", 0),
        ("a]", 1),
        ("^*", 1),
        ("$+", 1),
        ("\\b*", 2),
        ("\\B?", 2),
        ("(?=a)*", 5),
        ("(?!a)+", 5),
        ("(?<=a)?", 6),
        ("(?<!a){2}", 6),
        ("^{2}", 1),
        // escapes
        ("\\", 0),
        ("a\\", 1),
        ("\\a", 0),
        ("\\e", 0),
        ("\\z", 0),
        ("\\-", 0),
        ("\\ ", 0),
        ("\\c", 0),
        ("\\c1", 0),
        ("\\c-", 0),
        ("\\x", 0),
        ("\\x4", 0),
        ("\\x4G", 0),
        ("\\x{41}", 0),
        ("\\u", 0),
        ("\\u12", 0),
        ("\\u{}", 0),
        ("\\u{", 0),
        ("\\u{41", 0),
        ("\\u{110000}", 0),
        ("\\u{FFFFFFFFF}", 0),
        ("\\01", 0),
        ("\\00", 0),
        ("\\8", 0),
        // back-references
        ("\\1", 0),
        ("\\2(a)", 0),
        ("(a)\\2", 3),
        ("\\9(a)(b)(c)(d)(e)(f)(g)(h)", 0),
        ("\\k", 0),
        ("\\k<", 3),
        ("\\k<>", 3),
        ("\\k<a", 3),
        ("\\k<a>", 0),
        ("\\k<1>", 3),
        ("(?<a>x)\\k<b>", 7),
        ("\\k<a>|(?<b>x)", 0),
        // properties
        ("\\p", 0),
        ("\\p{", 0),
        ("\\p{}", 0),
        ("\\p{Foo}", 0),
        ("\\p{L", 0),
        ("\\p{Script}", 0),
        ("\\p{Script=}", 0),
        ("\\p{Script=Foo}", 0),
        ("\\p{Foo=Bar}", 0),
        ("\\p{General_Category=Latin}", 0),
        ("\\p{sc=Lu}", 0),
        ("\\p{Latin}", 0),
        ("\\p{ascii}", 0),
        ("\\p{lu}", 0),
        ("\\P{RGI_Emoji}", 0),
        ("\\p{RGI_Emoji=Yes}", 0),
        ("\\p{Lu=Yes}", 0),
        ("a\\P{Basic_Emoji}", 1),
        // classes
        ("[", 0),
        ("[a", 0),
        ("[a-]", 2),
        ("[-a]", 1),
        ("[a-\\d]", 3),
        ("[\\d-z]", 3),
        ("[z-a]", 1),
        ("[\\x5A-\\x41]", 1),
        ("[a-z--[aeiou]]", 1),
        ("[ab&&c]", 3),
        ("[a&&bc]", 5),
        ("[a&&b--c]", 5),
        ("[a--b&&c]", 5),
        ("[a&&&b]", 4),
        ("[a---b]", 4),
        ("[&&a]", 1),
        ("[--a]", 1),
        ("[a&&]", 4),
        ("[a--]", 4),
        ("[a&&", 4),
        ("[a--", 4),
        ("[a&&b", 0),
        ("[(]", 1),
        ("[)]", 1),
        ("[{]", 1),
        ("[}]", 1),
        ("[/]", 1),
        ("[|]", 1),
        ("[\\]", 0),
        ("[[]", 0),
        ("[a!!b]", 2),
        ("[a##b]", 2),
        ("[a$$b]", 2),
        ("[a%%b]", 2),
        ("[a**b]", 2),
        ("[a++b]", 2),
        ("[a,,b]", 2),
        ("[a..b]", 2),
        ("[a::b]", 2),
        ("[a;;b]", 2),
        ("[a<<b]", 2),
        ("[a==b]", 2),
        ("[a>>b]", 2),
        ("[a??b]", 2),
        ("[a@@b]", 2),
        ("[a^^b]", 2),
        ("[a``b]", 2),
        ("[a~~b]", 2),
        ("[\\x]", 1),
        ("[\\u{110000}]", 1),
        ("[\\c]", 1),
        ("[\\01]", 1),
        ("[\\1]", 1),
        ("[\\k<a>]", 1),
        ("[\\B]", 1),
        ("[\\a]", 1),
        ("[\\q]", 1),
        ("[\\q{]", 1),
        ("[\\q{a]", 1),
        ("[\\q{(}]", 4),
        ("[\\q{a!!}]", 5),
        ("[\\q{\\d}]", 4),
        ("[a-\\q{b}]", 3),
        // strings in negated classes
        ("[^\\q{ab}]", 0),
        ("[^\\q{}]", 0),
        ("[^\\q{a|}]", 0),
        ("[^[\\q{ab}]]", 0),
        ("[^\\q{ab}--\\q{a}]", 0),
        ("[^\\q{ab}&&\\q{cd}]", 0),
        ("[^\\p{RGI_Emoji}]", 0),
        ("[^a\\q{bc}]", 0),
        ("[^\\q{bc}a]", 0),
        ("[^[\\q{bc}]--[a]]", 0),
    ];

    #[test]
    fn every_valid_pattern_parses() {
        let count = VALID.len();
        assert!(count >= 150, "{count} valid patterns");
        for body in VALID {
            let result = parse(body);
            assert!(result.is_ok(), "{body:?}: {result:?}");
        }
    }

    #[test]
    fn every_invalid_pattern_is_rejected_at_its_offset() {
        let count = INVALID.len();
        assert!(count >= 150, "{count} invalid patterns");
        for (body, expected) in INVALID {
            let got = parse(body).err().map_or(usize::MAX, |e| e.offset);
            let explanation = message(body);
            assert_eq!(got, *expected, "{body:?}: {explanation}");
        }
    }

    #[test]
    fn named_groups_come_back_in_source_order_with_the_capture_count() {
        assert_eq!(parse(""), Ok(Pattern::default()));
        assert_eq!(
            parse("(?<y>a)(b)(?<z>c)|(?<y>d)(?:e)(?=f)"),
            Ok(Pattern {
                groups: vec!["y".to_owned(), "z".to_owned(), "y".to_owned()],
                capture_count: 4,
            })
        );
        assert_eq!(
            parse("(?<\\u0061\\u{62}>x)").map(|p| p.groups),
            Ok(vec!["ab".to_owned()]),
            "escapes in a name decode"
        );
        assert_eq!(parse("((((a))))").map(|p| p.capture_count), Ok(4));
        assert_eq!(
            parse("(?<\\uD835\\uDC4E>x)").map(|p| p.groups),
            Ok(vec!["\u{1D44E}".to_owned()]),
            "a surrogate pair in a name is one character"
        );
        assert_eq!(
            parse("(?<\\u{1D44E}>x)\\k<\\uD835\\uDC4E>").map(|p| p.groups),
            Ok(vec!["\u{1D44E}".to_owned()]),
            "both spellings name the same group"
        );
    }

    #[test]
    fn messages_name_the_problem() {
        let cases = [
            ("\\k<x>", "`\\k<x>` names no group in the pattern"),
            ("(a)\\2", "back-reference `\\2` to a group that does not exist: the pattern has 1 capturing groups"),
            ("a{2,1}", "quantifier `{2,1}` is out of order"),
            ("{", "`{` must be escaped as `\\{` unless it is a quantifier `{n}`, `{n,}` or `{n,m}`"),
            ("{2}", "nothing to repeat before `{`"),
            ("a**", "nothing to repeat before `*`"),
            ("^*", "an assertion cannot be quantified"),
            ("}", "`}` must be escaped as `\\}`"),
            ("[a-]", "`-` must be escaped as `\\-` unless it forms a range"),
            ("[z-a]", "range `z-a` is out of order"),
            ("[\\d-z]", "a range needs a single character on each side of `-`"),
            ("[a-\\d]", "a range needs a single character on each side of `-`"),
            ("[ab&&c]", "a set operator cannot follow a union: bracket the operands, as in `[[ab]&&[bc]]`"),
            ("[a-z--[aeiou]]", "a range cannot be an operand of `--`: bracket it, as in `[[a-z]--[aeiou]]`"),
            ("[a&&b--c]", "expected `&&` or `]`: operators cannot be mixed without brackets"),
            ("[a&&&b]", "`&&&` is not permitted"),
            ("[a!!b]", "`!!` is reserved inside a class: escape it as `\\!\\!`"),
            ("[(]", "`(` must be escaped as `\\(` inside a class"),
            ("[a&&]", "expected a class operand before `]`"),
            ("[a&&", "expected a class operand before the end of the pattern"),
            ("[a", "unterminated character class: expected `]`"),
            ("[^\\q{ab}]", "a negated class `[^…]` cannot contain strings"),
            ("[\\q]", "`\\q` must be followed by `{…}`"),
            ("[\\q{a]", "unterminated `\\q{`: expected `}`"),
            ("\\p{Foo}", "unknown Unicode property `Foo`"),
            ("\\p{Script=Foo}", "unknown Unicode property `Script=Foo`"),
            ("\\P{RGI_Emoji}", "`\\P{RGI_Emoji}` cannot negate a property of strings"),
            ("\\p", "`\\p` must be followed by `{…}`"),
            ("\\p{L", "unterminated `\\p{`: expected `}`"),
            ("\\c", "`\\c` must be followed by a letter"),
            ("\\01", "`\\0` cannot be followed by a digit: octal escapes are not permitted"),
            ("\\x4", "`\\x` must be followed by two hexadecimal digits"),
            ("\\u{}", "`\\u{` must hold hexadecimal digits and close with `}`"),
            ("\\u{110000}", "`\\u{…}` must be at most 10FFFF"),
            ("\\u12", "`\\u` must be followed by four hexadecimal digits or `{…}`"),
            ("\\e", "`\\e` is not an escape: only syntax characters and `/` may be escaped"),
            ("\\", "`\\` at the end of the pattern"),
            ("\\k", "`\\k` must be followed by `<name>`"),
            ("(?<>a)", "empty group name"),
            ("(?<a", "unterminated group name: expected `>`"),
            ("(?<1>a)", "`1` is not permitted in a group name"),
            ("(?<\\x61>x)", "only `\\u` escapes are permitted in a group name"),
            ("(?<\\uD83D>x)", "a lone surrogate is not permitted in a group name"),
            ("(?<a>x)(?<a>y)", "duplicate group name `a`: a name may recur only in different alternatives"),
            ("(?ii:a)", "modifier `i` is repeated"),
            ("(?-:a)", "`(?-:` sets no modifier"),
            ("(?x:a)", "`(?` must continue with `:`, `<name>`, `=`, `!`, `<=`, `<!` or modifiers `i`, `m`, `s`"),
            ("(a", "unterminated group: expected `)`"),
            (")", "unmatched `)`"),
        ];
        for (body, expected) in cases {
            let got = message(body);
            assert_eq!(got, expected, "{body:?}");
        }
        assert_eq!(
            message("(a)"),
            "parsed: Pattern { groups: [], capture_count: 1 }"
        );
        assert_eq!(
            parse("\\k<x>"),
            Err(PatternError {
                offset: 0,
                message: "`\\k<x>` names no group in the pattern".to_owned()
            })
        );
    }
}
