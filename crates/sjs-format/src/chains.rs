//! Member chains (US-044, SPEC.md §9.3). A chain is a head followed by links: property
//! accesses (`.b`, `?.b`), indexes (`[0]`), calls (`(x)`) and tagged templates. Its lines
//! are the head with the links before the first access, then one line per method call,
//! each beginning with the `.` of the accesses that lead to the call and holding what
//! follows it up to the next call's access. Three or more method calls break the lines
//! whatever the width (S010 when the source does not); fewer stay on one line when it
//! fits and break the same way when it does not; a chain with no call never breaks.

use sjs_diag::{Code, Diagnostic};
use sjs_syntax::{Element, NodeKind, NodeRef};

use crate::doc::{print, Doc};
use crate::expr::Formatter;

/// What a link adds to what precedes it: an access, a call, or something glued to what
/// precedes it (an index, a tagged template).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Member,
    Call,
    Other,
}

/// One link: its kind, its elements (the chain node's children after the first), and
/// whether its first token begins a line in the source.
struct Link<'t> {
    kind: Kind,
    elements: Vec<Element<'t>>,
    starts_line: bool,
}

/// A chain flattened: the head and the links after it, in order.
struct Chain<'t> {
    head: Element<'t>,
    links: Vec<Link<'t>>,
}

/// Whether a node continues the chain of its first child.
pub(crate) fn is_chain(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::MemberExpr
            | NodeKind::IndexExpr
            | NodeKind::CallExpr
            | NodeKind::OptionalChain
            | NodeKind::TaggedTemplate
    )
}

fn link_kind(kind: NodeKind) -> Kind {
    match kind {
        NodeKind::MemberExpr => Kind::Member,
        NodeKind::CallExpr => Kind::Call,
        _ => Kind::Other,
    }
}

/// `node` flattened: the first child followed down while it is a chain node.
fn flatten(node: NodeRef<'_>) -> Chain<'_> {
    let mut links = Vec::new();
    let mut current = node;
    loop {
        let mut children = current.children();
        let first = children.next();
        // An optional chain only wraps the chain it makes optional.
        if current.kind() != NodeKind::OptionalChain {
            let elements: Vec<Element<'_>> = children.collect();
            let starts_line = matches!(
                elements.first(),
                Some(Element::Token(token)) if token.token().has_newline_before
            );
            links.push(Link {
                kind: link_kind(current.kind()),
                elements,
                starts_line,
            });
        }
        match first {
            Some(Element::Node(inner)) if is_chain(inner.kind()) => current = inner,
            other => {
                links.reverse();
                return Chain {
                    head: other.unwrap_or(Element::Node(current)),
                    links,
                };
            }
        }
    }
}

/// How many method calls a chain holds: calls right after an access.
fn method_calls(links: &[Link<'_>]) -> usize {
    links
        .windows(2)
        .filter(|pair| pair[0].kind == Kind::Member && pair[1].kind == Kind::Call)
        .count()
}

/// The indices of the links that begin a line after the head: the first access, and
/// every access after a line that holds a call, as long as a call still follows (the
/// accesses after the last call stay on its line). A chain with no call has none.
fn line_starts(links: &[Link<'_>]) -> Vec<usize> {
    let mut starts = Vec::new();
    let mut open = false;
    for (at, link) in links.iter().enumerate() {
        let call_follows = links[at..].iter().any(|later| later.kind == Kind::Call);
        match link.kind {
            Kind::Member if (starts.is_empty() || open) && call_follows => {
                starts.push(at);
                open = false;
            }
            Kind::Call => open = true,
            Kind::Member | Kind::Other => {}
        }
    }
    starts
}

impl Formatter<'_> {
    /// The document of a chain: its lines as groups, separated by hard breaks when it
    /// holds three or more method calls and by soft ones otherwise.
    pub(crate) fn chain(&self, node: NodeRef<'_>) -> Doc {
        let chain = flatten(node);
        let starts = line_starts(&chain.links);
        let mut head = vec![self.element(chain.head)];
        let mut lines: Vec<Vec<Doc>> = Vec::new();
        for (at, link) in chain.links.iter().enumerate() {
            if starts.contains(&at) {
                lines.push(Vec::new());
            }
            let line = lines.last_mut().unwrap_or(&mut head);
            line.push(self.link(link));
        }
        if lines.is_empty() {
            return Doc::Group(head);
        }
        let separator = if method_calls(&chain.links) >= 3 {
            Doc::HardLine
        } else {
            Doc::SoftLine
        };
        let mut rest = Vec::new();
        for line in lines {
            rest.push(separator.clone());
            rest.extend(line);
        }
        head.push(Doc::indent(rest));
        Doc::Group(head)
    }

    fn link(&self, link: &Link<'_>) -> Doc {
        if link.kind == Kind::Call {
            return self.call_tail(&link.elements);
        }
        Doc::Group(
            link.elements
                .iter()
                .map(|element| self.element(*element))
                .collect(),
        )
    }

    /// S010 for every chain of three or more method calls not laid out one call per
    /// line in the source, with the layout as the replacement.
    #[must_use]
    pub fn method_chains(&self) -> Vec<Diagnostic> {
        let mut findings = Vec::new();
        self.collect_chains(self.tree.root(), false, &mut findings);
        findings
    }

    /// `continued`: the node is the first child of a chain node, so part of its chain.
    fn collect_chains(&self, node: NodeRef<'_>, continued: bool, findings: &mut Vec<Diagnostic>) {
        let chain = is_chain(node.kind());
        if chain && !continued {
            findings.extend(self.chain_finding(node));
        }
        for (at, child) in node.children().enumerate() {
            if let Element::Node(child) = child {
                self.collect_chains(child, chain && at == 0, findings);
            }
        }
    }

    fn chain_finding(&self, node: NodeRef<'_>) -> Option<Diagnostic> {
        let chain = flatten(node);
        if method_calls(&chain.links) < 3 {
            return None;
        }
        let starts = line_starts(&chain.links);
        if starts.iter().all(|&at| chain.links[at].starts_line) {
            return None;
        }
        let current = self.text(node.span()).to_owned();
        let replacement = self.chain_text(node);
        Diagnostic::new(
            Code::S010,
            self.file.path(),
            node.span(),
            "a chain of three or more calls: one call per line, each beginning with its `.` (SPEC.md §9.3)",
        )
        .with_fix(current, replacement)
        .ok()
    }

    /// The chain laid out on its own, its lines after the first indented like the
    /// source line the chain starts on.
    fn chain_text(&self, node: NodeRef<'_>) -> String {
        self.indented_like(node, &print(&[self.chain(node)]))
    }
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    /// `(line, column, current, replacement)` of every S010.
    fn chains(text: &str) -> Vec<(u32, u32, String, String)> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .method_chains()
            .into_iter()
            .map(|d| {
                assert_eq!(d.code, Code::S010);
                (
                    d.span.start.line,
                    d.span.start.column,
                    d.current.clone().unwrap_or_default(),
                    d.replacement.clone().unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn three_method_calls_break_and_two_do_not() {
        assert_eq!(module("a.b().c().d()"), "a\n  .b()\n  .c()\n  .d()\n");
        assert_eq!(module("a.b().c()"), "a.b().c()\n");
        assert_eq!(
            module("f().a().b()"),
            "f().a().b()\n",
            "the head's call is no method call"
        );
        assert_eq!(module("a[0].b().c()"), "a[0].b().c()\n");
        assert_eq!(module("a[0].b().c().d()"), "a[0]\n  .b()\n  .c()\n  .d()\n");
        assert_eq!(
            module("a.b.c().d()"),
            "a.b.c().d()\n",
            "an access before a call is no call"
        );
        assert_eq!(
            module("a.b().c().d().e()"),
            "a\n  .b()\n  .c()\n  .d()\n  .e()\n"
        );
        assert_eq!(
            module("new A().b().c().d()"),
            "new A()\n  .b()\n  .c()\n  .d()\n"
        );
        assert_eq!(
            module("a?.b()?.c().d()"),
            "a\n  ?.b()\n  ?.c()\n  .d()\n",
            "optional links count"
        );
    }

    #[test]
    fn accesses_lead_their_call_and_what_follows_a_call_stays_with_it() {
        assert_eq!(
            module("this.x.y().z().w()"),
            "this\n  .x.y()\n  .z()\n  .w()\n"
        );
        assert_eq!(module("a.b().c().d().e"), "a\n  .b()\n  .c()\n  .d().e\n");
        assert_eq!(module("a.b().c()[0].d()"), "a\n  .b()\n  .c()[0]\n  .d()\n");
        assert_eq!(module("a.b()`t`.c().d()"), "a\n  .b()`t`\n  .c()\n  .d()\n");
        assert_eq!(module("a.b.c.d"), "a.b.c.d\n");
        let long = format!("{}.{}.{}", "a".repeat(40), "b".repeat(40), "c".repeat(40));
        assert_eq!(
            module(&long),
            format!("{long}\n"),
            "accesses alone never break"
        );
    }

    #[test]
    fn an_interrupted_chain_starts_again_and_a_nested_one_is_its_own() {
        assert_eq!(module("(await a.b()).c().d()"), "(await a.b()).c().d()\n");
        assert_eq!(
            module("(await a.b().c().d()).e().f().g()"),
            "(await a\n  .b()\n  .c()\n  .d())\n  .e()\n  .f()\n  .g()\n"
        );
        assert_eq!(
            module("a.b((x) => x.c().d().e()).f()"),
            "a\n  .b(\n    (x) => x\n      .c()\n      .d()\n      .e(),\n  )\n  .f()\n",
            "a broken chain inside breaks the list and the chain around it"
        );
    }

    #[test]
    fn a_short_chain_too_wide_breaks_the_same_way() {
        let long = "a".repeat(60);
        assert_eq!(
            module(&format!("{long}.{long}().c()")),
            format!("{long}\n  .{long}()\n  .c()\n")
        );
    }

    #[test]
    fn s010_names_a_chain_on_one_line_and_lays_it_out() {
        assert_eq!(
            chains("const x = a.b().c().d()\n  f(a.b()\n    .c().d())"),
            vec![
                (
                    1,
                    11,
                    "a.b().c().d()".to_owned(),
                    "a\n  .b()\n  .c()\n  .d()".to_owned()
                ),
                (
                    2,
                    5,
                    "a.b()\n    .c().d()".to_owned(),
                    "a\n    .b()\n    .c()\n    .d()".to_owned()
                ),
            ]
        );
        assert_eq!(
            chains("a\n  .b()\n  .c()\n  .d()\na.b().c()\na.b.c.d\nf().a().b()"),
            Vec::new()
        );
        assert_eq!(
            chains("a.b().c().d()\na.b().c().d().e()"),
            vec![
                (
                    1,
                    1,
                    "a.b().c().d()".to_owned(),
                    "a\n  .b()\n  .c()\n  .d()".to_owned()
                ),
                (
                    2,
                    1,
                    "a.b().c().d().e()".to_owned(),
                    "a\n  .b()\n  .c()\n  .d()\n  .e()".to_owned()
                ),
            ],
            "a chain that is a statement's whole expression, and four calls"
        );
        let long = "a".repeat(60);
        assert_eq!(
            chains(&format!("{long}.{long}().c()")),
            Vec::new(),
            "two calls break for the width, which is no S010"
        );
    }
}
