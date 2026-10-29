//! The generator: random trees over the §2 grammar, recording every production used, with
//! statement-level shrinking.

use std::collections::BTreeSet;

use super::tree::{
    Decl, Else, Expr, ForUpdate, Import, Item, Literal, Member, MemberKey, Param, Pattern, Program,
    Property, Stmt, TemplatePart,
};
use crate::prng::Rng;
use crate::prop::Gen;

/// Names a generated program must never use: keywords, contextual words, removed words
/// and every §6 global, so a program never shadows or collides with them.
pub const RESERVED: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "else",
    "export",
    "extends",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "import",
    "instanceof",
    "let",
    "new",
    "of",
    "return",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "while",
    "yield",
    "null",
    "true",
    "false",
    "undefined",
    "var",
    "with",
    "void",
    "delete",
    "in",
    "arguments",
    "eval",
    "debugger",
    "enum",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    "Object",
    "Array",
    "String",
    "Number",
    "BigInt",
    "Math",
    "Symbol",
    "Map",
    "Set",
    "WeakMap",
    "Promise",
    "Iterator",
    "JSON",
    "RegExp",
    "Error",
    "Intl",
    "Date",
    "Temporal",
    "structuredClone",
    "fetch",
    "Request",
    "Response",
    "Headers",
    "FormData",
    "URL",
    "URLSearchParams",
    "URLPattern",
    "AbortController",
    "AbortSignal",
    "EventTarget",
    "Event",
    "CustomEvent",
    "ReadableStream",
    "WritableStream",
    "TransformStream",
    "TextEncoderStream",
    "TextDecoderStream",
    "CompressionStream",
    "DecompressionStream",
    "Blob",
    "File",
    "TextEncoder",
    "TextDecoder",
    "WebSocket",
    "BroadcastChannel",
    "MessageChannel",
    "MessagePort",
    "crypto",
    "queueMicrotask",
    "setTimeout",
    "clearTimeout",
    "setInterval",
    "clearInterval",
    "performance",
    "console",
    "navigator",
    "atob",
    "btoa",
    "NaN",
    "Infinity",
    "describe",
    "it",
    "expect",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];

/// The identifier pool, exposed for the reserved-name test.
#[cfg(test)]
pub(super) const NAMES_FOR_TESTS: &[&str] = NAMES;

const NAMES: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "item", "total", "count", "label", "token", "node", "left",
    "right", "width", "height", "parsed", "result", "input", "output", "shape", "config",
];
const TYPE_NAMES: &[&str] = &["Widget", "Parser", "Cursor", "Shape", "Circle", "Report"];
const MODULES: &[&str] = &[
    "./util.js",
    "./shapes/circle.js",
    "../lib/parse.js",
    "./config.json",
];
const WORDS: &[&str] = &[
    "hello",
    "a \"quoted\" word",
    "back\\slash",
    "line\nbreak",
    "tab\there",
    "",
    "é 😀",
];

/// Every production the generator can emit; the coverage test asserts each appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(missing_docs)]
pub enum Production {
    ImportNamed,
    ImportAlias,
    ImportNamespace,
    ImportJson,
    ExportDecl,
    ExportList,
    ExportAlias,
    ExportFrom,
    Const,
    Let,
    Function,
    AsyncFunction,
    Generator,
    AsyncGenerator,
    Class,
    ClassExtends,
    ClassExtendsQualified,
    Field,
    FieldInit,
    StaticField,
    PrivateField,
    Method,
    AsyncMethod,
    GeneratorMethod,
    StaticMethod,
    PrivateMethod,
    SymbolMethod,
    Constructor,
    Getter,
    Setter,
    StaticAccessor,
    StaticBlock,
    Block,
    If,
    Else,
    ElseIf,
    ForOf,
    ForOfLet,
    ForAwait,
    For,
    ForAddAssign,
    ForSubAssign,
    ForPostInc,
    ForPostDec,
    ForPreInc,
    ForPreDec,
    While,
    Switch,
    SwitchStacked,
    SwitchDefault,
    Try,
    Catch,
    CatchNoBinding,
    Finally,
    Return,
    ReturnValue,
    Break,
    Continue,
    Throw,
    ExprStmt,
    BlockDecl,
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    RemAssign,
    ExpAssign,
    NullishAssign,
    Ternary,
    Nullish,
    Or,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEq,
    GreaterEq,
    InstanceOf,
    ShiftLeft,
    ShiftRight,
    ShiftRightUnsigned,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Exp,
    Not,
    Negate,
    Plus,
    BitNot,
    TypeOf,
    Await,
    TopLevelAwait,
    Member,
    OptionalMember,
    Index,
    OptionalIndex,
    Call,
    OptionalCall,
    Ident,
    This,
    Array,
    ArraySpread,
    Object,
    ObjectShorthand,
    ObjectNamed,
    ObjectStringKey,
    ObjectComputed,
    ObjectSpread,
    Arrow,
    AsyncArrow,
    ArrowBlock,
    ArrowObjectBody,
    Template,
    TemplateSubstitution,
    StringRaw,
    Regex,
    New,
    NewQualified,
    DynamicImport,
    Paren,
    Number,
    Double,
    BigInt,
    Str,
    Bool,
    Undefined,
    NullComparison,
    Yield,
    YieldValue,
    YieldDelegate,
    SuperCall,
    SuperMember,
    ObjectPattern,
    ObjectPatternNested,
    ObjectPatternRest,
    ArrayPattern,
    ArrayPatternRest,
    PatternDefault,
    DefaultParam,
    RestParam,
    NullishMixGuarded,
}

impl Production {
    /// Every production, for the coverage test.
    pub const ALL: &'static [Production] = &[
        Production::ImportNamed,
        Production::ImportAlias,
        Production::ImportNamespace,
        Production::ImportJson,
        Production::ExportDecl,
        Production::ExportList,
        Production::ExportAlias,
        Production::ExportFrom,
        Production::Const,
        Production::Let,
        Production::Function,
        Production::AsyncFunction,
        Production::Generator,
        Production::AsyncGenerator,
        Production::Class,
        Production::ClassExtends,
        Production::ClassExtendsQualified,
        Production::Field,
        Production::FieldInit,
        Production::StaticField,
        Production::PrivateField,
        Production::Method,
        Production::AsyncMethod,
        Production::GeneratorMethod,
        Production::StaticMethod,
        Production::PrivateMethod,
        Production::SymbolMethod,
        Production::Constructor,
        Production::Getter,
        Production::Setter,
        Production::StaticAccessor,
        Production::StaticBlock,
        Production::Block,
        Production::If,
        Production::Else,
        Production::ElseIf,
        Production::ForOf,
        Production::ForOfLet,
        Production::ForAwait,
        Production::For,
        Production::ForAddAssign,
        Production::ForSubAssign,
        Production::ForPostInc,
        Production::ForPostDec,
        Production::ForPreInc,
        Production::ForPreDec,
        Production::While,
        Production::Switch,
        Production::SwitchStacked,
        Production::SwitchDefault,
        Production::Try,
        Production::Catch,
        Production::CatchNoBinding,
        Production::Finally,
        Production::Return,
        Production::ReturnValue,
        Production::Break,
        Production::Continue,
        Production::Throw,
        Production::ExprStmt,
        Production::BlockDecl,
        Production::Assign,
        Production::AddAssign,
        Production::SubAssign,
        Production::MulAssign,
        Production::DivAssign,
        Production::RemAssign,
        Production::ExpAssign,
        Production::NullishAssign,
        Production::Ternary,
        Production::Nullish,
        Production::Or,
        Production::And,
        Production::BitOr,
        Production::BitXor,
        Production::BitAnd,
        Production::Equal,
        Production::NotEqual,
        Production::Less,
        Production::Greater,
        Production::LessEq,
        Production::GreaterEq,
        Production::InstanceOf,
        Production::ShiftLeft,
        Production::ShiftRight,
        Production::ShiftRightUnsigned,
        Production::Add,
        Production::Sub,
        Production::Mul,
        Production::Div,
        Production::Rem,
        Production::Exp,
        Production::Not,
        Production::Negate,
        Production::Plus,
        Production::BitNot,
        Production::TypeOf,
        Production::Await,
        Production::TopLevelAwait,
        Production::Member,
        Production::OptionalMember,
        Production::Index,
        Production::OptionalIndex,
        Production::Call,
        Production::OptionalCall,
        Production::Ident,
        Production::This,
        Production::Array,
        Production::ArraySpread,
        Production::Object,
        Production::ObjectShorthand,
        Production::ObjectNamed,
        Production::ObjectStringKey,
        Production::ObjectComputed,
        Production::ObjectSpread,
        Production::Arrow,
        Production::AsyncArrow,
        Production::ArrowBlock,
        Production::ArrowObjectBody,
        Production::Template,
        Production::TemplateSubstitution,
        Production::StringRaw,
        Production::Regex,
        Production::New,
        Production::NewQualified,
        Production::DynamicImport,
        Production::Paren,
        Production::Number,
        Production::Double,
        Production::BigInt,
        Production::Str,
        Production::Bool,
        Production::Undefined,
        Production::NullComparison,
        Production::Yield,
        Production::YieldValue,
        Production::YieldDelegate,
        Production::SuperCall,
        Production::SuperMember,
        Production::ObjectPattern,
        Production::ObjectPatternNested,
        Production::ObjectPatternRest,
        Production::ArrayPattern,
        Production::ArrayPatternRest,
        Production::PatternDefault,
        Production::DefaultParam,
        Production::RestParam,
        Production::NullishMixGuarded,
    ];
}

/// What kind of program to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// How deep statements and expressions may nest.
    pub max_depth: u8,
    /// Most module items (after imports).
    pub max_items: u8,
    /// Allow non-integer number literals (`0.5`); otherwise every number is an `int32`.
    pub doubles: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_depth: 4,
            max_items: 6,
            doubles: false,
        }
    }
}

/// Where the generator is, so it only emits what the grammar allows there.
#[derive(Debug, Clone, Copy, Default)]
struct Ctx {
    flags: u8,
    depth: u8,
}

const IN_FUNCTION: u8 = 1;
const IS_ASYNC: u8 = 2;
const IS_GENERATOR: u8 = 4;
const IN_LOOP: u8 = 8;
const IN_SWITCH: u8 = 16;
const IN_CLASS: u8 = 32;
const IN_DERIVED_CONSTRUCTOR: u8 = 64;
/// A static block: a member body like a method's (no `await`), but `return` is an
/// early error there.
const IN_STATIC_BLOCK: u8 = 128;

/// The flags whose condition holds, added (never or-ed) so each bit is counted once.
fn flags_of(flags: &[(bool, u8)]) -> u8 {
    flags.iter().filter(|(on, _)| *on).map(|(_, bit)| bit).sum()
}

/// The context of a `function` body.
fn function_ctx(depth: u8, is_async: bool, is_generator: bool) -> Ctx {
    Ctx {
        flags: flags_of(&[
            (true, IN_FUNCTION),
            (is_async, IS_ASYNC),
            (is_generator, IS_GENERATOR),
        ]),
        depth,
    }
}

/// The context of a class member body.
fn member_ctx(depth: u8) -> Ctx {
    Ctx {
        flags: flags_of(&[(true, IN_FUNCTION), (true, IN_CLASS)]),
        depth,
    }
}

/// The context of a method body: the class context with its own async/generator flags.
fn method_ctx(class: Ctx, is_async: bool, is_generator: bool) -> Ctx {
    Ctx {
        flags: flags_of(&[
            (class.has(IN_FUNCTION), IN_FUNCTION),
            (class.has(IN_CLASS), IN_CLASS),
            (class.has(IN_DERIVED_CONSTRUCTOR), IN_DERIVED_CONSTRUCTOR),
            (is_async, IS_ASYNC),
            (is_generator, IS_GENERATOR),
        ]),
        depth: class.depth,
    }
}

/// The context of an arrow body: a function that keeps the class flags of its home.
fn arrow_ctx(home: Ctx, is_async: bool) -> Ctx {
    Ctx {
        flags: flags_of(&[
            (true, IN_FUNCTION),
            (is_async, IS_ASYNC),
            (home.has(IN_CLASS), IN_CLASS),
            (home.has(IN_DERIVED_CONSTRUCTOR), IN_DERIVED_CONSTRUCTOR),
        ]),
        depth: home.depth + 1,
    }
}

impl Ctx {
    const fn has(self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    const fn with(self, flag: u8) -> Self {
        Self {
            flags: self.flags | flag,
            depth: self.depth,
        }
    }

    const fn deeper(self) -> Self {
        Self {
            flags: self.flags,
            depth: self.depth + 1,
        }
    }
}

/// A [`Gen`] of programs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramGen {
    /// The options.
    pub options: Options,
}

struct Generator<'a> {
    rng: &'a mut Rng,
    options: Options,
    used: BTreeSet<Production>,
    /// Names the program has exported: a name is exported once (an ES2025 early error
    /// otherwise, E000 in the parser).
    exported: BTreeSet<String>,
}

impl ProgramGen {
    /// Generates a program and reports the productions it used.
    pub fn generate_with_coverage(&self, rng: &mut Rng) -> (Program, BTreeSet<Production>) {
        let mut g = Generator {
            rng,
            options: self.options,
            used: BTreeSet::new(),
            exported: BTreeSet::new(),
        };
        let program = g.program();
        (program, g.used)
    }
}

impl Gen for ProgramGen {
    type Value = Program;

    fn generate(&self, rng: &mut Rng) -> Program {
        self.generate_with_coverage(rng).0
    }

    /// Every program with one item or one nested statement removed.
    fn shrink(&self, value: &Program) -> Vec<Program> {
        shrink_items(&value.items)
            .into_iter()
            .map(|items| Program { items })
            .collect()
    }
}

fn shrink_items(items: &[Item]) -> Vec<Vec<Item>> {
    let mut out = Vec::new();
    for index in 0..items.len() {
        let mut fewer = items.to_vec();
        fewer.remove(index);
        out.push(fewer);
    }
    for (index, item) in items.iter().enumerate() {
        let nested = match item {
            Item::Export(decl) | Item::Decl(decl) => shrink_decl(decl)
                .into_iter()
                .map(|d| wrap_decl(item, d))
                .collect(),
            Item::Stmt(stmt) => shrink_stmt(stmt).into_iter().map(Item::Stmt).collect(),
            Item::Import(_) | Item::ExportList(..) => Vec::new(),
        };
        for replacement in nested {
            let mut changed = items.to_vec();
            changed[index] = replacement;
            out.push(changed);
        }
    }
    out
}

fn wrap_decl(original: &Item, decl: Decl) -> Item {
    match original {
        Item::Export(_) => Item::Export(decl),
        _ => Item::Decl(decl),
    }
}

/// Every version of `stmts` with one statement removed or one nested statement removed.
fn shrink_block(stmts: &[Stmt]) -> Vec<Vec<Stmt>> {
    let mut out = Vec::new();
    for index in 0..stmts.len() {
        let mut fewer = stmts.to_vec();
        fewer.remove(index);
        out.push(fewer);
    }
    for (index, stmt) in stmts.iter().enumerate() {
        for replacement in shrink_stmt(stmt) {
            let mut changed = stmts.to_vec();
            changed[index] = replacement;
            out.push(changed);
        }
    }
    out
}

fn shrink_decl(decl: &Decl) -> Vec<Decl> {
    match decl {
        Decl::Function {
            is_async,
            is_generator,
            name,
            params,
            body,
        } => shrink_block(body)
            .into_iter()
            .map(|body| Decl::Function {
                is_async: *is_async,
                is_generator: *is_generator,
                name: name.clone(),
                params: params.clone(),
                body,
            })
            .collect(),
        Decl::Class {
            name,
            extends,
            members,
        } => {
            let mut out = Vec::new();
            for index in 0..members.len() {
                let mut fewer = members.clone();
                fewer.remove(index);
                out.push(Decl::Class {
                    name: name.clone(),
                    extends: extends.clone(),
                    members: fewer,
                });
            }
            out
        }
        Decl::Const(..) | Decl::Let(..) => Vec::new(),
    }
}

fn shrink_stmt(stmt: &Stmt) -> Vec<Stmt> {
    match stmt {
        Stmt::Block(body) => shrink_block(body).into_iter().map(Stmt::Block).collect(),
        Stmt::If(cond, then, otherwise) => {
            let mut out: Vec<Stmt> = shrink_block(then)
                .into_iter()
                .map(|then| Stmt::If(cond.clone(), then, otherwise.clone()))
                .collect();
            if otherwise.is_some() {
                out.push(Stmt::If(cond.clone(), then.clone(), None));
            }
            out
        }
        Stmt::ForOf {
            is_await,
            is_let,
            pattern,
            iterable,
            body,
        } => shrink_block(body)
            .into_iter()
            .map(|body| Stmt::ForOf {
                is_await: *is_await,
                is_let: *is_let,
                pattern: pattern.clone(),
                iterable: iterable.clone(),
                body,
            })
            .collect(),
        Stmt::For {
            var,
            init,
            test,
            update,
            body,
        } => shrink_block(body)
            .into_iter()
            .map(|body| Stmt::For {
                var: var.clone(),
                init: init.clone(),
                test: test.clone(),
                update: update.clone(),
                body,
            })
            .collect(),
        Stmt::While(cond, body) => shrink_block(body)
            .into_iter()
            .map(|body| Stmt::While(cond.clone(), body))
            .collect(),
        Stmt::Switch(scrutinee, cases, default) => {
            let mut out = Vec::new();
            for index in 0..cases.len() {
                let mut fewer = cases.clone();
                fewer.remove(index);
                out.push(Stmt::Switch(scrutinee.clone(), fewer, default.clone()));
            }
            if default.is_some() {
                out.push(Stmt::Switch(scrutinee.clone(), cases.clone(), None));
            }
            out
        }
        Stmt::Try(body, catch, finally) => {
            let mut out: Vec<Stmt> = shrink_block(body)
                .into_iter()
                .map(|body| Stmt::Try(body, catch.clone(), finally.clone()))
                .collect();
            if catch.is_some() && finally.is_some() {
                out.push(Stmt::Try(body.clone(), None, finally.clone()));
                out.push(Stmt::Try(body.clone(), catch.clone(), None));
            }
            out
        }
        Stmt::Decl(decl) => shrink_decl(decl).into_iter().map(Stmt::Decl).collect(),
        Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Throw(_) | Stmt::Expr(_) => {
            Vec::new()
        }
    }
}

impl Generator<'_> {
    fn note(&mut self, production: Production) {
        self.used.insert(production);
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let index = self.rng.index(items.len());
        items[index]
    }

    fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.rng.chance(numerator, denominator)
    }

    fn name(&mut self) -> String {
        let base = self.pick(NAMES);
        if self.chance(1, 3) {
            format!("{base}{}", self.rng.below(10))
        } else {
            base.to_owned()
        }
    }

    fn type_name(&mut self) -> String {
        self.pick(TYPE_NAMES).to_owned()
    }

    fn module_path(&mut self) -> String {
        let path = self.pick(&MODULES[..3]);
        path.to_owned()
    }

    fn program(&mut self) -> Program {
        let mut items = Vec::new();
        let import_count = self.rng.index(3);
        for _ in 0..import_count {
            items.push(Item::Import(self.import()));
        }
        let count = 1 + self.rng.index(usize::from(self.options.max_items));
        let ctx = Ctx::default();
        for _ in 0..count {
            items.push(self.item(ctx));
        }
        Program { items }
    }

    fn import(&mut self) -> Import {
        match self.rng.below(3) {
            0 => {
                self.note(Production::ImportNamed);
                let names = self.aliases(Production::ImportAlias);
                Import::Named(names, self.module_path())
            }
            1 => {
                self.note(Production::ImportNamespace);
                Import::Namespace(self.name(), self.module_path())
            }
            _ => {
                self.note(Production::ImportJson);
                Import::Json(self.name(), MODULES[3].to_owned())
            }
        }
    }

    fn aliases(&mut self, alias_production: Production) -> Vec<(String, Option<String>)> {
        let count = 1 + self.rng.index(3);
        (0..count)
            .map(|_| {
                let name = self.name();
                let alias = if self.chance(1, 3) {
                    self.note(alias_production);
                    Some(self.name())
                } else {
                    None
                };
                (name, alias)
            })
            .collect()
    }

    /// `name [as alias]` with the exported name made unique in the program: a name
    /// exported before gets a fresh alias.
    fn unique_export(&mut self, name: String, alias: Option<&str>) -> (String, Option<String>) {
        let base = alias.unwrap_or(&name);
        let mut exported = base.to_owned();
        let mut suffix = 0;
        while self.exported.contains(&exported) {
            exported = format!("{base}{suffix}");
            suffix += 1;
        }
        self.exported.insert(exported.clone());
        let alias = (exported != name).then_some(exported);
        (name, alias)
    }

    fn item(&mut self, ctx: Ctx) -> Item {
        match self.rng.below(10) {
            0 | 1 => {
                let decl = self.decl(ctx);
                let names = decl.bound_names();
                if names.iter().any(|name| self.exported.contains(name)) {
                    // Already exported under one of these names: keep the declaration
                    // private so the program stays valid.
                    return Item::Decl(decl);
                }
                self.exported.extend(names);
                self.note(Production::ExportDecl);
                Item::Export(decl)
            }
            2 => {
                self.note(Production::ExportList);
                let names = self
                    .aliases(Production::ExportAlias)
                    .into_iter()
                    .map(|(name, alias)| self.unique_export(name, alias.as_deref()))
                    .collect();
                let from = if self.chance(1, 2) {
                    self.note(Production::ExportFrom);
                    Some(self.module_path())
                } else {
                    None
                };
                Item::ExportList(names, from)
            }
            3..=5 => Item::Decl(self.decl(ctx)),
            _ => Item::Stmt(self.stmt(ctx)),
        }
    }

    fn decl(&mut self, ctx: Ctx) -> Decl {
        match self.rng.below(6) {
            0 | 1 => {
                self.note(Production::Const);
                Decl::Const(self.pattern(ctx), self.expr(ctx.deeper()))
            }
            2 => {
                self.note(Production::Let);
                Decl::Let(self.pattern(ctx), self.expr(ctx.deeper()))
            }
            3 | 4 => self.function(ctx),
            _ => self.class(ctx),
        }
    }

    fn function(&mut self, ctx: Ctx) -> Decl {
        let is_async = self.chance(1, 3);
        let is_generator = self.chance(1, 3);
        self.note(match (is_async, is_generator) {
            (false, false) => Production::Function,
            (true, false) => Production::AsyncFunction,
            (false, true) => Production::Generator,
            (true, true) => Production::AsyncGenerator,
        });
        let inner = function_ctx(ctx.depth + 1, is_async, is_generator);
        Decl::Function {
            is_async,
            is_generator,
            name: self.name(),
            params: self.params(inner),
            body: self.block(inner),
        }
    }

    /// A parameter list whose names are unique across every parameter (a name declared
    /// twice is an early error, E000 in the parser).
    fn params(&mut self, ctx: Ctx) -> Vec<Param> {
        let count = self.rng.index(4);
        let mut taken = BTreeSet::new();
        let mut params: Vec<Param> = Vec::new();
        for _ in 0..count {
            let pattern = self.pattern_in(ctx, &mut taken);
            let default = if self.chance(1, 4) {
                self.note(Production::DefaultParam);
                Some(self.expr(ctx.deeper()))
            } else {
                None
            };
            params.push(Param::Plain(pattern, default));
        }
        if self.chance(1, 4) {
            self.note(Production::RestParam);
            params.push(Param::Rest(self.fresh(&mut taken)));
        }
        params
    }

    fn pattern(&mut self, ctx: Ctx) -> Pattern {
        let mut taken = BTreeSet::new();
        self.pattern_in(ctx, &mut taken)
    }

    /// A name no earlier part of the same pattern binds: binding one name twice in a
    /// pattern is an early error (E000 in the parser).
    fn fresh(&mut self, taken: &mut BTreeSet<String>) -> String {
        let base = self.name();
        let mut name = base.clone();
        let mut suffix = 0;
        while taken.contains(&name) {
            name = format!("{base}{suffix}");
            suffix += 1;
        }
        taken.insert(name.clone());
        name
    }

    fn pattern_in(&mut self, ctx: Ctx, taken: &mut BTreeSet<String>) -> Pattern {
        if ctx.depth >= self.options.max_depth || self.chance(2, 3) {
            return Pattern::Ident(self.fresh(taken));
        }
        if self.chance(1, 2) {
            self.note(Production::ObjectPattern);
            let count = 1 + self.rng.index(3);
            let mut entries = Vec::new();
            for _ in 0..count {
                let (key, target) = if self.chance(1, 3) {
                    self.note(Production::ObjectPatternNested);
                    let key = self.name();
                    (key, Some(Box::new(self.pattern_in(ctx.deeper(), taken))))
                } else {
                    (self.fresh(taken), None)
                };
                let default = if self.chance(1, 3) {
                    self.note(Production::PatternDefault);
                    Some(self.simple_expr())
                } else {
                    None
                };
                entries.push((key, target, default));
            }
            let rest = if self.chance(1, 3) {
                self.note(Production::ObjectPatternRest);
                Some(self.fresh(taken))
            } else {
                None
            };
            Pattern::Object(entries, rest)
        } else {
            self.note(Production::ArrayPattern);
            let count = 1 + self.rng.index(3);
            let mut entries = Vec::new();
            for _ in 0..count {
                let target = self.pattern_in(ctx.deeper(), taken);
                let default = if self.chance(1, 3) {
                    self.note(Production::PatternDefault);
                    Some(self.simple_expr())
                } else {
                    None
                };
                entries.push((target, default));
            }
            let rest = if self.chance(1, 3) {
                self.note(Production::ArrayPatternRest);
                Some(self.fresh(taken))
            } else {
                None
            };
            Pattern::Array(entries, rest)
        }
    }

    fn class(&mut self, ctx: Ctx) -> Decl {
        self.note(Production::Class);
        let extends = match self.rng.below(3) {
            0 => None,
            1 => {
                self.note(Production::ClassExtends);
                Some(self.type_name())
            }
            _ => {
                self.note(Production::ClassExtendsQualified);
                let ns = self.name();
                Some(format!("{ns}.{}", self.type_name()))
            }
        };
        let count = self.rng.index(5);
        let member_ctx = member_ctx(ctx.depth + 1);
        let mut taken = BTreeSet::new();
        let mut members: Vec<Member> = (0..count)
            .map(|_| self.member(member_ctx, &mut taken))
            .collect();
        // `x = 1` followed by `[Symbol.iterator]() {}` would parse as an index expression
        // and by `*g() {}` as a multiplication (no ASI inside class bodies), so
        // computed-key methods and bare generator methods go before every field.
        members.sort_by_key(|member| {
            !matches!(
                member,
                Member::Method {
                    key: MemberKey::Symbol(_),
                    ..
                } | Member::Method {
                    is_static: false,
                    is_async: false,
                    is_generator: true,
                    ..
                }
            )
        });
        if self.chance(1, 2) {
            self.note(Production::Constructor);
            let ctor_ctx = if extends.is_some() {
                member_ctx.with(IN_DERIVED_CONSTRUCTOR)
            } else {
                member_ctx
            };
            let params = self.params(ctor_ctx);
            let mut body = Vec::new();
            if extends.is_some() {
                self.note(Production::SuperCall);
                body.push(Stmt::Expr(Expr::SuperCall(self.args(ctor_ctx))));
            }
            body.extend(self.block(ctor_ctx));
            members.insert(0, Member::Constructor(params, body));
        }
        Decl::Class {
            name: self.type_name(),
            extends,
            members,
        }
    }

    /// A member key; a private name is declared once per class (E000 otherwise), so
    /// `taken` holds the ones used so far.
    fn member_key(&mut self, taken: &mut BTreeSet<String>) -> MemberKey {
        match self.rng.below(5) {
            0 => {
                self.note(Production::PrivateField);
                MemberKey::Private(self.fresh(taken))
            }
            _ => MemberKey::Ident(self.name()),
        }
    }

    fn member(&mut self, ctx: Ctx, taken: &mut BTreeSet<String>) -> Member {
        match self.rng.below(7) {
            0 | 1 => {
                let is_static = self.chance(1, 4);
                self.note(if is_static {
                    Production::StaticField
                } else {
                    Production::Field
                });
                let key = self.member_key(taken);
                let value = if self.chance(1, 2) {
                    self.note(Production::FieldInit);
                    Some(self.expr(ctx.deeper()))
                } else {
                    None
                };
                Member::Field {
                    is_static,
                    key,
                    value,
                }
            }
            2 | 3 => {
                let is_static = self.chance(1, 4);
                let is_async = self.chance(1, 3);
                let is_generator = self.chance(1, 3);
                self.note(Production::Method);
                if is_static {
                    self.note(Production::StaticMethod);
                }
                if is_async {
                    self.note(Production::AsyncMethod);
                }
                if is_generator {
                    self.note(Production::GeneratorMethod);
                }
                let key = match self.rng.below(5) {
                    0 => {
                        self.note(Production::SymbolMethod);
                        MemberKey::Symbol(self.pick(&[
                            "iterator",
                            "asyncIterator",
                            "dispose",
                            "asyncDispose",
                        ]))
                    }
                    1 => {
                        self.note(Production::PrivateMethod);
                        MemberKey::Private(self.fresh(taken))
                    }
                    _ => MemberKey::Ident(self.name()),
                };
                let inner = method_ctx(ctx, is_async, is_generator);
                Member::Method {
                    is_static,
                    is_async,
                    is_generator,
                    key,
                    params: self.params(inner),
                    body: self.block(inner),
                }
            }
            4 => {
                let is_static = self.chance(1, 4);
                self.note(if is_static {
                    Production::StaticAccessor
                } else {
                    Production::Getter
                });
                Member::Getter(is_static, self.member_key(taken), self.block(ctx))
            }
            5 => {
                let is_static = self.chance(1, 4);
                self.note(if is_static {
                    Production::StaticAccessor
                } else {
                    Production::Setter
                });
                let param = Param::Plain(Pattern::Ident(self.name()), None);
                Member::Setter(is_static, self.member_key(taken), param, self.block(ctx))
            }
            _ => {
                self.note(Production::StaticBlock);
                // `await` is reserved inside a static block, as in a plain method, and
                // `return` is an early error.
                Member::StaticBlock(self.block(ctx.with(IN_STATIC_BLOCK)))
            }
        }
    }

    fn block(&mut self, ctx: Ctx) -> Vec<Stmt> {
        if ctx.depth >= self.options.max_depth {
            return Vec::new();
        }
        let count = self.rng.index(4);
        (0..count).map(|_| self.stmt(ctx)).collect()
    }

    fn stmt(&mut self, ctx: Ctx) -> Stmt {
        let deep = ctx.deeper();
        let choice = if ctx.depth + 1 >= self.options.max_depth {
            12 + self.rng.below(6)
        } else {
            self.rng.below(18)
        };
        match choice {
            0 => {
                self.note(Production::Block);
                Stmt::Block(self.block(deep))
            }
            1 | 2 => self.if_stmt(deep),
            3 => self.for_of(ctx, deep),
            4 => self.for_classic(deep),
            5 => {
                self.note(Production::While);
                let loop_ctx = deep.with(IN_LOOP);
                Stmt::While(self.expr(deep), self.block(loop_ctx))
            }
            6 => self.switch(deep),
            7 => self.try_stmt(deep),
            8 => {
                self.note(Production::BlockDecl);
                Stmt::Decl(self.decl(ctx))
            }
            9 if ctx.has(IN_FUNCTION) && !ctx.has(IN_STATIC_BLOCK) => {
                if self.chance(1, 2) {
                    self.note(Production::ReturnValue);
                    Stmt::Return(Some(self.expr(deep)))
                } else {
                    self.note(Production::Return);
                    Stmt::Return(None)
                }
            }
            10 if ctx.has(IN_LOOP) => {
                if self.chance(1, 2) {
                    self.note(Production::Break);
                    Stmt::Break
                } else {
                    self.note(Production::Continue);
                    Stmt::Continue
                }
            }
            10 if ctx.has(IN_SWITCH) => {
                self.note(Production::Break);
                Stmt::Break
            }
            11 => {
                self.note(Production::Throw);
                Stmt::Throw(Expr::New("Error".to_owned(), vec![self.simple_expr()]))
            }
            _ => {
                self.note(Production::ExprStmt);
                Stmt::Expr(self.statement_expr(ctx))
            }
        }
    }

    fn for_of(&mut self, ctx: Ctx, deep: Ctx) -> Stmt {
        let is_await = ctx.has(IS_ASYNC) || !ctx.has(IN_FUNCTION);
        let is_await = is_await && self.chance(1, 3);
        let is_let = self.chance(1, 3);
        self.note(Production::ForOf);
        if is_await {
            self.note(Production::ForAwait);
        }
        if is_let {
            self.note(Production::ForOfLet);
        }
        let loop_ctx = deep.with(IN_LOOP);
        Stmt::ForOf {
            is_await,
            is_let,
            pattern: self.pattern(deep),
            iterable: self.expr(deep),
            body: self.block(loop_ctx),
        }
    }

    fn for_classic(&mut self, deep: Ctx) -> Stmt {
        self.note(Production::For);
        let (update, production) = match self.rng.below(6) {
            0 => (
                ForUpdate::AddAssign(self.simple_expr()),
                Production::ForAddAssign,
            ),
            1 => (
                ForUpdate::SubAssign(self.simple_expr()),
                Production::ForSubAssign,
            ),
            2 => (ForUpdate::PostInc, Production::ForPostInc),
            3 => (ForUpdate::PostDec, Production::ForPostDec),
            4 => (ForUpdate::PreInc, Production::ForPreInc),
            _ => (ForUpdate::PreDec, Production::ForPreDec),
        };
        self.note(production);
        let loop_ctx = deep.with(IN_LOOP);
        let var = self.name();
        let test = Expr::Binary(
            Box::new(Expr::Ident(var.clone())),
            "<",
            Box::new(self.simple_expr()),
        );
        Stmt::For {
            var,
            init: self.simple_expr(),
            test,
            update,
            body: self.block(loop_ctx),
        }
    }

    fn if_stmt(&mut self, ctx: Ctx) -> Stmt {
        self.note(Production::If);
        let cond = self.expr(ctx);
        let then = self.block(ctx);
        let otherwise = self.else_part(ctx);
        Stmt::If(cond, then, otherwise)
    }

    fn else_part(&mut self, ctx: Ctx) -> Option<Box<Else>> {
        match self.rng.below(3) {
            0 => None,
            1 => {
                self.note(Production::Else);
                Some(Box::new(Else::Block(self.block(ctx))))
            }
            _ => {
                self.note(Production::ElseIf);
                let cond = self.expr(ctx);
                let then = self.block(ctx);
                let rest = if ctx.depth < self.options.max_depth {
                    self.else_part(ctx.deeper())
                } else {
                    None
                };
                Some(Box::new(Else::If(cond, then, rest)))
            }
        }
    }

    fn switch(&mut self, ctx: Ctx) -> Stmt {
        self.note(Production::Switch);
        let switch_ctx = ctx.with(IN_SWITCH);
        let count = 1 + self.rng.index(3);
        let mut taken = Vec::new();
        let cases = (0..count)
            .map(|_| {
                let labels = if self.chance(1, 3) {
                    self.note(Production::SwitchStacked);
                    vec![self.case_label(&mut taken), self.case_label(&mut taken)]
                } else {
                    vec![self.case_label(&mut taken)]
                };
                let mut body = self.block(switch_ctx);
                body.push(Stmt::Break);
                (labels, body)
            })
            .collect();
        let default = if self.chance(1, 2) {
            self.note(Production::SwitchDefault);
            Some(self.block(switch_ctx))
        } else {
            None
        };
        Stmt::Switch(self.expr(ctx), cases, default)
    }

    fn try_stmt(&mut self, ctx: Ctx) -> Stmt {
        self.note(Production::Try);
        let body = self.block(ctx);
        let catch = match self.rng.below(3) {
            0 => None,
            1 => {
                self.note(Production::Catch);
                Some((Some(self.name()), self.block(ctx)))
            }
            _ => {
                self.note(Production::CatchNoBinding);
                Some((None, self.block(ctx)))
            }
        };
        let finally = if catch.is_none() || self.chance(1, 2) {
            self.note(Production::Finally);
            Some(self.block(ctx))
        } else {
            None
        };
        Stmt::Try(body, catch, finally)
    }

    /// An expression that can stand as a statement: it starts with a name or keyword,
    /// never with `(`, `[`, `` ` ``, `+`, `-`, `/` or `{`, so there is no ASI hazard.
    fn statement_expr(&mut self, ctx: Ctx) -> Expr {
        let deep = ctx.deeper();
        match self.rng.below(5) {
            0 | 1 => {
                self.note(Production::Call);
                Expr::Call(Box::new(self.chain_base(ctx)), self.args(deep), false)
            }
            2 => {
                let ops = ["=", "+=", "-=", "*=", "/=", "%=", "**=", "??="];
                let op = self.pick(&ops);
                self.note(match op {
                    "=" => Production::Assign,
                    "+=" => Production::AddAssign,
                    "-=" => Production::SubAssign,
                    "*=" => Production::MulAssign,
                    "/=" => Production::DivAssign,
                    "%=" => Production::RemAssign,
                    "**=" => Production::ExpAssign,
                    _ => Production::NullishAssign,
                });
                let target = if self.chance(1, 2) {
                    Expr::Ident(self.name())
                } else {
                    Expr::Member(Box::new(self.chain_base(ctx)), self.name(), false)
                };
                Expr::Assign(Box::new(target), op, Box::new(self.expr(deep)))
            }
            3 if ctx.has(IS_ASYNC) || !ctx.has(IN_FUNCTION) => {
                self.note(if ctx.has(IN_FUNCTION) {
                    Production::Await
                } else {
                    Production::TopLevelAwait
                });
                Expr::Unary("await", Box::new(self.chain(ctx)))
            }
            3 if ctx.has(IS_GENERATOR) => self.yield_expr(ctx),
            _ => {
                self.note(Production::New);
                Expr::New(self.type_name(), self.args(deep))
            }
        }
    }

    fn yield_expr(&mut self, ctx: Ctx) -> Expr {
        match self.rng.below(3) {
            0 => {
                self.note(Production::Yield);
                Expr::Yield(None, false)
            }
            1 => {
                self.note(Production::YieldValue);
                Expr::Yield(Some(Box::new(self.expr(ctx.deeper()))), false)
            }
            _ => {
                self.note(Production::YieldDelegate);
                Expr::Yield(Some(Box::new(self.expr(ctx.deeper()))), true)
            }
        }
    }

    /// `name`, `this` (in a class) or `super.name` (in a class): the start of a chain.
    fn chain_base(&mut self, ctx: Ctx) -> Expr {
        if ctx.has(IN_CLASS) && !ctx.has(IN_DERIVED_CONSTRUCTOR) && self.chance(1, 4) {
            if self.chance(1, 2) {
                self.note(Production::This);
                return Expr::This;
            }
            self.note(Production::SuperMember);
            return Expr::SuperMember(self.name());
        }
        self.note(Production::Ident);
        Expr::Ident(self.name())
    }

    /// A member/call chain on a base.
    fn chain(&mut self, ctx: Ctx) -> Expr {
        let mut current = self.chain_base(ctx);
        let links = self.rng.index(3);
        for _ in 0..links {
            current = match self.rng.below(6) {
                0 | 1 => {
                    self.note(Production::Member);
                    Expr::Member(Box::new(current), self.name(), false)
                }
                2 => {
                    self.note(Production::OptionalMember);
                    Expr::Member(Box::new(current), self.name(), true)
                }
                3 => {
                    self.note(Production::Index);
                    Expr::Index(Box::new(current), Box::new(self.simple_expr()), false)
                }
                4 => {
                    self.note(Production::OptionalIndex);
                    Expr::Index(Box::new(current), Box::new(self.simple_expr()), true)
                }
                _ => {
                    let optional = self.chance(1, 3);
                    self.note(if optional {
                        Production::OptionalCall
                    } else {
                        Production::Call
                    });
                    Expr::Call(Box::new(current), self.args(ctx.deeper()), optional)
                }
            };
        }
        current
    }

    fn args(&mut self, ctx: Ctx) -> Vec<Expr> {
        let count = self.rng.index(3);
        (0..count).map(|_| self.expr(ctx)).collect()
    }

    fn literal(&mut self) -> Literal {
        match self.rng.below(6) {
            0 | 1 => {
                if self.options.doubles && self.chance(1, 2) {
                    self.note(Production::Double);
                    Literal::Number(self.pick(&["0.5", "2.25", "1e-3", "3.75"]).to_owned())
                } else {
                    self.note(Production::Number);
                    Literal::Number(
                        self.pick(&[
                            "0",
                            "1",
                            "2",
                            "7",
                            "42",
                            "0x1F",
                            "0o17",
                            "0b101",
                            "1_000",
                            "2147483647",
                        ])
                        .to_owned(),
                    )
                }
            }
            2 => {
                self.note(Production::BigInt);
                Literal::BigInt(self.pick(&["0n", "1n", "9007199254740993n"]).to_owned())
            }
            3 => {
                self.note(Production::Str);
                Literal::Str(self.pick(WORDS).to_owned())
            }
            4 => {
                self.note(Production::Bool);
                Literal::Bool(self.chance(1, 2))
            }
            _ => {
                self.note(Production::Undefined);
                Literal::Undefined
            }
        }
    }

    /// A `case` label: a literal other than `undefined` (a name, not a `Literal`) that
    /// no earlier label of the same `switch` used, since a repeated label is E000. The
    /// pool is far larger than the six labels a generated `switch` holds.
    fn case_label(&mut self, taken: &mut Vec<Literal>) -> Literal {
        loop {
            let label = self.literal();
            if label != Literal::Undefined && !taken.contains(&label) {
                taken.push(label.clone());
                return label;
            }
        }
    }

    /// A literal or a name: used where the grammar wants something small.
    fn simple_expr(&mut self) -> Expr {
        if self.chance(1, 3) {
            self.note(Production::Ident);
            Expr::Ident(self.name())
        } else {
            Expr::Literal(self.literal())
        }
    }

    fn expr(&mut self, ctx: Ctx) -> Expr {
        if ctx.depth >= self.options.max_depth {
            return self.simple_expr();
        }
        let deep = ctx.deeper();
        match self.rng.below(16) {
            0 | 1 => self.simple_expr(),
            2 => self.chain(ctx),
            3 => self.binary(deep),
            4 => self.unary(ctx),
            5 => {
                self.note(Production::Ternary);
                Expr::Ternary(
                    Box::new(self.expr(deep)),
                    Box::new(self.expr(deep)),
                    Box::new(self.expr(deep)),
                )
            }
            6 => self.array(deep),
            7 => self.object(deep),
            8 => self.arrow(ctx),
            9 => self.template(deep),
            10 => {
                self.note(Production::Regex);
                let flags: String = ['d', 'g', 'i', 'm', 's', 'y']
                    .into_iter()
                    .filter(|_| self.chance(1, 4))
                    .chain(std::iter::once('v'))
                    .collect();
                Expr::Regex(
                    self.pick(&["a+b", "[a-z]+", "\\d{2,}", "^x$"]).to_owned(),
                    flags,
                )
            }
            11 => {
                if self.chance(1, 2) {
                    self.note(Production::New);
                    Expr::New(self.type_name(), self.args(deep))
                } else {
                    self.note(Production::NewQualified);
                    let ns = self.name();
                    Expr::New(format!("{ns}.{}", self.type_name()), self.args(deep))
                }
            }
            12 => {
                self.note(Production::DynamicImport);
                Expr::DynamicImport(self.module_path())
            }
            13 => {
                self.note(Production::Paren);
                Expr::Paren(Box::new(self.expr(deep)))
            }
            14 => {
                self.note(Production::NullComparison);
                let op = if self.chance(1, 2) { "===" } else { "!==" };
                Expr::Binary(
                    Box::new(self.chain(ctx)),
                    op,
                    Box::new(Expr::Ident("null".to_owned())),
                )
            }
            _ if ctx.has(IS_GENERATOR) => self.yield_expr(ctx),
            _ => self.simple_expr(),
        }
    }

    fn binary(&mut self, ctx: Ctx) -> Expr {
        const OPS: &[(&str, Production)] = &[
            ("??", Production::Nullish),
            ("??", Production::NullishMixGuarded),
            ("??", Production::Nullish),
            ("||", Production::Or),
            ("&&", Production::And),
            ("|", Production::BitOr),
            ("^", Production::BitXor),
            ("&", Production::BitAnd),
            ("===", Production::Equal),
            ("!==", Production::NotEqual),
            ("<", Production::Less),
            (">", Production::Greater),
            ("<=", Production::LessEq),
            (">=", Production::GreaterEq),
            ("instanceof", Production::InstanceOf),
            ("<<", Production::ShiftLeft),
            (">>", Production::ShiftRight),
            (">>>", Production::ShiftRightUnsigned),
            ("+", Production::Add),
            ("-", Production::Sub),
            ("*", Production::Mul),
            ("/", Production::Div),
            ("%", Production::Rem),
            ("**", Production::Exp),
        ];
        let (op, production) = self.pick(OPS);
        self.note(production);
        let left = match production {
            Production::NullishMixGuarded => self.logical_pair(),
            _ => self.expr(ctx),
        };
        let right = if op == "instanceof" {
            Expr::Ident(self.type_name())
        } else {
            self.expr(ctx)
        };
        Expr::Binary(Box::new(left), op, Box::new(right))
    }

    /// `a || b` or `a && b` over simple operands: the left side that makes `??` need its
    /// guard parentheses.
    fn logical_pair(&mut self) -> Expr {
        let inner = if self.chance(1, 2) { "||" } else { "&&" };
        Expr::Binary(
            Box::new(self.simple_expr()),
            inner,
            Box::new(self.simple_expr()),
        )
    }

    fn unary(&mut self, ctx: Ctx) -> Expr {
        let deep = ctx.deeper();
        let can_await = ctx.has(IS_ASYNC) || !ctx.has(IN_FUNCTION);
        let (op, production) = match self.rng.below(if can_await { 6 } else { 5 }) {
            0 => ("!", Production::Not),
            1 => ("-", Production::Negate),
            2 => ("+", Production::Plus),
            3 => ("~", Production::BitNot),
            4 => ("typeof", Production::TypeOf),
            _ => (
                "await",
                if ctx.has(IN_FUNCTION) {
                    Production::Await
                } else {
                    Production::TopLevelAwait
                },
            ),
        };
        self.note(production);
        Expr::Unary(op, Box::new(self.expr(deep)))
    }

    fn array(&mut self, ctx: Ctx) -> Expr {
        self.note(Production::Array);
        let count = self.rng.index(4);
        let items: Vec<Expr> = (0..count).map(|_| self.expr(ctx)).collect();
        let spread: Vec<bool> = items
            .iter()
            .map(|_| {
                let spread = self.chance(1, 4);
                if spread {
                    self.note(Production::ArraySpread);
                }
                spread
            })
            .collect();
        Expr::Array(items, spread)
    }

    fn object(&mut self, ctx: Ctx) -> Expr {
        self.note(Production::Object);
        let count = self.rng.index(4);
        let mut keys = BTreeSet::new();
        let mut properties = Vec::new();
        for _ in 0..count {
            let name = self.name();
            let property = match self.rng.below(5) {
                0 if keys.insert(name.clone()) => {
                    self.note(Production::ObjectShorthand);
                    Property::Shorthand(name)
                }
                // `"a"` and `a` spell the same key (E134 if repeated).
                1 if keys.insert(name.clone()) => {
                    self.note(Production::ObjectStringKey);
                    Property::StringKey(name, self.expr(ctx))
                }
                2 => {
                    self.note(Production::ObjectComputed);
                    let key = Expr::Literal(Literal::Str(format!("key-{}", self.rng.below(100))));
                    Property::Computed(key, self.expr(ctx))
                }
                3 => {
                    self.note(Production::ObjectSpread);
                    Property::Spread(Expr::Ident(self.name()))
                }
                _ if keys.insert(name.clone()) => {
                    self.note(Production::ObjectNamed);
                    Property::Named(name, self.expr(ctx))
                }
                _ => continue,
            };
            properties.push(property);
        }
        Expr::Object(properties)
    }

    fn arrow(&mut self, ctx: Ctx) -> Expr {
        let is_async = self.chance(1, 3);
        self.note(if is_async {
            Production::AsyncArrow
        } else {
            Production::Arrow
        });
        let inner = arrow_ctx(ctx, is_async);
        let params = self.params(inner);
        let body = match self.rng.below(3) {
            0 => {
                self.note(Production::ArrowBlock);
                Err(self.block(inner))
            }
            1 => {
                self.note(Production::ArrowObjectBody);
                Ok(Box::new(self.object(inner)))
            }
            _ => Ok(Box::new(self.expr(inner))),
        };
        Expr::Arrow {
            is_async,
            params,
            body,
        }
    }

    fn template(&mut self, ctx: Ctx) -> Expr {
        let raw = self.chance(1, 4);
        self.note(if raw {
            Production::StringRaw
        } else {
            Production::Template
        });
        let count = 1 + self.rng.index(3);
        let parts = (0..count)
            .map(|_| {
                if self.chance(1, 2) {
                    self.note(Production::TemplateSubstitution);
                    TemplatePart::Substitution(self.chain(ctx))
                } else {
                    TemplatePart::Text(
                        self.pick(&[
                            "plain text ",
                            "with \"quotes\" ",
                            "and 'single' ",
                            "n=",
                            " done",
                        ])
                        .to_owned(),
                    )
                }
            })
            .collect();
        if raw {
            Expr::StringRaw(parts)
        } else {
            Expr::Template(parts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::Expr;
    use super::{
        arrow_ctx, flags_of, function_ctx, member_ctx, method_ctx, Ctx, Generator, Options,
        Production, IN_CLASS, IN_DERIVED_CONSTRUCTOR, IN_FUNCTION, IN_LOOP, IN_STATIC_BLOCK,
        IN_SWITCH, IS_ASYNC, IS_GENERATOR,
    };
    use crate::prng::Rng;
    use std::collections::BTreeSet;

    #[test]
    fn flags_add_each_bit_once() {
        assert_eq!(flags_of(&[]), 0);
        assert_eq!(flags_of(&[(true, IN_LOOP)]), IN_LOOP);
        assert_eq!(flags_of(&[(false, IN_LOOP)]), 0);
        assert_eq!(
            flags_of(&[(true, IN_FUNCTION), (true, IS_ASYNC), (false, IS_GENERATOR)]),
            3
        );
        let all = [
            IN_FUNCTION,
            IS_ASYNC,
            IS_GENERATOR,
            IN_LOOP,
            IN_SWITCH,
            IN_CLASS,
            IN_DERIVED_CONSTRUCTOR,
        ];
        assert_eq!(all.iter().copied().sum::<u8>(), 127, "seven distinct bits");
    }

    #[test]
    fn contexts_carry_exactly_their_flags() {
        for (is_async, is_generator) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let async_bit = if is_async { IS_ASYNC } else { 0 };
            let generator_bit = if is_generator { IS_GENERATOR } else { 0 };
            let ctx = function_ctx(3, is_async, is_generator);
            assert_eq!(
                (ctx.depth, ctx.flags),
                (3, IN_FUNCTION + async_bit + generator_bit)
            );
            let class = member_ctx(2).with(IN_DERIVED_CONSTRUCTOR);
            let method = method_ctx(class.with(IS_ASYNC), is_async, is_generator);
            assert_eq!(method.depth, 2);
            assert_eq!(
                method.flags,
                IN_FUNCTION + IN_CLASS + IN_DERIVED_CONSTRUCTOR + async_bit + generator_bit,
                "a method takes its own async/generator flags, not the class's"
            );
            let arrow = arrow_ctx(
                Ctx {
                    flags: IN_LOOP + IN_SWITCH + IN_CLASS,
                    depth: 5,
                },
                is_async,
            );
            assert_eq!(
                (arrow.depth, arrow.flags),
                (6, IN_FUNCTION + IN_CLASS + async_bit),
                "an arrow leaves loops and switches"
            );
        }
        assert_eq!(member_ctx(1).flags, IN_FUNCTION + IN_CLASS);
        assert_eq!(
            member_ctx(1).with(IN_STATIC_BLOCK).flags,
            IN_FUNCTION + IN_CLASS + IN_STATIC_BLOCK
        );
        assert_eq!(Ctx::default().deeper().depth, 1);
    }

    #[test]
    fn the_guarded_nullish_mix_is_what_its_production_says() {
        let mut rng = Rng::from_seed(3);
        let mut g = Generator {
            rng: &mut rng,
            options: Options::default(),
            used: BTreeSet::new(),
            exported: BTreeSet::new(),
        };
        for _ in 0..50 {
            let pair = g.logical_pair();
            assert!(matches!(pair, Expr::Binary(_, "||" | "&&", _)), "{pair:?}");
        }
        let mut guarded = 0;
        let mut plain = 0;
        for _ in 0..400 {
            g.used.clear();
            let expr = g.binary(Ctx {
                flags: 0,
                depth: Options::default().max_depth,
            });
            if let Expr::Binary(left, "??", _) = &expr {
                if matches!(**left, Expr::Binary(_, "||" | "&&", _)) {
                    guarded += 1;
                    assert!(g.used.contains(&Production::NullishMixGuarded), "{expr:?}");
                } else {
                    plain += 1;
                    assert!(!g.used.contains(&Production::NullishMixGuarded), "{expr:?}");
                }
            }
        }
        assert!(guarded > 5 && plain > 5, "{guarded} guarded, {plain} plain");
    }
}
