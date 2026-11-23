//! JSON values, written and read in-house (STORIES.md US-100): the documents the
//! compiler writes are read back by the compiler's own reader, with no other crate.
//! A document is written one way only: two spaces a level, a member a line, the keys
//! in the order they are given.

use std::fmt::Write as _;

/// A JSON value. A number is a whole one: the compiler's documents hold no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A whole number.
    Int(i64),
    /// A string.
    Text(String),
    /// An array.
    List(Vec<Json>),
    /// An object: its members in order.
    Map(Vec<(String, Json)>),
}

impl Json {
    /// A string value.
    #[must_use]
    pub fn text(text: &str) -> Self {
        Self::Text(text.to_owned())
    }

    /// An object of these members.
    #[must_use]
    pub fn map(members: Vec<(&str, Self)>) -> Self {
        let members = members.into_iter();
        Self::Map(
            members
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    }

    /// The member of an object at `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Map(members) => {
                let member = members.iter().find(|(name, _)| name == key);
                member.map(|(_, value)| value)
            }
            _ => None,
        }
    }

    /// The text of a string value.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            _ => None,
        }
    }

    /// The flag of a boolean value.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    /// The elements of an array.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Self]> {
        match self {
            Self::List(elements) => Some(elements),
            _ => None,
        }
    }

    /// The value as a document: indented by two spaces, ended by a newline.
    #[must_use]
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_at(0, &mut out);
        out.push('\n');
        out
    }

    fn write_at(&self, depth: usize, out: &mut String) {
        let indent = "  ".repeat(depth + 1);
        let close = "  ".repeat(depth);
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Self::Int(number) => out.push_str(&number.to_string()),
            Self::Text(text) => quote(text, out),
            Self::List(elements) if elements.is_empty() => out.push_str("[]"),
            Self::Map(members) if members.is_empty() => out.push_str("{}"),
            Self::List(elements) => {
                out.push('[');
                for (at, element) in elements.iter().enumerate() {
                    out.push_str(if at == 0 { "\n" } else { ",\n" });
                    out.push_str(&indent);
                    element.write_at(depth + 1, out);
                }
                out.push('\n');
                out.push_str(&close);
                out.push(']');
            }
            Self::Map(members) => {
                out.push('{');
                for (at, (key, value)) in members.iter().enumerate() {
                    out.push_str(if at == 0 { "\n" } else { ",\n" });
                    out.push_str(&indent);
                    quote(key, out);
                    out.push_str(": ");
                    value.write_at(depth + 1, out);
                }
                out.push('\n');
                out.push_str(&close);
                out.push('}');
            }
        }
    }

    /// Reads a document: one value, with nothing but whitespace around it.
    ///
    /// # Errors
    ///
    /// What is wrong, and at which byte.
    pub fn read(text: &str) -> Result<Self, String> {
        let mut reader = Reader {
            bytes: text.as_bytes(),
            text,
            at: 0,
        };
        let value = reader.value()?;
        reader.blank();
        if reader.at < reader.bytes.len() {
            return Err(reader.fail("more after the value"));
        }
        Ok(value)
    }
}

/// Writes a string as JSON writes one.
fn quote(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                // Writing to a string does not fail.
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct Reader<'a> {
    bytes: &'a [u8],
    text: &'a str,
    at: usize,
}

impl Reader<'_> {
    fn fail(&self, why: &str) -> String {
        format!("byte {}: {why}", self.at)
    }

    /// Passes whitespace.
    fn blank(&mut self) {
        let rest = &self.bytes[self.at..];
        let blank = rest.iter().take_while(|byte| b" \t\r\n".contains(byte));
        self.at += blank.count();
    }

    /// Passes `byte`, when it is next.
    fn eat(&mut self, byte: u8) -> bool {
        let found = self.bytes.get(self.at) == Some(&byte);
        self.at += usize::from(found);
        found
    }

    fn value(&mut self) -> Result<Json, String> {
        self.blank();
        let rest = &self.text[self.at..];
        for (word, value) in [
            ("null", Json::Null),
            ("true", Json::Bool(true)),
            ("false", Json::Bool(false)),
        ] {
            if rest.starts_with(word) {
                self.at += word.len();
                return Ok(value);
            }
        }
        match self.bytes.get(self.at) {
            Some(b'"') => self.string().map(Json::Text),
            Some(b'[') => {
                self.at += 1;
                let mut elements = Vec::new();
                self.elements(&mut elements)?;
                Ok(Json::List(elements))
            }
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                self.members(&mut members)?;
                Ok(Json::Map(members))
            }
            _ => {
                let negative = usize::from(rest.starts_with('-'));
                let digits = rest[negative..].bytes().take_while(u8::is_ascii_digit);
                let length = negative + digits.count();
                let number = rest[..length].parse::<i64>().ok();
                self.at += length;
                number.map(Json::Int).ok_or_else(|| self.fail("no value"))
            }
        }
    }

    /// The elements of an array, after its `[`, into `elements`.
    fn elements(&mut self, elements: &mut Vec<Json>) -> Result<(), String> {
        self.blank();
        if self.eat(b']') {
            return Ok(());
        }
        elements.push(self.value()?);
        self.blank();
        if self.eat(b',') {
            self.blank();
            if self.bytes.get(self.at) == Some(&b']') {
                return Err(self.fail("a comma before the `]`"));
            }
            return self.elements(elements);
        }
        if self.eat(b']') {
            return Ok(());
        }
        Err(self.fail("neither `,` nor `]`"))
    }

    /// The members of an object, after its `{`, into `members`.
    fn members(&mut self, members: &mut Vec<(String, Json)>) -> Result<(), String> {
        self.blank();
        if self.eat(b'}') {
            return Ok(());
        }
        let key = self.string()?;
        self.blank();
        if !self.eat(b':') {
            return Err(self.fail("no `:` after a key"));
        }
        members.push((key, self.value()?));
        self.blank();
        if self.eat(b',') {
            self.blank();
            if self.bytes.get(self.at) == Some(&b'}') {
                return Err(self.fail("a comma before the `}`"));
            }
            return self.members(members);
        }
        if self.eat(b'}') {
            return Ok(());
        }
        Err(self.fail("neither `,` nor `}`"))
    }

    /// A string, from its opening quotation mark.
    fn string(&mut self) -> Result<String, String> {
        if !self.eat(b'"') {
            return Err(self.fail("no string"));
        }
        let mut read = String::new();
        let mut rest = self.text[self.at..].chars();
        while let Some(c) = rest.next() {
            self.at += c.len_utf8();
            match c {
                '"' => return Ok(read),
                '\\' => {
                    let escape = rest.next();
                    self.at += escape.map_or(0, char::len_utf8);
                    read.push(match escape {
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some('/') => '/',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('b') => '\u{8}',
                        Some('f') => '\u{c}',
                        Some('u') => {
                            // Four hexadecimal digits, taken as the text goes on.
                            let digits: String = rest.by_ref().take(4).collect();
                            let code = u32::from_str_radix(&digits, 16).ok();
                            let decoded = code.filter(|_| digits.len() == 4);
                            let decoded = decoded.and_then(char::from_u32);
                            self.at += digits.len();
                            decoded.ok_or_else(|| self.fail("no such `\\u` escape"))?
                        }
                        _ => return Err(self.fail("no such escape")),
                    });
                }
                c if u32::from(c) < 0x20 => return Err(self.fail("a control character")),
                c => read.push(c),
            }
        }
        Err(self.fail("a string that does not end"))
    }
}
