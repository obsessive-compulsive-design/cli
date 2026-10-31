//! Shared by the formatter's integration tests: format a file's expression statements,
//! and compare parse trees modulo trivia and redundant parentheses (US-050's `strip`).

#![allow(dead_code)]

use sjs_diag::{Code, SourceFile};
use sjs_format::{print, Formatter};
use sjs_syntax::{parse, Element, NodeKind, NodeRef};

pub fn file(text: &str) -> SourceFile {
    SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8")
}

/// Every expression statement of `text`, formatted, one per line.
pub fn formatted(text: &str) -> String {
    let f = file(text);
    let (tree, _) = parse(&f);
    let formatter = Formatter::new(&f, &tree);
    let mut lines = Vec::new();
    for item in tree.root().children() {
        if let Element::Node(statement) = item {
            if statement.kind() == NodeKind::ExprStmt {
                let expression = statement.children().next().expect("an expression");
                lines.push(print(&[formatter.element(expression)]));
            }
        }
    }
    lines.join("\n")
}

/// `text` without its first (outermost, leftmost) pair of parentheses around an
/// expression; `None` when it has none.
pub fn without_first_pair(text: &str) -> Option<String> {
    let f = file(text);
    let (tree, _) = parse(&f);
    let span = first_paren(tree.root())?;
    let mut out = String::new();
    out.push_str(&text[..span.byte_start]);
    out.push_str(&text[span.byte_start + 1..span.byte_end - 1]);
    out.push_str(&text[span.byte_end..]);
    Some(out)
}

fn first_paren(node: NodeRef<'_>) -> Option<sjs_diag::Span> {
    if node.kind() == NodeKind::ParenExpr {
        return Some(node.span());
    }
    node.children().find_map(|child| match child {
        Element::Node(child) => first_paren(child),
        Element::Token(_) => None,
    })
}

/// The whole file `text`, formatted (US-042).
pub fn module(text: &str) -> String {
    let f = file(text);
    let (tree, _) = parse(&f);
    Formatter::new(&f, &tree).module()
}

/// The whole file `text`, formatted at `width` columns (US-043).
pub fn module_at(text: &str, width: usize) -> String {
    let f = file(text);
    let (tree, _) = parse(&f);
    Formatter::new(&f, &tree).module_at(width)
}

/// The tree of `text` with redundant parentheses removed, as an s-expression, or
/// `None` when it does not parse clean.
pub fn stripped(text: &str) -> Option<String> {
    let f = file(text);
    let (tree, diagnostics) = parse(&f);
    if diagnostics.iter().any(|d| d.code == Code::E000) {
        return None;
    }
    Some(strip(&tree.to_sexp(&f)))
}

/// [`stripped`], and also without what the layout itself may change (US-046's corpus):
/// a trailing comma before a closing bracket, and an arrow's block body that is a
/// single `return` rendered as the value.
pub fn canonical(text: &str) -> Option<String> {
    let f = file(text);
    let (tree, diagnostics) = parse(&f);
    if diagnostics.iter().any(|d| d.code == Code::E000) {
        return None;
    }
    let chars: Vec<char> = tree.to_sexp(&f).chars().collect();
    let mut pos = 0;
    let item = parse_item(&chars, &mut pos);
    Some(render_with(&item, true))
}

enum Item {
    Node(String, Vec<Item>),
    Token(String),
}

/// `sexp` with every `(ParenExpr ( inner ))` rewritten as `inner`.
fn strip(sexp: &str) -> String {
    let chars: Vec<char> = sexp.chars().collect();
    let mut pos = 0;
    let item = parse_item(&chars, &mut pos);
    render(&item)
}

/// A node opens with `(` directly followed by its kind; a token `(` or `)` is set off by
/// a space on its inside; a node closes with a `)` that follows its last child directly.
fn parse_item(chars: &[char], pos: &mut usize) -> Item {
    while chars.get(*pos) == Some(&' ') {
        *pos += 1;
    }
    if chars.get(*pos) == Some(&'(') && chars.get(*pos + 1).is_some_and(char::is_ascii_uppercase) {
        *pos += 1;
        let mut kind = String::new();
        while let Some(&ch) = chars.get(*pos) {
            if ch == ' ' || ch == ')' {
                break;
            }
            kind.push(ch);
            *pos += 1;
        }
        let mut children = Vec::new();
        loop {
            while chars.get(*pos) == Some(&' ') {
                *pos += 1;
            }
            let closes = chars.get(*pos) == Some(&')') && chars.get(*pos - 1) != Some(&' ');
            if closes || *pos >= chars.len() {
                *pos += 1;
                return Item::Node(kind, children);
            }
            children.push(parse_item(chars, pos));
        }
    }
    let mut token = String::new();
    if let Some(&ch) = chars.get(*pos) {
        if ch == '(' || ch == ')' {
            *pos += 1;
            return Item::Token(ch.to_string());
        }
    }
    while let Some(&ch) = chars.get(*pos) {
        if ch == ' ' || ch == ')' {
            break;
        }
        token.push(ch);
        *pos += 1;
    }
    Item::Token(token)
}

fn render(item: &Item) -> String {
    render_with(item, false)
}

/// `item` with redundant parentheses removed and, when `canonical`, without a trailing
/// comma before a closing bracket and with a single-`return` arrow body as its value.
fn render_with(item: &Item, canonical: bool) -> String {
    match item {
        Item::Token(text) => text.clone(),
        Item::Node(kind, children) => {
            if kind == "ParenExpr" && children.len() == 3 {
                return render_with(&children[1], canonical);
            }
            if canonical && kind == "ArrowFunction" {
                if let Some(value) = children.last().and_then(returned_value) {
                    let head: Vec<String> = children[..children.len() - 1]
                        .iter()
                        .map(|child| render_with(child, canonical))
                        .collect();
                    return format!(
                        "({kind} {} {})",
                        head.join(" "),
                        render_with(value, canonical)
                    );
                }
            }
            let mut inner: Vec<String> = Vec::new();
            for (at, child) in children.iter().enumerate() {
                let closer = matches!(children.get(at + 1), Some(Item::Token(t)) if t == ")" || t == "]" || t == "}");
                if canonical && closer && matches!(child, Item::Token(t) if t == ",") {
                    continue;
                }
                inner.push(render_with(child, canonical));
            }
            format!("({kind} {})", inner.join(" "))
        }
    }
}

/// The value an arrow's block body returns when it is a single `return` with a value.
fn returned_value(body: &Item) -> Option<&Item> {
    let Item::Node(kind, children) = body else {
        return None;
    };
    if kind != "Block" || children.len() != 3 {
        return None;
    }
    let Item::Node(statement, parts) = &children[1] else {
        return None;
    };
    if statement != "ReturnStmt" || parts.len() != 2 {
        return None;
    }
    Some(&parts[1])
}
