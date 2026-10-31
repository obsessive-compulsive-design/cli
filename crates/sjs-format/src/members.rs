//! Class member order (US-047, SPEC.md §9.4). A class body's members stand in groups:
//! static fields; static methods, static accessors and `static {}` blocks; instance
//! fields, public then `#private`; the constructor; accessors; methods, public then
//! `#private`. Within a group the source order stays, so a getter and a setter written
//! together stay together and a `static {}` block keeps its place among the static
//! methods. A member moves with the comments right above it, below any blank line, and
//! with the comment at the end of its line. A body holding tokens the parser left bare
//! (a `;`, a recovery's leftovers) or a token the lexer rejected is left as written. `Formatter::member_order()`
//! reports S009 once per class out of order, with the body as written and the body in
//! order.

use sjs_diag::{Code, Diagnostic};
use sjs_syntax::{Element, NodeKind, NodeRef, Punct, TokenKind};

use crate::doc::{print, Doc};
use crate::expr::Formatter;
use crate::stmt::{detached_len, items_of, reorder, Body, Item};

const MESSAGE: &str = "class members out of order: static fields, static methods, fields, the \
                       constructor, accessors, methods, public before private (SPEC.md §9.4)";

impl Formatter<'_> {
    /// S009 for every class whose members are out of order: the body as written and the
    /// body in order.
    #[must_use]
    pub fn member_order(&self) -> Vec<Diagnostic> {
        let mut findings = Vec::new();
        self.collect_bodies(self.tree.root(), &mut findings);
        findings
    }

    fn collect_bodies(&self, node: NodeRef<'_>, findings: &mut Vec<Diagnostic>) {
        if is_laid_out_body(node) {
            findings.extend(self.body_finding(node));
        }
        for child in node.children() {
            if let Element::Node(child) = child {
                self.collect_bodies(child, findings);
            }
        }
    }

    fn body_finding(&self, body: NodeRef<'_>) -> Option<Diagnostic> {
        let mut items = items_of(body, false);
        let order = self.member_positions(&items)?;
        if order.iter().enumerate().all(|(to, &from)| to == from) {
            return None;
        }
        self.sort_members(&mut items);
        let laid_out = Doc::Group(vec![
            Doc::text("{"),
            Doc::indent(self.lines(&items, false)),
            Doc::HardLine,
            Doc::text("}"),
        ]);
        let current = self.text(body.span()).to_owned();
        let replacement = self.indented_like(body, &print(&[laid_out]));
        Diagnostic::new(Code::S009, self.file.path(), body.span(), MESSAGE)
            .with_fix(current, replacement)
            .ok()
    }

    /// A class body's items (the closing brace last) with the members in their groups.
    pub(crate) fn sort_members(&self, items: &mut [Item<'_>]) {
        if let Some(order) = self.member_positions(items) {
            reorder(items, &order, |above, _| detached_len(above));
        }
    }

    /// The positions of a class body's items in order: members by group, equal groups
    /// in their order, the closing brace last; `None` for a body holding bare tokens or
    /// a token the lexer rejected.
    fn member_positions(&self, items: &[Item<'_>]) -> Option<Vec<usize>> {
        let rejected = items.iter().any(|item| match item.body {
            Body::Node(member) => member
                .tokens()
                .iter()
                .any(|token| token.kind == TokenKind::Error),
            Body::Stray(..) => true,
            Body::End => false,
        });
        if rejected {
            return None;
        }
        let groups: Vec<usize> = items
            .iter()
            .map(|item| match item.body {
                Body::Node(member) => self.group_of(member),
                Body::Stray(..) | Body::End => usize::MAX,
            })
            .collect();
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by_key(|&at| groups[at]);
        Some(order)
    }

    /// A member's group: static fields first, then static methods (with static accessors
    /// and `static {}` blocks), fields, `#private` fields, the constructor, accessors,
    /// methods, `#private` methods.
    fn group_of(&self, member: NodeRef<'_>) -> usize {
        let (is_static, key) = self.modifier_and_key(member);
        let private = usize::from(key.starts_with('#'));
        match member.kind() {
            NodeKind::StaticBlock => 1,
            NodeKind::ClassField if is_static => 0,
            NodeKind::ClassField => 2 + private,
            NodeKind::ClassAccessor if is_static => 1,
            NodeKind::ClassAccessor => 5,
            _ if is_static => 1,
            _ if key == "constructor" => 4,
            _ => 6 + private,
        }
    }

    /// Whether a member is `static`, and its key as written: the key is the child before
    /// the parameters, the initializer, the body or the `;`, or else the last child; the
    /// member is static when a `static` token precedes the key (a key named `static` is
    /// one only when nothing precedes it). A `[Symbol.…]` key reads as no name.
    fn modifier_and_key(&self, member: NodeRef<'_>) -> (bool, &str) {
        let children: Vec<Element<'_>> = member.children().collect();
        let end = children
            .iter()
            .position(|child| match child {
                Element::Node(node) => matches!(
                    node.kind(),
                    NodeKind::Params | NodeKind::Initializer | NodeKind::Block
                ),
                Element::Token(token) => token.token().kind == TokenKind::Punct(Punct::Semicolon),
            })
            .unwrap_or(children.len());
        let key_at = end.saturating_sub(1);
        let text_of = |at: usize| match children.get(at) {
            Some(Element::Token(token)) => self.text(token.token().span),
            _ => "",
        };
        (key_at > 0 && text_of(0) == "static", text_of(key_at))
    }
}

/// Whether `node` is the body of a class declaration, which the layout writes member by
/// member (a class expression, removed, is copied as written).
pub(crate) fn is_laid_out_body(node: NodeRef<'_>) -> bool {
    node.kind() == NodeKind::ClassBody
        && node
            .parent()
            .is_some_and(|parent| parent.kind() == NodeKind::ClassDecl)
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    /// A finding: its code, line, column and fix.
    type Found = (Code, u32, u32, Option<(String, String)>);

    fn found(text: &str) -> Vec<Found> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .member_order()
            .into_iter()
            .map(|d| {
                let fix = d.current.clone().zip(d.replacement.clone());
                (d.code, d.span.start.line, d.span.start.column, fix)
            })
            .collect()
    }

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    /// Formats twice: the second pass must change nothing.
    fn stable(text: &str) -> String {
        let out = module(text);
        assert_eq!(module(&out), out, "a second pass changes:\n{out}");
        out
    }

    fn fix(current: &str, replacement: &str) -> (String, String) {
        (current.to_owned(), replacement.to_owned())
    }

    #[test]
    fn members_stand_in_their_groups_and_keep_their_order_within_one() {
        assert_eq!(
            stable("class A {\n  m() {}\n  [Symbol.iterator]() {}\n  #q() {}\n  set g(v) {}\n  get g() {}\n  constructor() {}\n  #p = 2\n  x = 1\n  static {\n    f()\n  }\n  static s() {}\n  static #r() {}\n  static get b() {}\n  static t = 1\n  static static = 1\n  static constructor() {}\n}"),
            "class A {\n  static t = 1\n  static static = 1\n  static {\n    f()\n  }\n  static s() {}\n  static #r() {}\n  static get b() {}\n  static constructor() {}\n  x = 1\n  #p = 2\n  constructor() {}\n  set g(v) {}\n  get g() {}\n  m() {}\n  [Symbol.iterator]() {}\n  #q() {}\n}\n"
        );
        assert_eq!(
            stable("class A {\n  get a() {}\n  m() {}\n  set a(v) {}\n  get #c() {}\n  set #c(v) {}\n}"),
            "class A {\n  get a() {}\n  set a(v) {}\n  get #c() {}\n  set #c(v) {}\n  m() {}\n}\n",
            "a getter and its setter stay together"
        );
        assert_eq!(
            stable("class A {\n  a = 1\n}\nclass B {}\nclass C {\n  static {\n    f()\n  }\n  static s() {}\n  static {\n    g()\n  }\n}"),
            "class A {\n  a = 1\n}\n\nclass B {}\n\nclass C {\n  static {\n    f()\n  }\n  static s() {}\n  static {\n    g()\n  }\n}\n",
            "one member, none, and static blocks among static methods: nothing moves"
        );
        assert_eq!(
            stable("const A = class {\n  m() {}\n  a = 1\n}"),
            "const A = class {\n  m() {}\n  a = 1\n}\n",
            "a class expression (E000) is copied as written"
        );
        assert_eq!(
            stable("class A {\n  static s() {}\n  static\n}"),
            "class A {\n  static s() {}\n  static\n}\n",
            "a field named `static` is not static"
        );
        assert_eq!(
            stable("class A {\n  m() {}\n  ;\n  a = 1\n}"),
            "class A {\n  m() {}\n  ;\n  a = 1\n}\n",
            "a body holding a bare `;` is left as written"
        );
    }

    #[test]
    fn a_field_before_a_bracket_or_star_member_keeps_the_semicolon_it_needs() {
        assert_eq!(
            stable("class A {\n  [Symbol.iterator]() {}\n  *g() {}\n  x = 1\n  #y\n  m() {}\n}"),
            "class A {\n  x = 1\n  #y;\n  [Symbol.iterator]() {}\n  *g() {}\n  m() {}\n}\n",
            "in order, the last field meets the `[` member"
        );
        assert_eq!(
            stable("class A {\n  x = 1;\n  *g() {}\n  y = 2;\n  m() {}\n}"),
            "class A {\n  x = 1\n  y = 2;\n  *g() {}\n  m() {}\n}\n",
            "kept where needed, dropped where not"
        );
    }

    #[test]
    fn comments_move_with_their_member_and_detached_lines_stay() {
        assert_eq!(
            stable("class A {\n  // about m\n  m() {} // m's\n  /** The field. */\n  a = 1\n}"),
            "class A {\n  /** The field. */\n  a = 1\n  // about m\n  m() {}  // m's\n}\n"
        );
        assert_eq!(
            stable("class A {\n  m() {}\n  // group two\n\n  a = 1\n\n  b = 2\n}"),
            "class A {\n  a = 1\n  // group two\n\n  b = 2\n\n  m() {}\n}\n",
            "a comment above a blank line and the blank lines keep their positions"
        );
    }

    #[test]
    fn s009_names_a_class_out_of_order_with_its_body_in_order() {
        assert_eq!(
            found("class A {\n  m() {}\n  a = 1\n}\nclass B {\n  a = 1\n  m() {}\n}"),
            vec![(
                Code::S009,
                1,
                9,
                Some(fix("{\n  m() {}\n  a = 1\n}", "{\n  a = 1\n  m() {}\n}"))
            )],
            "one per class, none for a class in order"
        );
        assert_eq!(
            found("function f() {\n  class A {\n    m() {}\n\n    a = 1\n  }\n}"),
            vec![(
                Code::S009,
                2,
                11,
                Some(fix(
                    "{\n    m() {}\n\n    a = 1\n  }",
                    "{\n    a = 1\n\n    m() {}\n  }"
                ))
            )],
            "nested, the body is indented like its class and a blank line stays blank"
        );
        for text in [
            "class A {\n  a = 1\n}",
            "class A {}",
            "const A = class {\n  m() {}\n  a = 1\n}",
            "class A {\n  m() {}\n  ;\n  a = 1\n}",
            "class A {\n  m() {}\n  # b = 2\n}",
            "",
        ] {
            assert_eq!(found(text), Vec::new(), "{text}");
        }
    }
}
