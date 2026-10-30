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
    match item {
        Item::Token(text) => text.clone(),
        Item::Node(kind, children) => {
            if kind == "ParenExpr" && children.len() == 3 {
                return render(&children[1]);
            }
            let inner: Vec<String> = children.iter().map(render).collect();
            format!("({kind} {})", inner.join(" "))
        }
    }
}
