//! Tokens and trivia: the lossless representation of a file (SPEC.md §11.1).
//!
//! Trivia (whitespace, line terminators, comments) is attached to the token that follows
//! it; the trailing run of a file is attached to the `Eof` token. Printing every token's
//! trivia then text, in order, reproduces the source exactly.

use sjs_diag::{LineTerminator, SourceFile, Span};

/// A run of source text that is not a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriviaKind {
    /// Spaces, tabs and the other ES2025 `WhiteSpace` characters.
    Whitespace,
    /// One line terminator (`\r\n` counts as one).
    LineTerminator(LineTerminator),
    /// `// …` up to, not including, the line terminator.
    LineComment,
    /// `/* … */`, which may span lines; `/**/` is one of these.
    BlockComment {
        /// True when the comment contains a line terminator, which makes it count as a
        /// newline for ASI purposes (ES2025 §12.4).
        has_line_terminator: bool,
    },
    /// `/** … */`: a block comment starting with `/**` and longer than `/**/`, so
    /// `/***/` is a doc comment whose body is `*` (SPEC.md §2, §13.7). It documents
    /// the declaration on the next line; see [`Token::doc_comment`].
    DocComment {
        /// As for `BlockComment`.
        has_line_terminator: bool,
    },
}

/// One trivia run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trivia {
    /// What it is.
    pub kind: TriviaKind,
    /// Where it is.
    pub span: Span,
}

impl Trivia {
    /// True when this trivia ends a line as far as ASI is concerned.
    #[must_use]
    pub fn is_newline(&self) -> bool {
        matches!(
            self.kind,
            TriviaKind::LineTerminator(_)
                | TriviaKind::BlockComment {
                    has_line_terminator: true
                }
                | TriviaKind::DocComment {
                    has_line_terminator: true
                }
        )
    }
}

/// A reserved word of the subset, or a removed word kept as a token so the parser can
/// name it (a removed word after `.` is an ordinary property name, SPEC.md §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum Keyword {
    Break,
    Case,
    Catch,
    Class,
    Const,
    Continue,
    Default,
    Else,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    Instanceof,
    Let,
    New,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    While,
    // Reserved in strict mode and therefore never identifiers; the parser rejects them.
    Do,
    Enum,
    Implements,
    Interface,
    Package,
    Private,
    Protected,
    Public,
    // The eight removed words of SPEC.md §2.
    Var,
    With,
    Void,
    Delete,
    In,
    Arguments,
    Eval,
    Debugger,
}

impl Keyword {
    /// Every keyword with its spelling.
    pub const ALL: &'static [(Keyword, &'static str)] = &[
        (Keyword::Break, "break"),
        (Keyword::Case, "case"),
        (Keyword::Catch, "catch"),
        (Keyword::Class, "class"),
        (Keyword::Const, "const"),
        (Keyword::Continue, "continue"),
        (Keyword::Default, "default"),
        (Keyword::Else, "else"),
        (Keyword::Export, "export"),
        (Keyword::Extends, "extends"),
        (Keyword::Finally, "finally"),
        (Keyword::For, "for"),
        (Keyword::Function, "function"),
        (Keyword::If, "if"),
        (Keyword::Import, "import"),
        (Keyword::Instanceof, "instanceof"),
        (Keyword::Let, "let"),
        (Keyword::New, "new"),
        (Keyword::Return, "return"),
        (Keyword::Super, "super"),
        (Keyword::Switch, "switch"),
        (Keyword::This, "this"),
        (Keyword::Throw, "throw"),
        (Keyword::Try, "try"),
        (Keyword::Typeof, "typeof"),
        (Keyword::While, "while"),
        (Keyword::Do, "do"),
        (Keyword::Enum, "enum"),
        (Keyword::Implements, "implements"),
        (Keyword::Interface, "interface"),
        (Keyword::Package, "package"),
        (Keyword::Private, "private"),
        (Keyword::Protected, "protected"),
        (Keyword::Public, "public"),
        (Keyword::Var, "var"),
        (Keyword::With, "with"),
        (Keyword::Void, "void"),
        (Keyword::Delete, "delete"),
        (Keyword::In, "in"),
        (Keyword::Arguments, "arguments"),
        (Keyword::Eval, "eval"),
        (Keyword::Debugger, "debugger"),
    ];

    /// The keyword spelled `word`, if any (case-sensitive).
    #[must_use]
    pub fn from_word(word: &str) -> Option<Keyword> {
        Keyword::ALL
            .iter()
            .find(|(_, spelling)| *spelling == word)
            .map(|(keyword, _)| *keyword)
    }

    /// The spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        Keyword::ALL
            .iter()
            .find(|(keyword, _)| *keyword == self)
            .map_or("", |(_, spelling)| spelling)
    }

    /// For a removed word (SPEC.md §2), the §12 replacement to name in the E000 message.
    #[must_use]
    pub fn removed_replacement(self) -> Option<&'static str> {
        match self {
            Keyword::Var => Some("`const` or `let`"),
            Keyword::With => Some("nothing: there is no dynamic scope"),
            Keyword::Void => Some("`undefined`"),
            Keyword::Delete => Some("assigning `undefined`, or `Map.delete`"),
            Keyword::In => Some("a discriminant property or `Object.hasOwn`"),
            Keyword::Arguments => Some("rest parameters"),
            Keyword::Eval => Some("nothing: there is no code from strings"),
            Keyword::Debugger => Some("nothing: it is a host-side signal"),
            _ => None,
        }
    }
}

/// The value of a `number` literal (SPEC.md §2).
#[derive(Debug, Clone, Copy)]
pub struct Number {
    /// The double nearest the literal's mathematical value; infinite when it overflows.
    pub value: f64,
    /// True for an integer literal beyond ±`Number.MAX_SAFE_INTEGER` (2⁵³ − 1), where
    /// doubles no longer represent every integer: the type system must not treat the
    /// value as exact.
    pub exceeds_safe_integer: bool,
}

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.value.to_bits() == other.value.to_bits()
            && self.exceeds_safe_integer == other.exceeds_safe_integer
    }
}

impl Eq for Number {}

/// Which piece of a template literal a token is (SPEC.md §2). `` `a${b}c${d}e` `` lexes
/// as `Head("a")`, `b`, `Middle("c")`, `d`, `Tail("e")`; `` `a` `` is `Whole("a")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplatePart {
    /// `` `…` ``: no substitution.
    Whole,
    /// `` `…${ ``.
    Head,
    /// `}…${`.
    Middle,
    /// `` }…` ``.
    Tail,
}

/// A template literal token's values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Which piece.
    pub part: TemplatePart,
    /// Escapes decoded, continuations removed, CR and CRLF as LF.
    pub cooked: Box<str>,
    /// As written (`String.raw`), with CR and CRLF as LF.
    pub raw: Box<str>,
}

/// A punctuator. US-014 needs the braces to balance template substitutions and US-015
/// the division operators that a `/` is when it does not start a regex; US-018 adds
/// the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Punct {
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `/`
    Slash,
    /// `/=`
    SlashEq,
}

impl Punct {
    /// The spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Punct::LBrace => "{",
            Punct::RBrace => "}",
            Punct::Slash => "/",
            Punct::SlashEq => "/=",
        }
    }
}

/// A regex literal's parts (SPEC.md §2). The body is the text between the slashes as
/// written, already checked against the `v`-mode grammar (US-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regex {
    /// Between the slashes, as written.
    pub body: Box<str>,
    /// After the closing slash, as written: each of `d g i m s v y` at most once. A
    /// well-formed set lacking `v` is reported as E002 but still lexes.
    pub flags: Box<str>,
    /// The named groups in source order (US-016); a name recurs when two alternatives
    /// declare it.
    pub groups: Box<[Box<str>]>,
}

impl Regex {
    /// True when `flag` is among the flags.
    #[must_use]
    pub fn has_flag(&self, flag: char) -> bool {
        self.flags.contains(flag)
    }
}

/// What a token is. US-011 adds words, US-012 numbers, US-013 strings, US-014 templates
/// and the braces, US-015 regexes and the division operators; US-016 to US-018 add the
/// other punctuators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// `[A-Za-z_][A-Za-z0-9_]*` that is not a keyword; contextual words (`of`, `get`,
    /// `set`, `static`, `async`, `await`, `yield`, `from`, `as`) are identifiers here.
    Ident,
    /// A reserved or removed word.
    Keyword(Keyword),
    /// `null`, a literal whose placement the parser checks (SPEC.md §3.1).
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A `number` literal: decimal, `0x`, `0o` or `0b`, with its value.
    Number(Number),
    /// A `bigint` literal (`n` suffix): its decimal digits, no sign, no leading zeros.
    BigInt(Box<str>),
    /// A string literal, cooked: escapes decoded, line continuations removed. The raw
    /// text is the span. A `'…'` literal is this kind too, reported as S001.
    String(Box<str>),
    /// A piece of a template literal with its cooked and raw values; the span covers the
    /// piece including its `` ` ``, `${` and `}` delimiters.
    Template(Template),
    /// A regex literal with its body and flags.
    Regex(Regex),
    /// A punctuator.
    Punct(Punct),
    /// A character the lexer does not accept; reported as E000, lexing continues.
    Error,
    /// The end of the file; carries the trailing trivia.
    Eof,
}

/// A token with the trivia that precedes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What it is.
    pub kind: TokenKind,
    /// Its own text, trivia excluded (empty for `Eof`).
    pub span: Span,
    /// The trivia between the previous token and this one.
    pub leading: Vec<Trivia>,
    /// True when a line terminator, or a block comment containing one, lies between the
    /// previous token and this one (SPEC.md §2 E001 depends on it).
    pub has_newline_before: bool,
}

impl Token {
    /// The doc comment attached to this token (SPEC.md §2, §8.4): a `/** … */` whose
    /// closing line is directly above the token's line, with nothing but whitespace
    /// between them. A blank line, another comment, or sharing the token's line breaks
    /// the attachment and leaves the comment floating.
    #[must_use]
    pub fn doc_comment(&self) -> Option<&Trivia> {
        let mut before = self
            .leading
            .iter()
            .rev()
            .skip_while(|t| t.kind == TriviaKind::Whitespace);
        if !matches!(before.next()?.kind, TriviaKind::LineTerminator(_)) {
            return None;
        }
        before
            .find(|t| t.kind != TriviaKind::Whitespace)
            .filter(|t| matches!(t.kind, TriviaKind::DocComment { .. }))
    }
}

/// Every token of a file, `Eof` last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenStream {
    /// The tokens in source order.
    pub tokens: Vec<Token>,
}

impl TokenStream {
    /// The source text, reassembled from trivia and tokens.
    #[must_use]
    pub fn print(&self, file: &SourceFile) -> String {
        let mut out = String::with_capacity(file.len());
        for token in &self.tokens {
            for trivia in &token.leading {
                out.push_str(file.slice(&trivia.span).unwrap_or(""));
            }
            out.push_str(file.slice(&token.span).unwrap_or(""));
        }
        out
    }

    /// The `Eof` token, which every stream ends with.
    #[must_use]
    pub fn eof(&self) -> &Token {
        let last = self.tokens.len().saturating_sub(1);
        &self.tokens[last]
    }
}
