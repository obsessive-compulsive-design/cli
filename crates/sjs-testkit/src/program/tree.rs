//! The generator's own syntax tree: just enough structure to render every §2 production
//! and to shrink by dropping items and statements.

/// A whole module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// Imports first, then declarations and statements.
    pub items: Vec<Item>,
}

/// A module item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// `import …`
    Import(Import),
    /// `export <declaration>`
    Export(Decl),
    /// `export { a as b, … }` with an optional `from`.
    ExportList(Vec<(String, Option<String>)>, Option<String>),
    /// A declaration.
    Decl(Decl),
    /// A statement.
    Stmt(Stmt),
}

/// An import declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Import {
    /// `import { a as b, … } from "./x.js"`
    Named(Vec<(String, Option<String>)>, String),
    /// `import * as ns from "./x.js"`
    Namespace(String, String),
    /// `import cfg from "./c.json" with { type: "json" }`
    Json(String, String),
}

/// A declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    /// `const <pattern> = <expr>`
    Const(Pattern, Expr),
    /// `let <pattern> = <expr>`
    Let(Pattern, Expr),
    /// `[async] function [*] name(params) { body }`
    Function {
        /// `async`
        is_async: bool,
        /// `function*`
        is_generator: bool,
        /// The name.
        name: String,
        /// Parameters.
        params: Vec<Param>,
        /// The body.
        body: Vec<Stmt>,
    },
    /// `class Name [extends Base] { members }`
    Class {
        /// The name.
        name: String,
        /// `extends Ident` or `extends ns.Ident`.
        extends: Option<String>,
        /// The members.
        members: Vec<Member>,
    },
}

/// A class member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Member {
    /// `[static] name [= expr]`
    Field {
        /// `static`
        is_static: bool,
        /// `name` or `#name`.
        key: MemberKey,
        /// The initializer.
        value: Option<Expr>,
    },
    /// `[static] [async] [*] key(params) { body }`
    Method {
        /// `static`
        is_static: bool,
        /// `async`
        is_async: bool,
        /// `*`
        is_generator: bool,
        /// The key.
        key: MemberKey,
        /// Parameters.
        params: Vec<Param>,
        /// The body.
        body: Vec<Stmt>,
    },
    /// `constructor(params) { body }`
    Constructor(Vec<Param>, Vec<Stmt>),
    /// `[static] get key() { body }`
    Getter(bool, MemberKey, Vec<Stmt>),
    /// `[static] set key(param) { body }`
    Setter(bool, MemberKey, Param, Vec<Stmt>),
    /// `static { body }`
    StaticBlock(Vec<Stmt>),
}

/// A class member key (§2 `MemberKey`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberKey {
    /// A plain name.
    Ident(String),
    /// `#name`
    Private(String),
    /// `[Symbol.iterator]`, `[Symbol.asyncIterator]`, `[Symbol.dispose]`, `[Symbol.asyncDispose]`.
    Symbol(&'static str),
}

/// A parameter: a pattern with an optional default, or a rest parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    /// `pattern [= default]`
    Plain(Pattern, Option<Expr>),
    /// `...name`
    Rest(String),
}

/// A binding pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pattern {
    /// A name.
    Ident(String),
    /// `{ a, b: pattern = default, ...rest }`
    Object(
        Vec<(String, Option<Box<Pattern>>, Option<Expr>)>,
        Option<String>,
    ),
    /// `[pattern = default, ...rest]`
    Array(Vec<(Pattern, Option<Expr>)>, Option<String>),
}

/// A statement (§2 `Statement`), plus declarations in blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `{ … }`
    Block(Vec<Stmt>),
    /// `if (cond) { … } [else …]`
    If(Expr, Vec<Stmt>, Option<Box<Else>>),
    /// `for [await] (const|let pattern of iterable) { … }`
    ForOf {
        /// `for await`
        is_await: bool,
        /// `let` rather than `const`.
        is_let: bool,
        /// The binding.
        pattern: Pattern,
        /// The iterable.
        iterable: Expr,
        /// The body.
        body: Vec<Stmt>,
    },
    /// `for (let i = init; test; update) { … }`
    For {
        /// The loop variable.
        var: String,
        /// Its initializer.
        init: Expr,
        /// The test.
        test: Expr,
        /// The update.
        update: ForUpdate,
        /// The body.
        body: Vec<Stmt>,
    },
    /// `while (cond) { … }`
    While(Expr, Vec<Stmt>),
    /// `switch (x) { case …: { … } … [default: { … }] }`
    Switch(Expr, Vec<(Vec<Literal>, Vec<Stmt>)>, Option<Vec<Stmt>>),
    /// `try { … } [catch [(e)] { … }] [finally { … }]`
    Try(
        Vec<Stmt>,
        Option<(Option<String>, Vec<Stmt>)>,
        Option<Vec<Stmt>>,
    ),
    /// `return [expr]`
    Return(Option<Expr>),
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `throw expr`
    Throw(Expr),
    /// An expression statement.
    Expr(Expr),
    /// A declaration inside a block.
    Decl(Decl),
}

/// The `else` part of an `if`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Else {
    /// `else { … }`
    Block(Vec<Stmt>),
    /// `else if …`
    If(Expr, Vec<Stmt>, Option<Box<Else>>),
}

/// The update of a classic `for` header (§2 `ForUpdate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForUpdate {
    /// `i += expr`
    AddAssign(Expr),
    /// `i -= expr`
    SubAssign(Expr),
    /// `i++`
    PostInc,
    /// `i--`
    PostDec,
    /// `++i`
    PreInc,
    /// `--i`
    PreDec,
}

/// A literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// A number, as written.
    Number(String),
    /// A bigint, as written (with `n`).
    BigInt(String),
    /// A string (unescaped content).
    Str(String),
    /// `true` / `false`
    Bool(bool),
    /// `undefined`
    Undefined,
}

/// An object literal property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Property {
    /// `name`
    Shorthand(String),
    /// `name: expr`
    Named(String, Expr),
    /// `"string": expr`
    StringKey(String, Expr),
    /// `[expr]: expr`
    Computed(Expr, Expr),
    /// `...expr`
    Spread(Expr),
}

/// A piece of a template literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Literal text (no backticks, no `${`).
    Text(String),
    /// `${expr}`
    Substitution(Expr),
}

/// An expression (§2 `Expr`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Literal(Literal),
    /// A name.
    Ident(String),
    /// `this`
    This,
    /// `[a, ...b]`
    Array(Vec<Expr>, Vec<bool>),
    /// `{ … }`
    Object(Vec<Property>),
    /// `[async] (params) => expr | { body }`
    Arrow {
        /// `async`
        is_async: bool,
        /// Parameters.
        params: Vec<Param>,
        /// Expression body (`Ok`) or block body (`Err`).
        body: Result<Box<Expr>, Vec<Stmt>>,
    },
    /// `` `…${…}…` ``
    Template(Vec<TemplatePart>),
    /// `` String.raw`…` ``
    StringRaw(Vec<TemplatePart>),
    /// `/body/flags`
    Regex(String, String),
    /// `new Name(args)` / `new ns.Name(args)`
    New(String, Vec<Expr>),
    /// `import("./x.js")`
    DynamicImport(String),
    /// `(expr)` written by the author.
    Paren(Box<Expr>),
    /// `obj.name` / `obj?.name`
    Member(Box<Expr>, String, bool),
    /// `obj[index]` / `obj?.[index]`
    Index(Box<Expr>, Box<Expr>, bool),
    /// `callee(args)` / `callee?.(args)`
    Call(Box<Expr>, Vec<Expr>, bool),
    /// `super.name`
    SuperMember(String),
    /// `super(args)`
    SuperCall(Vec<Expr>),
    /// `op expr` for `! - + ~ typeof await`
    Unary(&'static str, Box<Expr>),
    /// `left op right` for every binary operator of §2.
    Binary(Box<Expr>, &'static str, Box<Expr>),
    /// `cond ? a : b`
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `target op= value` for `= += -= *= /= %= **= ??=`
    Assign(Box<Expr>, &'static str, Box<Expr>),
    /// `yield [expr]` / `yield* expr`
    Yield(Option<Box<Expr>>, bool),
}
