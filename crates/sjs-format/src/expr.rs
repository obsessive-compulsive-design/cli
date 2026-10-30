//! Expressions as documents (STORIES.md US-041, SPEC.md §9.2): spaces around binary and
//! assignment operators and around `?` and `:`, a space after `,` and after a
//! property's `:`, none inside `()` and `[]`, `{ a }` for a one-line object or pattern
//! and `{}` for an empty one, strings double-quoted by the function that spells the
//! lexer's S001 replacement, template literals copied verbatim, and parentheses kept
//! only where ES2025 needs them: every other pair is S005 (`Formatter::redundant_parentheses`),
//! and the layout drops it. Lists (arguments, elements, properties, parameters) are
//! groups that break one item per line with a trailing comma; chains, binary and
//! conditional expressions stay on one line until US-044 adds their breaks.

use sjs_diag::{Code, Diagnostic, SourceFile};
use sjs_syntax::{
    double_quoted, Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, Token, TokenKind,
    TriviaKind,
};

use crate::doc::Doc;
use crate::lists::single_return;

/// Precedence of an expression, higher binding tighter: ES2025's levels, with `?.`
/// chains, calls and `import()` as left-hand sides and, above them, members, `new` with
/// arguments, tags and primaries, which is what `new` takes as its callee.
const SEQUENCE: u8 = 0;
const ASSIGN: u8 = 1;
const CONDITIONAL: u8 = 2;
const NULLISH: u8 = 3;
const OR: u8 = 4;
const AND: u8 = 5;
const EXPONENT: u8 = 14;
const UNARY: u8 = 15;
const UPDATE: u8 = 16;
const LEFT_HAND_SIDE: u8 = 17;
const MEMBER: u8 = 18;

/// Formats the expressions of one file.
#[derive(Debug, Clone, Copy)]
pub struct Formatter<'a> {
    pub(crate) file: &'a SourceFile,
    pub(crate) tree: &'a SyntaxTree,
}

impl<'a> Formatter<'a> {
    /// A formatter over `tree`, parsed from `file`.
    #[must_use]
    pub fn new(file: &'a SourceFile, tree: &'a SyntaxTree) -> Self {
        Formatter { file, tree }
    }

    /// The document of an expression node; any other node is a statement's (US-042).
    #[must_use]
    pub fn expression(&self, node: NodeRef<'_>) -> Doc {
        match node.kind() {
            NodeKind::ParenExpr => self.paren(node),
            NodeKind::SequenceExpr => self.separated(node),
            NodeKind::AssignExpr
            | NodeKind::CondExpr
            | NodeKind::BinaryExpr
            | NodeKind::Initializer => self.spaced(node),
            NodeKind::UnaryExpr | NodeKind::AwaitExpr | NodeKind::YieldExpr => self.unary(node),
            NodeKind::UpdateExpr
            | NodeKind::MemberExpr
            | NodeKind::IndexExpr
            | NodeKind::OptionalChain
            | NodeKind::MetaProperty
            | NodeKind::TaggedTemplate
            | NodeKind::SpreadElement
            | NodeKind::RestElement
            | NodeKind::RestParam => self.glued(node),
            NodeKind::CallExpr | NodeKind::NewExpr | NodeKind::ImportCall => self.call(node),
            NodeKind::ArrayLit | NodeKind::ArrayPattern | NodeKind::Params => {
                self.list(node, false)
            }
            NodeKind::ObjectLit | NodeKind::ObjectPattern => self.list(node, true),
            NodeKind::Property
            | NodeKind::PatternProperty
            | NodeKind::PatternElement
            | NodeKind::Param => self.property(node),
            NodeKind::ArrowFunction => self.arrow(node),
            _ => self.statement(node),
        }
    }

    /// The document of a node or a token at an expression's position.
    #[must_use]
    pub fn element(&self, element: Element<'_>) -> Doc {
        match element {
            Element::Node(node) => self.expression(node),
            Element::Token(token) => self.token(token.token()),
        }
    }

    /// S005 for every pair of parentheses the layout drops: the text as written and the
    /// text inside the pair.
    #[must_use]
    pub fn redundant_parentheses(&self) -> Vec<Diagnostic> {
        let mut findings = Vec::new();
        self.collect_redundant(self.tree.root(), &mut findings);
        findings
    }

    fn collect_redundant(&self, node: NodeRef<'_>, findings: &mut Vec<Diagnostic>) {
        // A template literal is copied as written, substitutions included (§9.3), so
        // nothing inside one is judged.
        if node.kind() == NodeKind::TemplateLit {
            return;
        }
        if node.kind() == NodeKind::ParenExpr {
            if let Some(inner) = inner_of(node).filter(|&inner| !self.keeps(node, inner)) {
                let current = self.text(node.span()).to_owned();
                let replacement = self.text(span_of(inner)).to_owned();
                let finding = Diagnostic::new(
                    Code::S005,
                    self.file.path(),
                    node.span(),
                    "redundant parentheses: the expression reads the same without them (SPEC.md §9.2)",
                )
                .with_fix(current, replacement);
                findings.extend(finding.ok());
            }
        }
        for child in node.children() {
            if let Element::Node(child) = child {
                self.collect_redundant(child, findings);
            }
        }
    }

    /// Whether a pair of parentheses stays: ES2025 needs it (the position wants a
    /// tighter expression than the one inside, `??` sits beside `&&` or `||`, a unary
    /// sign would merge with the operand's, an optional chain would otherwise continue,
    /// a statement or an arrow body would otherwise begin with `{`). A pair with nothing
    /// inside never gets here: it is kept as written.
    fn keeps(&self, paren: NodeRef<'_>, inner: Element<'_>) -> bool {
        paren
            .parent()
            .is_none_or(|parent| self.keeps_under(parent, paren, inner))
    }

    /// `keeps` for a pair that is a child of `parent`.
    fn keeps_under(&self, parent: NodeRef<'_>, paren: NodeRef<'_>, inner: Element<'_>) -> bool {
        // A statement or an arrow body that would otherwise begin with `{` (or a
        // removed `function`/`class` form) keeps its parentheses, however deep on the
        // left the pair sits.
        if begins_a_block_like(inner) && leftmost_of(paren) {
            return true;
        }
        let first = index_of(parent, paren) == 0;
        let required = match parent.kind() {
            NodeKind::AssignExpr if first => LEFT_HAND_SIDE,
            NodeKind::CondExpr if first => NULLISH,
            NodeKind::BinaryExpr => {
                let operator = self.operator(parent);
                let level = binary_level(operator);
                if mixes_nullish(level, self.inner_level(inner)) {
                    return true;
                }
                // A left-associative operator takes its own level on the left and a
                // tighter one on the right; `**` takes an update expression on the left
                // (ES2025: no unary base) and its own level on the right.
                match (first, level == EXPONENT) {
                    (true, false) => level,
                    (false, false) => level + 1,
                    (true, true) => UPDATE,
                    (false, true) => EXPONENT,
                }
            }
            NodeKind::UnaryExpr => {
                if self.same_sign(parent, inner) {
                    return true;
                }
                UNARY
            }
            NodeKind::AwaitExpr => UNARY,
            NodeKind::MemberExpr
            | NodeKind::IndexExpr
            | NodeKind::CallExpr
            | NodeKind::TaggedTemplate
                if first =>
            {
                if matches!(inner, Element::Node(node) if node.kind() == NodeKind::OptionalChain) {
                    return true;
                }
                LEFT_HAND_SIDE
            }
            // `new` takes a member expression as its callee (a call would take the
            // arguments) and values as its arguments.
            NodeKind::NewExpr if index_of(parent, paren) == 1 => MEMBER,
            NodeKind::ArrowFunction
            | NodeKind::NewExpr
            | NodeKind::SequenceExpr
            | NodeKind::AssignExpr
            | NodeKind::CondExpr
            | NodeKind::YieldExpr
            | NodeKind::CallExpr
            | NodeKind::ImportCall
            | NodeKind::SpreadElement
            | NodeKind::RestElement
            | NodeKind::ArrayLit
            | NodeKind::ArrayPattern
            | NodeKind::Property
            | NodeKind::PatternProperty
            | NodeKind::PatternElement
            | NodeKind::Param
            | NodeKind::Initializer
            | NodeKind::ForOfStmt
            | NodeKind::ForUpdate => ASSIGN,
            // An expression statement, a pair inside a pair, an index, a statement head
            // (`if`, `while`, `do … while`, `with`, `switch`, `case`, `return`, `throw`),
            // a classic `for` clause or an update's operand (a name or a property, which
            // the parser checks) takes any expression. No other position holds a pair: a
            // removed form keeps its tokens bare.
            _ => SEQUENCE,
        };
        self.inner_level(inner) < required
    }

    /// The precedence of what a pair of parentheses holds, looking through further
    /// pairs: `((a))` is a name.
    fn inner_level(&self, inner: Element<'_>) -> u8 {
        let mut inner = match inner {
            Element::Node(node) => node,
            Element::Token(_) => return MEMBER,
        };
        while inner.kind() == NodeKind::ParenExpr {
            match inner_of(inner) {
                Some(Element::Node(next)) => inner = next,
                _ => return MEMBER,
            }
        }
        match inner.kind() {
            NodeKind::SequenceExpr => SEQUENCE,
            NodeKind::AssignExpr | NodeKind::ArrowFunction | NodeKind::YieldExpr => ASSIGN,
            NodeKind::CondExpr => CONDITIONAL,
            NodeKind::BinaryExpr => binary_level(self.operator(inner)),
            NodeKind::UnaryExpr | NodeKind::AwaitExpr => UNARY,
            NodeKind::UpdateExpr => UPDATE,
            NodeKind::CallExpr | NodeKind::OptionalChain | NodeKind::ImportCall => LEFT_HAND_SIDE,
            _ => MEMBER,
        }
    }

    /// The operator token of a binary or unary expression, as written.
    fn operator(&self, node: NodeRef<'_>) -> &'a str {
        node.children()
            .find_map(|child| match child {
                Element::Token(token)
                    if matches!(
                        token.token().kind,
                        TokenKind::Punct(_) | TokenKind::Keyword(_)
                    ) =>
                {
                    Some(self.text(token.token().span))
                }
                _ => None,
            })
            .unwrap_or("")
    }

    /// Whether the unary `parent`'s sign is the first character of `inner` once the
    /// redundant pairs inside it are dropped, so that without this pair the two would
    /// lex as `++` or `--`.
    fn same_sign(&self, parent: NodeRef<'_>, inner: Element<'_>) -> bool {
        let sign = self.operator(parent);
        if sign != "-" && sign != "+" {
            return false;
        }
        self.leads_with(inner, sign)
    }

    /// The text of the first token of `element`.
    fn first_text(&self, element: Element<'_>) -> &'a str {
        match element {
            Element::Node(node) => node
                .tokens()
                .first()
                .map_or("", |token| self.text(token.span)),
            Element::Token(token) => self.text(token.token().span),
        }
    }

    pub(crate) fn text(&self, span: sjs_diag::Span) -> &'a str {
        self.file.slice(&span).unwrap_or("")
    }

    /// A token as the layout writes it: a string double-quoted with minimal escapes,
    /// anything else as written.
    pub(crate) fn token(&self, token: &Token) -> Doc {
        let text = self.text(token.span);
        match token.kind {
            TokenKind::String(_) => Doc::text(double_quoted(text)),
            _ => Doc::text(text),
        }
    }

    /// `node` as written, line breaks and all.
    pub(crate) fn verbatim(&self, node: NodeRef<'_>) -> Doc {
        Doc::text(self.text(node.span()))
    }

    /// `( inner )`, or `inner` alone when the pair is redundant.
    fn paren(&self, node: NodeRef<'_>) -> Doc {
        match inner_of(node) {
            Some(inner) if !self.keeps(node, inner) => self.element(inner),
            _ => self.glued(node),
        }
    }

    /// Children with no space between them: `a.b`, `a?.[b]`, `i++`, `...a`, a tagged
    /// template.
    pub(crate) fn glued(&self, node: NodeRef<'_>) -> Doc {
        Doc::Group(node.children().map(|child| self.element(child)).collect())
    }

    /// Children with one space between them: `a = b`, `a ? b : c`, `a + b`.
    pub(crate) fn spaced(&self, node: NodeRef<'_>) -> Doc {
        let mut out = Vec::new();
        for (at, child) in node.children().enumerate() {
            if at > 0 {
                out.push(Doc::text(" "));
            }
            out.push(self.element(child));
        }
        Doc::Group(out)
    }

    /// Items with `, ` between them: the removed comma operator.
    fn separated(&self, node: NodeRef<'_>) -> Doc {
        let mut out = Vec::new();
        for child in node.children() {
            match child {
                Element::Token(token) if token.token().kind == TokenKind::Punct(Punct::Comma) => {
                    out.push(Doc::text(", "));
                }
                _ => out.push(self.element(child)),
            }
        }
        Doc::Group(out)
    }

    /// A prefix operator and its operand: a word operator (`typeof`, `await`, `yield`)
    /// and `yield*`'s star take a space, a sign none, unless the operand begins with the
    /// same sign (`- -a`, which `--a` would not be).
    fn unary(&self, node: NodeRef<'_>) -> Doc {
        let children: Vec<Element<'_>> = node.children().collect();
        let operand = children.len().saturating_sub(1);
        let mut out = Vec::new();
        for (at, child) in children.iter().enumerate() {
            if at == operand {
                out.push(self.element(*child));
                break;
            }
            let text = self.first_text(*child);
            out.push(Doc::text(text));
            let word = text.chars().all(char::is_alphabetic) || text == "*";
            let sign = (text == "-" || text == "+") && self.leads_with(children[operand], text);
            if at + 1 == operand && (word || sign) {
                out.push(Doc::text(" "));
            }
        }
        Doc::Group(out)
    }

    /// Whether `element`, once its redundant parentheses are dropped, begins with the
    /// character `sign`.
    fn leads_with(&self, mut element: Element<'_>, sign: &str) -> bool {
        while let Element::Node(node) = element {
            match inner_of(node)
                .filter(|&inner| node.kind() == NodeKind::ParenExpr && !self.keeps(node, inner))
            {
                Some(inner) => element = inner,
                None => break,
            }
        }
        self.first_text(element).starts_with(sign)
    }

    /// A call, `new` or `import(…)`: the callee glued to a parenthesised list.
    fn call(&self, node: NodeRef<'_>) -> Doc {
        let mut out = Vec::new();
        let mut items: Vec<Doc> = Vec::new();
        let mut in_list = false;
        for child in node.children() {
            match child {
                Element::Token(token) if token.token().kind == TokenKind::Punct(Punct::LParen) => {
                    in_list = true;
                }
                Element::Token(token) if token.token().kind == TokenKind::Punct(Punct::RParen) => {
                    in_list = false;
                    out.push(bracketed("(", ")", items, false, false, false));
                    items = Vec::new();
                }
                Element::Token(token) if in_list => {
                    if token.token().kind != TokenKind::Punct(Punct::Comma) {
                        items.push(self.token(token.token()));
                    }
                }
                Element::Node(argument) if in_list => {
                    items.push(self.expression(argument));
                }
                Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::New) => {
                    out.push(Doc::text("new "));
                }
                other => out.push(self.element(other)),
            }
        }
        Doc::Group(out)
    }

    /// `[ … ]`, `{ … }` or `( … )` holding items separated by commas: one line with a
    /// space inside braces when it fits, one item per line with a trailing comma
    /// otherwise, `{}`, `[]` and `()` when empty. A hole in an array stays a hole. An
    /// object literal with a line break or a comment between `{` and its first key
    /// stays one item per line: the one layout intention honored (§9.3).
    pub(crate) fn list(&self, node: NodeRef<'_>, braces: bool) -> Doc {
        let expanded = node.kind() == NodeKind::ObjectLit
            && node.tokens().get(1).is_some_and(|first| {
                first.has_newline_before
                    || first
                        .leading
                        .iter()
                        .any(|trivia| trivia.kind != TriviaKind::Whitespace)
            });
        let mut items: Vec<Doc> = Vec::new();
        let mut open = "";
        let mut close = "";
        let mut after_item = false;
        let mut rest_last = false;
        for child in node.children() {
            match child {
                Element::Token(token) => {
                    let text = self.text(token.token().span);
                    match token.token().kind {
                        TokenKind::Punct(Punct::LBracket | Punct::LBrace | Punct::LParen) => {
                            open = text;
                        }
                        TokenKind::Punct(Punct::RBracket | Punct::RBrace | Punct::RParen) => {
                            close = text;
                        }
                        TokenKind::Punct(Punct::Comma) => {
                            if !after_item {
                                // A hole: nothing between two commas.
                                items.push(Doc::text(""));
                            }
                            after_item = false;
                        }
                        _ => {
                            items.push(self.token(token.token()));
                            after_item = true;
                        }
                    }
                }
                Element::Node(item) => {
                    rest_last = matches!(item.kind(), NodeKind::RestElement | NodeKind::RestParam);
                    items.push(self.expression(item));
                    after_item = true;
                }
            }
        }
        bracketed(open, close, items, braces, rest_last, expanded)
    }

    /// A property, pattern property, pattern element or parameter: `key: value`,
    /// `key = default`, a shorthand, or a method (removed) as written.
    fn property(&self, node: NodeRef<'_>) -> Doc {
        if node
            .children()
            .any(|child| matches!(child, Element::Node(n) if n.kind() == NodeKind::Params))
        {
            return self.verbatim(node);
        }
        let mut out = Vec::new();
        for child in node.children() {
            match child {
                Element::Token(token) if token.token().kind == TokenKind::Punct(Punct::Colon) => {
                    out.push(Doc::text(": "));
                }
                Element::Node(initializer) if initializer.kind() == NodeKind::Initializer => {
                    out.push(Doc::text(" "));
                    out.push(self.spaced(initializer));
                }
                other => out.push(self.element(other)),
            }
        }
        Doc::Group(out)
    }

    /// `[async] (params) => body`; an object-literal body keeps the parentheses that
    /// make it one.
    /// `(params) => body`; a block body that is a single `return` with a value becomes
    /// the value (S011, US-043), in parentheses when it begins with `{`.
    fn arrow(&self, node: NodeRef<'_>) -> Doc {
        let returned = single_return(node).map(|(_, value)| value);
        let mut out = Vec::new();
        for child in node.children() {
            match child {
                Element::Token(token) => {
                    let text = self.text(token.token().span);
                    if text == "=>" {
                        out.push(Doc::text(" => "));
                    } else if text == "async" {
                        out.push(Doc::text("async "));
                    } else {
                        out.push(Doc::text(text));
                    }
                }
                Element::Node(child) => match returned {
                    Some(value) if child.kind() == NodeKind::Block => {
                        out.push(self.expression_body(value));
                    }
                    _ => out.push(self.expression(child)),
                },
            }
        }
        Doc::Group(out)
    }

    /// An arrow's expression body: in parentheses when it would read as a block.
    fn expression_body(&self, value: Element<'_>) -> Doc {
        if begins_a_block_like(value) {
            Doc::Group(vec![Doc::text("("), self.element(value), Doc::text(")")])
        } else {
            self.element(value)
        }
    }
}

/// `open items close` as a group: flat with a space inside braces, one item per line
/// with a trailing comma when broken, empty brackets for no items. A list ending in a
/// rest element or parameter never gets the trailing comma (ES forbids it); a spread
/// argument or element takes it. An `expanded` list is one item per line whatever its
/// width, and so is every list around it.
fn bracketed(
    open: &str,
    close: &str,
    items: Vec<Doc>,
    braces: bool,
    rest_last: bool,
    expanded: bool,
) -> Doc {
    if items.is_empty() {
        return Doc::text(format!("{open}{close}"));
    }
    let separator = match (expanded, braces) {
        (true, _) => Doc::HardLine,
        (false, true) => Doc::Line,
        (false, false) => Doc::SoftLine,
    };
    let between = if expanded { Doc::HardLine } else { Doc::Line };
    let mut inner = vec![separator.clone()];
    for (at, item) in items.into_iter().enumerate() {
        if at > 0 {
            inner.push(Doc::text(","));
            inner.push(between.clone());
        }
        inner.push(item);
    }
    if !rest_last {
        inner.push(Doc::if_break(vec![Doc::text(",")], Vec::new()));
    }
    Doc::group(vec![
        Doc::text(open),
        Doc::indent(inner),
        separator,
        Doc::text(close),
    ])
}

/// What a pair of parentheses holds: its one child that is not a parenthesis.
fn inner_of(paren: NodeRef<'_>) -> Option<Element<'_>> {
    paren.children().find(|child| match child {
        Element::Token(token) => !matches!(
            token.token().kind,
            TokenKind::Punct(Punct::LParen | Punct::RParen)
        ),
        Element::Node(_) => true,
    })
}

/// Whether `inner` begins with `{`, `function` or `class`: at the start of a
/// statement or an arrow body it would read as a block or a declaration.
pub(crate) fn begins_a_block_like(inner: Element<'_>) -> bool {
    match inner {
        Element::Node(node) => node.tokens().first().is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Punct(Punct::LBrace)
                    | TokenKind::Keyword(Keyword::Function | Keyword::Class)
            )
        }),
        Element::Token(_) => false,
    }
}

/// Whether `paren` is the first token of a statement or of an arrow's body: every
/// ancestor up to there holds it as its first child.
fn leftmost_of(node: NodeRef<'_>) -> bool {
    node.parent().is_some_and(|parent| match parent.kind() {
        // Under an arrow it is the body: a parameter list never holds a pair first.
        NodeKind::ExprStmt | NodeKind::ArrowFunction => true,
        _ => index_of(parent, node) == 0 && leftmost_of(parent),
    })
}

/// The position of `node` among `parent`'s children, tokens counted.
fn index_of(parent: NodeRef<'_>, node: NodeRef<'_>) -> usize {
    parent
        .children()
        .position(|child| matches!(child, Element::Node(n) if n.id() == node.id()))
        .unwrap_or(usize::MAX)
}

fn span_of(element: Element<'_>) -> sjs_diag::Span {
    match element {
        Element::Node(node) => node.span(),
        Element::Token(token) => token.token().span,
    }
}

/// The precedence of a binary operator, as written.
fn binary_level(operator: &str) -> u8 {
    match operator {
        "??" => NULLISH,
        "||" => OR,
        "&&" => AND,
        "|" => 6,
        "^" => 7,
        "&" => 8,
        "===" | "!==" | "==" | "!=" => 9,
        "<" | ">" | "<=" | ">=" | "instanceof" | "in" => 10,
        "<<" | ">>" | ">>>" => 11,
        "+" | "-" => 12,
        "*" | "/" | "%" => 13,
        _ => EXPONENT,
    }
}

/// Whether an operand at `inner` under an operator at `outer` mixes `??` with `&&` or
/// `||`, which ES2025 forbids without parentheses.
fn mixes_nullish(outer: u8, inner: u8) -> bool {
    (outer == NULLISH && matches!(inner, OR | AND))
        || (matches!(outer, OR | AND) && inner == NULLISH)
}

#[cfg(test)]
mod tests {
    use super::Formatter;
    use crate::doc::{layout, print};
    use sjs_diag::{Code, SourceFile};
    use sjs_syntax::{parse, Element, NodeKind};

    fn file(text: &str) -> SourceFile {
        SourceFile::new("a.js", text.as_bytes().to_vec()).expect("utf-8")
    }

    /// Every expression statement of `text`, formatted, one per line.
    fn formatted(text: &str) -> String {
        formatted_at(text, 100)
    }

    fn formatted_at(text: &str, width: usize) -> String {
        let f = file(text);
        let (tree, _) = parse(&f);
        let formatter = Formatter::new(&f, &tree);
        let mut lines = Vec::new();
        for item in tree.root().children() {
            if let Element::Node(statement) = item {
                if statement.kind() == NodeKind::ExprStmt {
                    let expression = statement.children().next().expect("an expression");
                    lines.push(layout(&[formatter.element(expression)], width));
                }
            }
        }
        lines.join("\n")
    }

    /// The S005 fixes of `text`: `(current, replacement)` in source order.
    fn redundant(text: &str) -> Vec<(String, String)> {
        let f = file(text);
        let (tree, _) = parse(&f);
        Formatter::new(&f, &tree)
            .redundant_parentheses()
            .into_iter()
            .map(|d| {
                assert_eq!(d.code, Code::S005);
                (
                    d.current.unwrap_or_default(),
                    d.replacement.unwrap_or_default(),
                )
            })
            .collect()
    }

    #[test]
    fn operators_get_spaces_and_members_calls_and_unaries_do_not() {
        assert_eq!(formatted("a+b*c"), "a + b * c");
        assert_eq!(formatted("a=b"), "a = b");
        assert_eq!(formatted("a  ??  b"), "a ?? b");
        assert_eq!(formatted("a?b:c"), "a ? b : c");
        assert_eq!(formatted("a . b [ c ] ( d , e )"), "a.b[c](d, e)");
        assert_eq!(formatted("a ?. b ?. [ c ] ?. ( d )"), "a?.b?.[c]?.(d)");
        assert_eq!(formatted("- a"), "-a");
        assert_eq!(formatted("! ! a"), "!!a");
        assert_eq!(formatted("typeof  a"), "typeof a");
        assert_eq!(formatted("await  p"), "await p");
        assert_eq!(formatted("i ++\n-- j"), "i++\n--j");
        assert_eq!(formatted("new  A ( 1 )"), "new A(1)");
        assert_eq!(formatted("new ns . A ( )"), "new ns.A()");
        assert_eq!(formatted("import ( \"./a.js\" )"), "import(\"./a.js\")");
        assert_eq!(formatted("f ( ... xs )"), "f(...xs)");
        assert_eq!(formatted("x = a instanceof B"), "x = a instanceof B");
    }

    #[test]
    fn lists_are_one_line_with_braces_spaced_or_one_item_per_line() {
        assert_eq!(formatted("[ 1 , 2 ]"), "[1, 2]");
        assert_eq!(formatted("[ ]"), "[]");
        assert_eq!(
            formatted("x = { a , b : 1 , \"c\" : 2 , [ d ] : 3 , ... e }"),
            "x = { a, b: 1, \"c\": 2, [d]: 3, ...e }"
        );
        assert_eq!(formatted("x = { }"), "x = {}");
        assert_eq!(formatted("f ( )"), "f()");
        let long = "x".repeat(60);
        assert_eq!(
            formatted(&format!("f({long}, {long})")),
            format!("f(\n  {long},\n  {long},\n)"),
            "arguments break one per line with a trailing comma"
        );
        assert_eq!(
            formatted(&format!("x = {{ a: {long}, b: {long} }}")),
            format!("x = {{\n  a: {long},\n  b: {long},\n}}")
        );
        assert_eq!(formatted_at("[1, 2]", 5), "[\n  1,\n  2,\n]");
        assert_eq!(
            formatted("[1, , 2]"),
            "[1, , 2]",
            "a hole stays (it is E000)"
        );
        assert_eq!(
            formatted("f(a, b,)"),
            "f(a, b)",
            "a trailing comma goes when flat"
        );
    }

    #[test]
    fn patterns_and_parameters_follow_the_list_rules() {
        assert_eq!(
            formatted("[ a , b = 1 , ... c ] = d"),
            "[a, b = 1, ...c] = d"
        );
        assert_eq!(
            formatted("( { a , b : c = 1 , ... d } = o )"),
            "({ a, b: c = 1, ...d } = o)"
        );
        assert_eq!(
            formatted("( a , b = 1 , ... c ) => d"),
            "(a, b = 1, ...c) => d"
        );
        assert_eq!(
            formatted("async ( a ) => { return a }"),
            "async (a) => a",
            "a single return is the body (S011, US-043)"
        );
        assert_eq!(formatted("( ) => ( { a } )"), "() => ({ a })");
        assert_eq!(
            formatted("( ) => { }"),
            "() => {}",
            "an empty block body (US-042)"
        );
        let long = "x".repeat(60);
        assert_eq!(
            formatted(&format!("({long}, ...{long}) => 1")),
            format!("(\n  {long},\n  ...{long}\n) => 1"),
            "a rest parameter never gets a trailing comma"
        );
        assert_eq!(
            formatted_at("f(aaaa, ...bbbb)", 12),
            "f(\n  aaaa,\n  ...bbbb,\n)",
            "a spread argument takes one"
        );
    }

    #[test]
    fn strings_are_double_quoted_and_templates_copied_as_written() {
        assert_eq!(formatted("x = 'a'"), "x = \"a\"");
        assert_eq!(
            formatted("x = '\"'"),
            "x = \"\\\"\"",
            "a string that is only `\"`"
        );
        assert_eq!(formatted("x = 'it\\'s'"), "x = \"it's\"");
        assert_eq!(
            formatted("x = \"a\\\"b\""),
            "x = \"a\\\"b\"",
            "already double-quoted"
        );
        assert_eq!(
            formatted("f( 'k' , { 'a' : 1 } )"),
            "f(\"k\", { \"a\": 1 })"
        );
        assert_eq!(
            formatted("x = `a ${ b+c }  d`"),
            "x = `a ${ b+c }  d`",
            "verbatim, substitutions too"
        );
        assert_eq!(formatted("x = String.raw `a\\n`"), "x = String.raw`a\\n`");
        assert_eq!(
            formatted("x = `a\nb`"),
            "x = `a\nb`",
            "line breaks inside stay"
        );
    }

    #[test]
    fn redundant_parentheses_are_dropped_and_reported() {
        assert_eq!(formatted("((a))"), "a");
        assert_eq!(
            redundant("((a))"),
            vec![
                ("((a))".to_owned(), "(a)".to_owned()),
                ("(a)".to_owned(), "a".to_owned())
            ],
            "one finding per pair"
        );
        assert_eq!(formatted("(a + b) + c"), "a + b + c");
        assert_eq!(formatted("a * (b * c) + (d)"), "a * (b * c) + d");
        assert_eq!(formatted("(a) = 1"), "a = 1");
        assert_eq!(
            redundant("(a) = 1"),
            vec![("(a)".to_owned(), "a".to_owned())]
        );
        assert_eq!(formatted("( a.b ) += 1"), "a.b += 1");
        assert_eq!(formatted("x = (a ? b : c)"), "x = a ? b : c");
        assert_eq!(formatted("x = a ? (b) : (c = 1)"), "x = a ? b : c = 1");
        assert_eq!(
            formatted("f((a), [(b)], { c: (d) })"),
            "f(a, [b], { c: d })"
        );
        assert_eq!(formatted("(a.b)()"), "a.b()");
        assert_eq!(formatted("x = (a.b)`t`"), "x = a.b`t`", "a member is a tag");
        assert_eq!(formatted("(a)++"), "a++");
        assert_eq!(formatted("typeof (a)"), "typeof a");
        assert_eq!(formatted("await (p)"), "await p");
        assert_eq!(formatted("a ** (b ** c)"), "a ** b ** c");
        assert_eq!(formatted("(a || b) || c"), "a || b || c");
        assert_eq!(formatted("(a && b) || c"), "a && b || c");
        assert_eq!(formatted("(a ?? b) ?? c"), "a ?? b ?? c");
        assert_eq!(formatted("x = () => (a + b)"), "x = () => a + b");
        assert_eq!(
            formatted("x = `${(a)}`"),
            "x = `${(a)}`",
            "inside a template nothing changes"
        );
        assert_eq!(
            formatted("if ((a)) {}\nx = 1"),
            "x = 1",
            "statements are US-042's"
        );
        assert_eq!(
            redundant("if ((a)) {}"),
            vec![("(a)".to_owned(), "a".to_owned())]
        );
        assert_eq!(
            redundant("const a = (1)"),
            vec![("(1)".to_owned(), "1".to_owned())]
        );
        assert_eq!(redundant("return (a)").len(), 1);
    }

    #[test]
    fn needed_parentheses_stay_and_are_not_reported() {
        for text in [
            "x = (a ?? b) || c",
            "x = (a || b) ?? c",
            "x = a ?? (b && c)",
            "x = (-1) ** 2",
            "x = (await p).x",
            "x = await (a + b)",
            "x = (() => 1)()",
            "x = typeof (a + b)",
            "x = -(-a)",
            "x = +(+a)",
            "x = -(--a)",
            "x = a + (b + c)",
            "x = (a + b) * c",
            "x = (a ** b) ** c",
            "x = -(a ** b)",
            "x = (a, b)",
            "x = (a = 1) + 2",
            "x = (a ? b : c) ? d : e",
            "x = (a?.b).c",
            "x = (a?.b)()",
            "({ a } = o)",
            "({}).x",
            "x = () => ({})",
            "x = () => ({}).x",
            "new (f())()",
            "x = (a + b).c",
            "x = (a + b)[0]",
            "x = (a + b)()",
            "x = f(...(a, b))",
        ] {
            assert_eq!(redundant(text), Vec::new(), "{text}");
            let out = formatted(text);
            assert_eq!(out, text, "{text}");
        }
        assert_eq!(formatted("x = -(-a)"), "x = -(-a)");
        assert_eq!(
            redundant("function* g() {\n  x = (yield) + 1\n  y = (yield)\n}"),
            vec![("(yield)".to_owned(), "yield".to_owned())],
            "a `yield` needs its pair as an operand, not as a whole value"
        );
        assert_eq!(
            formatted("x = - -a"),
            "x = - -a",
            "a space keeps the signs apart"
        );
        assert_eq!(formatted("x = -(+a)"), "x = -+a", "different signs merge");
        assert_eq!(formatted("x = -(++a)"), "x = -++a");
        assert_eq!(formatted("x = !(!a)"), "x = !!a");
    }

    #[test]
    fn deeper_positions_and_unjudged_pairs() {
        assert_eq!(formatted("x = -(a)"), "x = -a");
        assert_eq!(
            formatted("x = -((-a))"),
            "x = -(-a)",
            "the inner pair would merge the signs once the outer goes"
        );
        assert_eq!(
            redundant("x = -((-a))"),
            vec![("(-a)".to_owned(), "-a".to_owned())]
        );
        assert_eq!(
            redundant("x = (a++).b"),
            Vec::new(),
            "an update is no left-hand side"
        );
        assert_eq!(
            redundant("(a + b) = 1"),
            Vec::new(),
            "an invalid target is not judged"
        );
        assert_eq!(
            formatted("new (a.b)()"),
            "new a.b()",
            "a member is a callee"
        );
        assert_eq!(formatted("new (A)((b))"), "new A(b)");
        assert_eq!(
            formatted("new A((b + c))"),
            "new A(b + c)",
            "an argument is a value"
        );
        assert_eq!(formatted("new (new A())()"), "new new A()()");
        assert_eq!(
            redundant("new (a?.b)()\nnew (f())()\nnew (() => 1)()"),
            Vec::new(),
            "a chain, a call or an arrow is no member"
        );
        assert_eq!(formatted("x = -((a))"), "x = -a");
        assert_eq!(
            formatted("x = ((a + b)) * c"),
            "x = (a + b) * c",
            "the level looks through pairs"
        );
        assert_eq!(formatted("x = ([a]).b"), "x = [a].b");
        assert_eq!(formatted("x = (`t`).length"), "x = `t`.length");
        assert_eq!(formatted("x = a[(b)]"), "x = a[b]");
        assert_eq!(formatted("[(a)] = b"), "[a] = b");
        assert_eq!(formatted("({ a: (b.c) } = o)"), "({ a: b.c } = o)");
        assert_eq!(formatted("[...(a)] = b"), "[...a] = b");
        assert_eq!(
            redundant("const x = ({}).a"),
            vec![("({})".to_owned(), "{}".to_owned())],
            "an object begins a block only at a statement's start"
        );
        assert_eq!(
            redundant("x = `${(a)}`"),
            Vec::new(),
            "nothing inside a template is judged"
        );
        assert_eq!(
            redundant("for (const x of (a, b)) {}"),
            Vec::new(),
            "for-of takes one value"
        );
        assert_eq!(
            redundant("for (let i = 0; (i < n); i += (1)) {}"),
            vec![
                ("(i < n)".to_owned(), "i < n".to_owned()),
                ("(1)".to_owned(), "1".to_owned())
            ],
            "a loop header takes an expression, its update a value"
        );
        assert_eq!(
            redundant("class A extends (B) {}"),
            Vec::new(),
            "a removed form keeps its tokens bare: no pair to judge"
        );
    }

    #[test]
    fn removed_and_unknown_forms_are_copied_as_written() {
        assert_eq!(formatted("x = function ( ) { }"), "x = function ( ) { }");
        assert_eq!(formatted("x = { m ( ) { } }"), "x = { m ( ) { } }");
        assert_eq!(formatted("x = class  A { }"), "x = class  A { }");
        assert_eq!(formatted("a , b"), "a, b");
        assert_eq!(formatted("x = yield"), "x = yield", "no operand, no space");
        assert_eq!(formatted("function* g() {}\nx = yield* a"), "x = yield* a");
        assert_eq!(formatted("x = new.target"), "x = new.target");
        assert_eq!(
            formatted("x = ( )"),
            "x = ()",
            "a pair with nothing inside is kept"
        );
    }

    #[test]
    fn the_output_prints_through_the_layout() {
        let f = file("f(a)");
        let (tree, _) = parse(&f);
        let formatter = Formatter::new(&f, &tree);
        let doc = tree
            .root()
            .children()
            .next()
            .map(|statement| formatter.element(statement));
        assert_eq!(
            doc.map(|doc| print(&[doc])).as_deref(),
            Some("f(a)"),
            "a statement node is copied as written"
        );
    }
}
