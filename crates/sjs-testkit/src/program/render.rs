//! Rendering the tree as text in the canonical layout: two-space indent, no semicolons,
//! and parentheses exactly where ES2025 precedence needs them.

use std::fmt;
use std::fmt::Write as _;

/// Appends formatted text to a `String`, whose `Write` never fails.
macro_rules! push {
    ($out:expr, $($arg:tt)*) => {{
        let _ = write!($out, $($arg)*);
    }};
}

use super::tree::{
    Decl, Else, Expr, ForUpdate, Import, Item, Literal, Member, MemberKey, Param, Pattern, Program,
    Property, Stmt, TemplatePart,
};

impl fmt::Display for Program {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        for (index, item) in self.items.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            render_item(&mut out, item, 0);
        }
        f.write_str(&out)
    }
}

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

fn render_item(out: &mut String, item: &Item, level: usize) {
    match item {
        Item::Import(import) => {
            indent(out, level);
            render_import(out, import);
            out.push('\n');
        }
        Item::Export(decl) => {
            indent(out, level);
            out.push_str("export ");
            render_decl(out, decl, level);
        }
        Item::ExportList(names, from) => {
            indent(out, level);
            out.push_str("export { ");
            out.push_str(&aliases(names));
            out.push_str(" }");
            if let Some(path) = from {
                push!(out, " from {}", string(path));
            }
            out.push('\n');
        }
        Item::Decl(decl) => {
            indent(out, level);
            render_decl(out, decl, level);
        }
        Item::Stmt(stmt) => render_stmt(out, stmt, level),
    }
}

fn aliases(names: &[(String, Option<String>)]) -> String {
    names
        .iter()
        .map(|(name, alias)| {
            alias
                .as_ref()
                .map_or_else(|| name.clone(), |alias| format!("{name} as {alias}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_import(out: &mut String, import: &Import) {
    match import {
        Import::Named(names, path) => {
            push!(out, "import {{ {} }} from {}", aliases(names), string(path));
        }
        Import::Namespace(name, path) => {
            push!(out, "import * as {name} from {}", string(path));
        }
        Import::Json(name, path) => {
            push!(
                out,
                "import {name} from {} with {{ type: \"json\" }}",
                string(path)
            );
        }
    }
}

/// Renders a declaration; the caller has written the indentation. Ends with a newline.
fn render_decl(out: &mut String, decl: &Decl, level: usize) {
    match decl {
        Decl::Const(pattern, value) => {
            push!(
                out,
                "const {} = {}\n",
                pattern_text(pattern, level),
                expr(value, 0, level)
            );
        }
        Decl::Let(pattern, value) => {
            push!(
                out,
                "let {} = {}\n",
                pattern_text(pattern, level),
                expr(value, 0, level)
            );
        }
        Decl::Function {
            is_async,
            is_generator,
            name,
            params,
            body,
        } => {
            if *is_async {
                out.push_str("async ");
            }
            out.push_str("function");
            if *is_generator {
                out.push('*');
            }
            push!(out, " {name}({})", params_text(params, level));
            render_body(out, body, level);
        }
        Decl::Class {
            name,
            extends,
            members,
        } => {
            push!(out, "class {name}");
            if let Some(base) = extends {
                push!(out, " extends {base}");
            }
            out.push_str(" {\n");
            for member in members {
                indent(out, level + 1);
                render_member(out, member, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
    }
}

/// ` { … }` after a signature, then a newline.
fn render_body(out: &mut String, body: &[Stmt], level: usize) {
    out.push_str(" {\n");
    for stmt in body {
        render_stmt(out, stmt, level + 1);
    }
    indent(out, level);
    out.push_str("}\n");
}

fn render_member(out: &mut String, member: &Member, level: usize) {
    match member {
        Member::Field {
            is_static,
            key,
            value,
        } => {
            if *is_static {
                out.push_str("static ");
            }
            out.push_str(&member_key(key));
            if let Some(value) = value {
                push!(out, " = {}", expr(value, 0, level));
            }
            out.push('\n');
        }
        Member::Method {
            is_static,
            is_async,
            is_generator,
            key,
            params,
            body,
        } => {
            if *is_static {
                out.push_str("static ");
            }
            if *is_async {
                out.push_str("async ");
            }
            if *is_generator {
                out.push('*');
            }
            push!(out, "{}({})", member_key(key), params_text(params, level));
            render_body(out, body, level);
        }
        Member::Constructor(params, body) => {
            push!(out, "constructor({})", params_text(params, level));
            render_body(out, body, level);
        }
        Member::Getter(is_static, key, body) => {
            if *is_static {
                out.push_str("static ");
            }
            push!(out, "get {}()", member_key(key));
            render_body(out, body, level);
        }
        Member::Setter(is_static, key, param, body) => {
            if *is_static {
                out.push_str("static ");
            }
            push!(out, "set {}({})", member_key(key), param_text(param, level));
            render_body(out, body, level);
        }
        Member::StaticBlock(body) => {
            out.push_str("static");
            render_body(out, body, level);
        }
    }
}

fn member_key(key: &MemberKey) -> String {
    match key {
        MemberKey::Ident(name) => name.clone(),
        MemberKey::Private(name) => format!("#{name}"),
        MemberKey::Symbol(name) => format!("[Symbol.{name}]"),
    }
}

fn params_text(params: &[Param], level: usize) -> String {
    params
        .iter()
        .map(|p| param_text(p, level))
        .collect::<Vec<_>>()
        .join(", ")
}

fn param_text(param: &Param, level: usize) -> String {
    match param {
        Param::Plain(pattern, None) => pattern_text(pattern, level),
        Param::Plain(pattern, Some(default)) => format!(
            "{} = {}",
            pattern_text(pattern, level),
            expr(default, 0, level)
        ),
        Param::Rest(name) => format!("...{name}"),
    }
}

fn pattern_text(pattern: &Pattern, level: usize) -> String {
    match pattern {
        Pattern::Ident(name) => name.clone(),
        Pattern::Object(entries, rest) => {
            let mut parts: Vec<String> = entries
                .iter()
                .map(|(key, target, default)| {
                    let mut text = key.clone();
                    if let Some(target) = target {
                        push!(text, ": {}", pattern_text(target, level));
                    }
                    if let Some(default) = default {
                        push!(text, " = {}", expr(default, 0, level));
                    }
                    text
                })
                .collect();
            if let Some(rest) = rest {
                parts.push(format!("...{rest}"));
            }
            format!("{{ {} }}", parts.join(", "))
        }
        Pattern::Array(entries, rest) => {
            let mut parts: Vec<String> = entries
                .iter()
                .map(|(target, default)| match default {
                    Some(default) => format!(
                        "{} = {}",
                        pattern_text(target, level),
                        expr(default, 0, level)
                    ),
                    None => pattern_text(target, level),
                })
                .collect();
            if let Some(rest) = rest {
                parts.push(format!("...{rest}"));
            }
            format!("[{}]", parts.join(", "))
        }
    }
}

fn render_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match stmt {
        Stmt::Block(body) => {
            out.push_str("{\n");
            for inner in body {
                render_stmt(out, inner, level + 1);
            }
            indent(out, level);
            out.push_str("}\n");
        }
        Stmt::If(cond, then, otherwise) => {
            push!(out, "if ({})", expr(cond, 0, level));
            render_body_with_else(out, then, otherwise.as_deref(), level);
        }
        Stmt::ForOf {
            is_await,
            is_let,
            pattern,
            iterable,
            body,
        } => {
            out.push_str("for ");
            if *is_await {
                out.push_str("await ");
            }
            let kind = if *is_let { "let" } else { "const" };
            push!(
                out,
                "({kind} {} of {})",
                pattern_text(pattern, level),
                expr(iterable, 0, level)
            );
            render_body(out, body, level);
        }
        Stmt::For {
            var,
            init,
            test,
            update,
            body,
        } => {
            let update = match update {
                ForUpdate::AddAssign(step) => format!("{var} += {}", expr(step, 2, level)),
                ForUpdate::SubAssign(step) => format!("{var} -= {}", expr(step, 2, level)),
                ForUpdate::PostInc => format!("{var}++"),
                ForUpdate::PostDec => format!("{var}--"),
                ForUpdate::PreInc => format!("++{var}"),
                ForUpdate::PreDec => format!("--{var}"),
            };
            push!(
                out,
                "for (let {var} = {}; {}; {update})",
                expr(init, 0, level),
                expr(test, 0, level)
            );
            render_body(out, body, level);
        }
        Stmt::While(cond, body) => {
            push!(out, "while ({})", expr(cond, 0, level));
            render_body(out, body, level);
        }
        Stmt::Switch(scrutinee, cases, default) => {
            render_switch(out, scrutinee, cases, default.as_deref(), level);
        }
        Stmt::Try(body, catch, finally) => {
            render_try(out, body, catch.as_ref(), finally.as_deref(), level);
        }
        Stmt::Return(None) => out.push_str("return\n"),
        Stmt::Return(Some(value)) => push!(out, "return {}\n", expr(value, 0, level)),
        Stmt::Break => out.push_str("break\n"),
        Stmt::Continue => out.push_str("continue\n"),
        Stmt::Throw(value) => push!(out, "throw {}\n", expr(value, 0, level)),
        Stmt::Expr(value) => push!(out, "{}\n", expr(value, 0, level)),
        Stmt::Decl(decl) => render_decl(out, decl, level),
    }
}

fn render_switch(
    out: &mut String,
    scrutinee: &Expr,
    cases: &[(Vec<Literal>, Vec<Stmt>)],
    default: Option<&[Stmt]>,
    level: usize,
) {
    push!(out, "switch ({}) {{\n", expr(scrutinee, 0, level));
    for (labels, body) in cases {
        for label in labels {
            indent(out, level + 1);
            push!(out, "case {}:\n", literal(label));
        }
        render_case_body(out, body, level + 1);
    }
    if let Some(body) = default {
        indent(out, level + 1);
        out.push_str("default:\n");
        render_case_body(out, body, level + 1);
    }
    indent(out, level);
    out.push_str("}\n");
}

fn render_try(
    out: &mut String,
    body: &[Stmt],
    catch: Option<&(Option<String>, Vec<Stmt>)>,
    finally: Option<&[Stmt]>,
    level: usize,
) {
    out.push_str("try");
    render_body(out, body, level);
    if let Some((binding, body)) = catch {
        indent(out, level);
        out.push_str("catch");
        if let Some(name) = binding {
            push!(out, " ({name})");
        }
        render_body(out, body, level);
    }
    if let Some(body) = finally {
        indent(out, level);
        out.push_str("finally");
        render_body(out, body, level);
    }
}

fn render_case_body(out: &mut String, body: &[Stmt], level: usize) {
    indent(out, level);
    out.push_str("{\n");
    for stmt in body {
        render_stmt(out, stmt, level + 1);
    }
    indent(out, level);
    out.push_str("}\n");
}

fn render_body_with_else(out: &mut String, then: &[Stmt], otherwise: Option<&Else>, level: usize) {
    out.push_str(" {\n");
    for stmt in then {
        render_stmt(out, stmt, level + 1);
    }
    indent(out, level);
    out.push('}');
    match otherwise {
        None => out.push('\n'),
        Some(Else::Block(body)) => {
            out.push_str(" else");
            render_body(out, body, level);
        }
        Some(Else::If(cond, then, otherwise)) => {
            push!(out, " else if ({})", expr(cond, 0, level));
            render_body_with_else(out, then, otherwise.as_deref(), level);
        }
    }
}

/// Precedence levels: a child is parenthesised when its level is below what its position
/// requires. Assignment and arrows are the loosest; primaries the tightest.
const ASSIGN: u8 = 1;
const TERNARY: u8 = 2;
const NULLISH: u8 = 3;
const OR: u8 = 4;
const AND: u8 = 5;
const BIT_OR: u8 = 6;
const BIT_XOR: u8 = 7;
const BIT_AND: u8 = 8;
const EQUALITY: u8 = 9;
const RELATIONAL: u8 = 10;
const SHIFT: u8 = 11;
const ADDITIVE: u8 = 12;
const MULTIPLICATIVE: u8 = 13;
const EXPONENT: u8 = 14;
const UNARY: u8 = 15;
/// Postfix and primary expressions bind the same way as far as parentheses go.
const POSTFIX: u8 = 16;

fn binary_level(op: &str) -> u8 {
    match op {
        "??" => NULLISH,
        "||" => OR,
        "&&" => AND,
        "|" => BIT_OR,
        "^" => BIT_XOR,
        "&" => BIT_AND,
        "===" | "!==" => EQUALITY,
        "<" | ">" | "<=" | ">=" | "instanceof" => RELATIONAL,
        "<<" | ">>" | ">>>" => SHIFT,
        "+" | "-" => ADDITIVE,
        "*" | "/" | "%" => MULTIPLICATIVE,
        _ => EXPONENT,
    }
}

fn level_of(e: &Expr) -> u8 {
    match e {
        Expr::Assign(..) | Expr::Arrow { .. } | Expr::Yield(..) => ASSIGN,
        Expr::Ternary(..) => TERNARY,
        Expr::Binary(_, op, _) => binary_level(op),
        Expr::Unary(..) => UNARY,
        _ => POSTFIX,
    }
}

/// Renders `e` in a position that requires at least level `min`.
pub(super) fn expr(e: &Expr, min: u8, level: usize) -> String {
    let text = render_expr(e, level);
    if level_of(e) < min {
        format!("({text})")
    } else {
        text
    }
}

fn render_expr(e: &Expr, level: usize) -> String {
    match e {
        Expr::Literal(value) => literal(value),
        Expr::Ident(name) => name.clone(),
        Expr::This => "this".to_owned(),
        Expr::Array(items, spread) => {
            let parts: Vec<String> = items
                .iter()
                .zip(spread)
                .map(|(item, spread)| {
                    format!(
                        "{}{}",
                        if *spread { "..." } else { "" },
                        expr(item, ASSIGN, level)
                    )
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::Object(properties) => {
            if properties.is_empty() {
                return "{}".to_owned();
            }
            let parts: Vec<String> = properties.iter().map(|p| property(p, level)).collect();
            format!("{{ {} }}", parts.join(", "))
        }
        Expr::Arrow {
            is_async,
            params,
            body,
        } => render_arrow(*is_async, params, body, level),
        Expr::Template(parts) => format!("`{}`", template(parts, level)),
        Expr::StringRaw(parts) => format!("String.raw`{}`", template(parts, level)),
        Expr::Regex(body, flags) => format!("/{body}/{flags}"),
        Expr::New(name, args) => format!("new {name}({})", args_text(args, level)),
        Expr::DynamicImport(path) => format!("import({})", string(path)),
        Expr::Paren(inner) => format!("({})", render_expr(inner, level)),
        Expr::Member(object, name, optional) => {
            format!(
                "{}{}{name}",
                expr(object, POSTFIX, level),
                if *optional { "?." } else { "." }
            )
        }
        Expr::Index(object, index, optional) => format!(
            "{}{}[{}]",
            expr(object, POSTFIX, level),
            if *optional { "?." } else { "" },
            expr(index, 0, level)
        ),
        Expr::Call(callee, args, optional) => format!(
            "{}{}({})",
            expr(callee, POSTFIX, level),
            if *optional { "?." } else { "" },
            args_text(args, level)
        ),
        Expr::SuperMember(name) => format!("super.{name}"),
        Expr::SuperCall(args) => format!("super({})", args_text(args, level)),
        Expr::Unary(op, operand) => {
            let operand = expr(operand, UNARY, level);
            // A word needs a space; so does `-` before `-x` (or `+` before `+x`), which
            // would otherwise lex as `--` (or `++`).
            let doubled = matches!(*op, "-" | "+") && operand.starts_with(*op);
            let space = if op.chars().all(char::is_alphabetic) || doubled {
                " "
            } else {
                ""
            };
            format!("{op}{space}{operand}")
        }
        Expr::Binary(left, op, right) => render_binary(left, op, right, level),
        Expr::Ternary(cond, then, otherwise) => format!(
            "{} ? {} : {}",
            expr(cond, NULLISH, level),
            expr(then, ASSIGN, level),
            expr(otherwise, ASSIGN, level)
        ),
        Expr::Assign(target, op, value) => format!(
            "{} {op} {}",
            render_expr(target, level),
            expr(value, ASSIGN, level)
        ),
        Expr::Yield(None, _) => "yield".to_owned(),
        Expr::Yield(Some(value), delegate) => {
            format!(
                "yield{} {}",
                if *delegate { "*" } else { "" },
                expr(value, ASSIGN, level)
            )
        }
    }
}

fn render_binary(left: &Expr, op: &str, right: &Expr, level: usize) -> String {
    let own = binary_level(op);
    let (left_min, right_min) = if own == EXPONENT {
        // `**` is right-associative and refuses a bare unary operand on its left.
        (POSTFIX, EXPONENT)
    } else {
        (own, own + 1)
    };
    let mut left_text = expr(left, left_min, level);
    let mut right_text = expr(right, right_min, level);
    // `??` may not mix with `||`/`&&` unparenthesised, in either direction.
    if own == NULLISH {
        left_text = guard_logical(left, left_text);
        right_text = guard_logical(right, right_text);
    }
    format!("{left_text} {op} {right_text}")
}

fn render_arrow(
    is_async: bool,
    params: &[Param],
    body: &Result<Box<Expr>, Vec<Stmt>>,
    level: usize,
) -> String {
    let head = format!(
        "{}({}) =>",
        if is_async { "async " } else { "" },
        params_text(params, level)
    );
    match body {
        Ok(value) => {
            // ES reads a body whose first token is `{` as a block, so one that begins
            // with an object literal, however deep on its left, is parenthesised.
            if starts_with_brace(value) {
                format!("{head} ({})", render_expr(value, level))
            } else {
                format!("{head} {}", expr(value, ASSIGN, level))
            }
        }
        Err(stmts) => {
            let mut out = head;
            render_body(&mut out, stmts, level);
            out.pop();
            out
        }
    }
}

/// Whether the first token of `e`, as rendered, is `{`: an object literal, or an
/// expression whose leftmost operand is one.
fn starts_with_brace(e: &Expr) -> bool {
    match e {
        Expr::Object(_) => true,
        Expr::Ternary(test, _, _) => starts_with_brace(test),
        Expr::Binary(left, _, _) | Expr::Assign(left, _, _) => starts_with_brace(left),
        Expr::Member(base, _, _) | Expr::Index(base, _, _) | Expr::Call(base, _, _) => {
            starts_with_brace(base)
        }
        _ => false,
    }
}

fn guard_logical(child: &Expr, text: String) -> String {
    if matches!(level_of(child), OR | AND) {
        format!("({text})")
    } else {
        text
    }
}

fn args_text(args: &[Expr], level: usize) -> String {
    args.iter()
        .map(|a| expr(a, ASSIGN, level))
        .collect::<Vec<_>>()
        .join(", ")
}

fn property(p: &Property, level: usize) -> String {
    match p {
        Property::Shorthand(name) => name.clone(),
        Property::Named(name, value) => format!("{name}: {}", expr(value, ASSIGN, level)),
        Property::StringKey(key, value) => {
            format!("{}: {}", string(key), expr(value, ASSIGN, level))
        }
        Property::Computed(key, value) => format!(
            "[{}]: {}",
            expr(key, ASSIGN, level),
            expr(value, ASSIGN, level)
        ),
        Property::Spread(value) => format!("...{}", expr(value, ASSIGN, level)),
    }
}

fn template(parts: &[TemplatePart], level: usize) -> String {
    parts
        .iter()
        .map(|part| match part {
            TemplatePart::Text(text) => text.clone(),
            TemplatePart::Substitution(value) => format!("${{{}}}", expr(value, 0, level)),
        })
        .collect()
}

fn literal(value: &Literal) -> String {
    match value {
        Literal::Number(text) | Literal::BigInt(text) => text.clone(),
        Literal::Str(text) => string(text),
        Literal::Bool(true) => "true".to_owned(),
        Literal::Bool(false) => "false".to_owned(),
        Literal::Undefined => "undefined".to_owned(),
    }
}

/// A double-quoted string with the escapes the subset needs.
pub(super) fn string(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}
