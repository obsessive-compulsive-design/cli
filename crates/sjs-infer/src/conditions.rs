//! Conditions (SPEC.md §3.5, §4.3): what `if`, `while`, `?:`, `!` and, in a
//! condition, `&&` and `||` ask is a boolean. A value that is `undefined` or `null`
//! or else never false (`T | undefined` where no `T` is `""`, `0`, `0n`, `false` or
//! `NaN`) may be asked too, and is narrowed by the answer; any other is E102, with
//! the comparison to write. A condition that is always the same is E121: one written
//! `true` or `false`, a `const` that is one of them, a comparison a narrowing has
//! settled, and a `while (true)` whose body never leaves it.

use crate::expressions::{element_span_of, nodes, punct, Typer};
use sjs_bind::is_scope;
use sjs_diag::{Code, Diagnostic};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, TokenKind};
use sjs_types::{Literal, Type, TypeId};

/// Whether an expression is `a && b` or `a || b`.
fn is_logical(node: NodeRef<'_>) -> bool {
    let parts: Vec<Element<'_>> = node.children().collect();
    let sign = punct(parts.get(1));
    // Only a binary expression has one of them between its sides.
    matches!(sign, Some(Punct::AmpAmp | Punct::PipePipe))
}

/// Whether an expression is asked for its truth: the test of an `if`, a `while` or a
/// `?:`, what `!` negates, or an operand of `&&` or `||` that is.
pub(crate) fn is_asked(node: NodeRef<'_>) -> bool {
    node.parent().is_some_and(|parent| {
        let first = parent.span().byte_start == node.span().byte_start;
        match parent.kind() {
            NodeKind::IfStmt | NodeKind::WhileStmt => true,
            NodeKind::UnaryExpr => punct(parent.children().next().as_ref()) == Some(Punct::Bang),
            NodeKind::CondExpr => first,
            NodeKind::ParenExpr => is_asked(parent),
            _ => is_logical(parent) && is_asked(parent),
        }
    })
}

/// Whether a loop's body leaves the loop: by a `break` that is the loop's own, not
/// that of a loop or a `switch` inside, or with `jumps`, by `return` or `throw`
/// anywhere in it.
pub(crate) fn leaves(node: NodeRef<'_>, inner: bool, jumps: bool) -> bool {
    nodes(node).filter(|part| !is_scope(*part)).any(|part| {
        let binds = matches!(
            part.kind(),
            NodeKind::WhileStmt | NodeKind::ForStmt | NodeKind::ForOfStmt | NodeKind::SwitchStmt
        );
        match part.kind() {
            NodeKind::ReturnStmt | NodeKind::ThrowStmt => jumps,
            NodeKind::BreakStmt => !inner,
            _ => leaves(part, inner || binds, jumps),
        }
    })
}

impl Typer<'_> {
    /// Whether a member of a union may be a value that counts as false, `undefined`
    /// and `null` aside.
    pub(crate) fn may_be_false(&self, member: TypeId) -> bool {
        match self.types.get(member) {
            Type::Number
            | Type::Int32
            | Type::BigInt
            | Type::String
            | Type::Boolean
            | Type::Unknown
            | Type::Var(_) => true,
            Type::Literal(Literal::Number(number)) => number.value() == 0.0,
            Type::Literal(Literal::BigInt(integer)) => {
                integer.decimal().trim_start_matches('-') == "0"
            }
            Type::Literal(Literal::String(text)) => text.is_empty(),
            Type::Literal(Literal::Boolean(value)) => !value,
            _ => false,
        }
    }

    /// The comparison to write for the truth of `asked`, a `ty` that is no boolean:
    /// `x !== undefined`, `x.length > 0`, `x !== 0`, `x !== ""`, `x !== 0n`; with
    /// `negated`, the opposite. `None` when there is none to tell.
    fn comparison(&mut self, asked: &Element<'_>, ty: TypeId, negated: bool) -> Option<String> {
        // Only what binds tighter than a comparison is compared as it is written.
        let simple = match asked {
            Element::Token(_) => true,
            Element::Node(node) => matches!(
                node.kind(),
                NodeKind::MemberExpr | NodeKind::IndexExpr | NodeKind::CallExpr
            ),
        };
        let text = self
            .source
            .slice(&element_span_of(asked))
            .filter(|_| simple)?;
        let members = self.members_of(ty);
        let absent = [TypeId::UNDEFINED, TypeId::NULL];
        let present = members
            .iter()
            .copied()
            .filter(|member| !absent.contains(member));
        let present = self.types.union(present);
        // Of what is only ever absent there is nothing to tell.
        let text = (present != TypeId::NEVER).then_some(text)?;
        let counts = text.ends_with(".length");
        let (differs, against) = match self.widened(present) {
            _ if members.contains(&TypeId::UNDEFINED) => ("!==", "undefined"),
            _ if members.contains(&TypeId::NULL) => ("!==", "null"),
            // A length is never less than none.
            TypeId::NUMBER | TypeId::INT32 => (if counts { ">" } else { "!==" }, "0"),
            TypeId::STRING => ("!==", "\"\""),
            TypeId::BIGINT => ("!==", "0n"),
            _ => return None,
        };
        let operator = if negated { "===" } else { differs };
        Some(format!("{text} {operator} {against}"))
    }

    /// Judges what is asked for its truth: E102 unless it is a boolean, or a value
    /// that is absent or else never false. `negation` is the `!x` it is the `x` of.
    pub(crate) fn asked(
        &mut self,
        first: bool,
        asked: &Element<'_>,
        ty: Option<TypeId>,
        negation: Option<NodeRef<'_>>,
    ) {
        // What is never reached has no value to ask.
        let asks = |ty: &TypeId| ![TypeId::BOOLEAN, TypeId::NEVER].contains(ty);
        let Some(ty) = ty.filter(|&ty| asks(&self.widened(ty))) else {
            return;
        };
        let members = self.members_of(ty);
        let absent = [TypeId::UNDEFINED, TypeId::NULL];
        let present: Vec<TypeId> = members
            .iter()
            .copied()
            .filter(|member| !absent.contains(member))
            .collect();
        let told = present.len() < members.len()
            && !present.is_empty()
            && !present.iter().any(|&member| self.may_be_false(member));
        if told || !first {
            return;
        }
        let span = negation.map_or(element_span_of(asked), NodeRef::span);
        let comparison = self.comparison(asked, ty, negation.is_some());
        let advice = comparison.as_ref().map_or(
            "compare it with the value it is to be told from".to_owned(),
            |comparison| format!("write `{comparison}`"),
        );
        let message = format!(
            "a `{}` is asked for its truth: a condition is a boolean, or a value that is `undefined` or else never false; {advice} (SPEC.md §3.5)",
            self.print_type(ty)
        );
        let found = Diagnostic::new(Code::E102, self.source.path(), span, message);
        let current = self.source.slice(&span).unwrap_or_default();
        let fixed =
            comparison.and_then(|comparison| found.clone().with_fix(current, comparison).ok());
        self.push(fixed.unwrap_or(found));
    }

    /// What a condition always is, when it is always the same: a boolean literal, a
    /// comparison of two values each known to be the one value, or the `!` of one.
    fn always(&mut self, asked: Element<'_>) -> Option<bool> {
        let unit = |typer: &Self, ty: TypeId| {
            matches!(
                typer.types.get(ty),
                Type::Literal(_) | Type::Undefined | Type::Null
            )
        };
        if let Element::Node(node) = asked {
            let parts: Vec<Element<'_>> = node.children().collect();
            let sign = punct(parts.get(1));
            match (node.kind(), parts.as_slice()) {
                (NodeKind::ParenExpr, [_, inside, _]) => return self.always(*inside),
                (NodeKind::UnaryExpr, [not, operand]) if punct(Some(not)) == Some(Punct::Bang) => {
                    return self.always(*operand).map(|always| !always);
                }
                (NodeKind::BinaryExpr, [left, _, right])
                    if matches!(sign, Some(Punct::EqEqEq | Punct::BangEqEq)) =>
                {
                    let sides = self.type_of(*left).zip(self.type_of(*right));
                    let same = sides.filter(|&(left, right)| left == right && unit(self, left));
                    return same.map(|_| sign == Some(Punct::EqEqEq));
                }
                _ => {}
            }
        }
        match self.type_of(asked).map(|ty| self.types.get(ty)) {
            Some(Type::Literal(Literal::Boolean(always))) => Some(*always),
            _ => None,
        }
    }

    /// E121 for a test that is always the same. A `while` whose test is always true
    /// is the one way to loop without end, and is E121 only when nothing in its body
    /// leaves it.
    pub(crate) fn settled_test(&mut self, first: bool, owner: NodeRef<'_>, test: Element<'_>) {
        let Some(always) = self.always(test) else {
            return;
        };
        // The last `else if` of a chain may be what closes it: the test that names
        // the one value left (US-114).
        let has_else = owner.children().any(|part| {
            matches!(&part, Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::Else))
        });
        let chained = owner
            .parent()
            .is_some_and(|parent| parent.kind() == NodeKind::IfStmt);
        if owner.kind() == NodeKind::IfStmt && chained && !has_else && always {
            return;
        }
        let forever = owner.kind() == NodeKind::WhileStmt && always;
        let message = if forever {
            "this loop never ends: its body has no `break`, `return` or `throw` (SPEC.md §4.3)"
                .to_owned()
        } else {
            format!("this condition is always {always}: what it guards runs always or never (SPEC.md §8.1)")
        };
        if !forever || !leaves(owner, false, true) {
            self.report(first, Code::E121, element_span_of(&test), message);
        }
    }

    /// Judges the test of an `if` or a `while`: E102 for one that is no boolean,
    /// E121 for one that is always the same.
    pub(crate) fn test_rule(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) {
        if !matches!(node.kind(), NodeKind::IfStmt | NodeKind::WhileStmt) {
            return;
        }
        // `if`, `(`, the test.
        for &test in children.iter().skip(2).take(1) {
            let ty = self.type_of(test);
            self.asked(true, &test, ty, None);
            self.settled_test(true, node, test);
        }
    }
}
