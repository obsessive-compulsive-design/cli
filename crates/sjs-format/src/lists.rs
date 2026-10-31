//! Lists and arrow bodies (US-043, SPEC.md §9.3): S004 for a trailing comma that does
//! not match a list's canonical layout, and S011 for an arrow whose block body is a
//! single `return`. The layout itself lives in [`crate::expr`]: a list is one line when
//! it fits and one item per line otherwise, with a trailing comma only then; an object
//! literal with a line break or a comment after its `{` stays one item per line.

use sjs_diag::{Code, Diagnostic, SourceFile};
use sjs_syntax::{parse, Element, NodeKind, NodeRef, Punct, Token, TokenKind, TriviaKind};

use crate::expr::{begins_a_block_like, Formatter};

/// The nodes whose items a comma separates inside brackets.
const LISTS: [NodeKind; 10] = [
    NodeKind::ArrayLit,
    NodeKind::ObjectLit,
    NodeKind::ArrayPattern,
    NodeKind::ObjectPattern,
    NodeKind::Params,
    NodeKind::NamedImports,
    NodeKind::NamedExports,
    NodeKind::CallExpr,
    NodeKind::NewExpr,
    NodeKind::ImportCall,
];

impl Formatter<'_> {
    /// S004 for every list whose trailing comma differs from its canonical layout's: a
    /// multi-line list ends in one, a single-line list does not. The canonical text is
    /// parsed again and its lists compared with the source's, pair by pair.
    #[must_use]
    pub fn trailing_commas(&self) -> Vec<Diagnostic> {
        SourceFile::new(self.file.path(), self.module().into_bytes())
            .ok()
            .map_or_else(Vec::new, |formatted| self.compare_lists(&formatted))
    }

    fn compare_lists(&self, formatted: &SourceFile) -> Vec<Diagnostic> {
        let (tree, _) = parse(formatted);
        let before = lists_of(self.tree.root());
        let after = lists_of(tree.root());
        // A file the grammar rejects may be read differently once laid out: then the
        // lists do not pair up and nothing is said.
        let aligned = before.len() == after.len()
            && before
                .iter()
                .zip(&after)
                .all(|(source, canonical)| source.kind() == canonical.kind());
        if !aligned {
            return Vec::new();
        }
        before
            .iter()
            .zip(&after)
            .filter_map(|(source, canonical)| {
                self.comma_finding(*source, trailing_comma(*canonical).is_some())
            })
            .collect()
    }

    /// The S004 of one list, given whether its canonical layout ends in a comma.
    fn comma_finding(&self, list: NodeRef<'_>, canonical: bool) -> Option<Diagnostic> {
        match (trailing_comma(list), canonical) {
            (Some(comma), false) => Diagnostic::new(
                Code::S004,
                self.file.path(),
                comma.span,
                "a trailing comma in a single-line list: only a multi-line list ends in one (SPEC.md §9.2)",
            )
            .with_fix(",", "")
            .ok(),
            (None, true) => last_item_end(list)
                .and_then(|end| self.file.span(end, end))
                .and_then(|span| {
                    Diagnostic::new(
                        Code::S004,
                        self.file.path(),
                        span,
                        "no trailing comma in a multi-line list: every multi-line list ends in one (SPEC.md §9.2)",
                    )
                    .with_fix("", ",")
                    .ok()
                }),
            _ => None,
        }
    }

    /// S011 for every arrow whose block body is a single `return` with a value: the
    /// value is the body, in parentheses when it begins with `{`.
    #[must_use]
    pub fn arrow_returns(&self) -> Vec<Diagnostic> {
        let mut findings = Vec::new();
        self.collect_arrow_returns(self.tree.root(), &mut findings);
        findings
    }

    fn collect_arrow_returns(&self, node: NodeRef<'_>, findings: &mut Vec<Diagnostic>) {
        if node.kind() == NodeKind::ArrowFunction {
            if let Some((block, value)) = single_return(node) {
                let current = self.text(block.span()).to_owned();
                let value_text = self.text(span_of(value));
                let replacement = if begins_a_block_like(value) {
                    format!("({value_text})")
                } else {
                    value_text.to_owned()
                };
                let finding = Diagnostic::new(
                    Code::S011,
                    self.file.path(),
                    block.span(),
                    "an arrow whose block body is a single `return`: the value is the body (SPEC.md §9.3)",
                )
                .with_fix(current, replacement);
                findings.extend(finding.ok());
            }
        }
        for child in node.children() {
            if let Element::Node(child) = child {
                self.collect_arrow_returns(child, findings);
            }
        }
    }
}

/// The block body of `arrow` and the value it returns, when the body is a block holding
/// one `return` with a value and no comment.
pub(crate) fn single_return(arrow: NodeRef<'_>) -> Option<(NodeRef<'_>, Element<'_>)> {
    let Some(Element::Node(block)) = arrow.children().last() else {
        return None;
    };
    let commented = block
        .tokens()
        .iter()
        .any(|token| token.leading.iter().any(|trivia| is_comment(trivia.kind)));
    if block.kind() != NodeKind::Block || commented {
        return None;
    }
    let children: Vec<Element<'_>> = block.children().collect();
    let [_, Element::Node(statement), _] = children.as_slice() else {
        return None;
    };
    if statement.kind() != NodeKind::ReturnStmt {
        return None;
    }
    statement
        .children()
        .nth(1)
        .filter(|value| !is_punct(*value, Punct::Semicolon))
        .map(|value| (block, value))
}

fn is_comment(kind: TriviaKind) -> bool {
    matches!(
        kind,
        TriviaKind::LineComment | TriviaKind::BlockComment { .. } | TriviaKind::DocComment { .. }
    )
}

fn is_punct(element: Element<'_>, punct: Punct) -> bool {
    matches!(element, Element::Token(token) if token.token().kind == TokenKind::Punct(punct))
}

/// Every list under `node`, in source order.
fn lists_of(node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut lists = Vec::new();
    collect_lists(node, &mut lists);
    lists
}

fn collect_lists<'t>(node: NodeRef<'t>, lists: &mut Vec<NodeRef<'t>>) {
    if LISTS.contains(&node.kind()) {
        lists.push(node);
    }
    for child in node.children() {
        if let Element::Node(child) = child {
            collect_lists(child, lists);
        }
    }
}

/// The comma right before a list's closing bracket.
fn trailing_comma(list: NodeRef<'_>) -> Option<&Token> {
    let children: Vec<Element<'_>> = list.children().collect();
    children
        .iter()
        .rev()
        .nth(1)
        .and_then(|element| match element {
            Element::Token(token) if token.token().kind == TokenKind::Punct(Punct::Comma) => {
                Some(token.token())
            }
            _ => None,
        })
}

/// Where a list's last item ends: the byte after the element before the closing
/// bracket. Only a list that is not empty gets asked.
fn last_item_end(list: NodeRef<'_>) -> Option<usize> {
    let children: Vec<Element<'_>> = list.children().collect();
    children
        .iter()
        .rev()
        .nth(1)
        .map(|element| span_of(*element).byte_end)
}

fn span_of(element: Element<'_>) -> sjs_diag::Span {
    match element {
        Element::Node(node) => node.span(),
        Element::Token(token) => token.token().span,
    }
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::parse;

    /// A finding: its code, line, column and fix.
    type Found = (Code, u32, u32, Option<(String, String)>);

    fn found(text: &str, returns: bool) -> Vec<Found> {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        let formatter = Formatter::new(&f, &tree);
        let findings = if returns {
            formatter.arrow_returns()
        } else {
            formatter.trailing_commas()
        };
        findings
            .into_iter()
            .map(|d| {
                let fix = d.current.clone().zip(d.replacement.clone());
                (d.code, d.span.start.line, d.span.start.column, fix)
            })
            .collect()
    }

    fn commas(text: &str) -> Vec<Found> {
        found(text, false)
    }

    fn returns(text: &str) -> Vec<Found> {
        found(text, true)
    }

    fn module(text: &str) -> String {
        let f = SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8");
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree).module()
    }

    fn fix(current: &str, replacement: &str) -> (String, String) {
        (current.to_owned(), replacement.to_owned())
    }

    const LONG: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn a_trailing_comma_in_a_single_line_list_is_s004() {
        assert_eq!(
            commas("f(a, b,)\nconst [x, y,] = [1, 2,]\nconst { k, } = { k: 1, }\nnew A(1,)\nimport { a, } from \"./a.js\"\nexport { a, }"),
            vec![
                (Code::S004, 1, 7, Some(fix(",", ""))),
                (Code::S004, 2, 12, Some(fix(",", ""))),
                (Code::S004, 2, 22, Some(fix(",", ""))),
                (Code::S004, 3, 10, Some(fix(",", ""))),
                (Code::S004, 3, 22, Some(fix(",", ""))),
                (Code::S004, 4, 8, Some(fix(",", ""))),
                (Code::S004, 5, 11, Some(fix(",", ""))),
                (Code::S004, 6, 11, Some(fix(",", ""))),
            ]
        );
        assert_eq!(
            commas("const f = (a, b,) => 1"),
            vec![(Code::S004, 1, 16, Some(fix(",", "")))]
        );
    }

    #[test]
    fn a_missing_trailing_comma_in_a_multi_line_list_is_s004() {
        assert_eq!(
            commas(&format!("f({LONG}, b)")),
            vec![(Code::S004, 1, 106, Some(fix("", ",")))],
            "at the end of the last item"
        );
        assert_eq!(
            commas("const o = {\n  a: 1,\n  b: 2\n}"),
            vec![(Code::S004, 3, 7, Some(fix("", ",")))],
            "a kept multi-line object"
        );
        assert_eq!(
            commas(&format!(
                "const o = {{\n  a: 1,\n  b: 2,\n}}\nconst a = [{LONG}, 1,]\n"
            )),
            Vec::new(),
            "the commas the layout wants"
        );
    }

    #[test]
    fn lists_that_match_their_layout_produce_nothing() {
        assert_eq!(
            commas("f(a, b)\nconst [x] = [1]\nconst { k } = { k: 1 }\nf()\n[]\n({})"),
            Vec::new()
        );
        assert_eq!(
            commas(&format!("function f({LONG}, ...rest) {{}}\n")),
            Vec::new(),
            "a rest parameter never takes the comma"
        );
        assert_eq!(
            module(&format!("const {{ a }} = import(\"./{LONG}.js\")\n")),
            format!("const {{ a }} = import(\n  \"./{LONG}.js\"\n)\n"),
            "an import() never takes the comma either"
        );
        assert_eq!(
            commas(&format!("const {{ a }} = import(\"./{LONG}.js\")\n")),
            Vec::new()
        );
        assert_eq!(commas(""), Vec::new());
    }

    #[test]
    fn a_file_the_layout_reads_differently_is_left_alone() {
        assert_eq!(
            commas("if (a) b(c,)\nelse d()"),
            Vec::new(),
            "laid out, the rejected form reads differently: the lists do not pair up"
        );
        assert_eq!(
            commas("var v = [1,]"),
            Vec::new(),
            "a form copied as written is its own layout"
        );
    }

    #[test]
    fn an_arrow_block_that_is_one_return_is_s011_and_becomes_the_body() {
        assert_eq!(
            returns("const f = () => { return a }\nconst g = () => { return { a } }\nconst h = async (x) => {\n  return await x;\n}"),
            vec![
                (Code::S011, 1, 17, Some(fix("{ return a }", "a"))),
                (Code::S011, 2, 17, Some(fix("{ return { a } }", "({ a })"))),
                (Code::S011, 3, 24, Some(fix("{\n  return await x;\n}", "await x"))),
            ]
        );
        assert_eq!(
            module("const f = () => { return a }\nconst g = () => { return { a } }\nf(() => { return [1] })"),
            "const f = () => a\n\nconst g = () => ({ a })\n\nf(() => [1])\n"
        );
    }

    #[test]
    fn other_arrow_blocks_stay() {
        for text in [
            "() => { return }",
            "() => { return; }",
            "() => {}",
            "() => { f()\n  return a }",
            "() => { // why\n  return a }",
            "() => { return a // why\n}",
            "() => { f() }",
            "() =>",
        ] {
            assert_eq!(returns(text), Vec::new(), "{text}");
        }
        assert_eq!(
            module("const f = () => {\n  f()\n  return a\n}"),
            "const f = () => {\n  f()\n  return a\n}\n"
        );
    }
}
