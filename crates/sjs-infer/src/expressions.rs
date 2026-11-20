//! The types of literal and primitive expressions (SPEC.md §3.1, §4.1).

use sjs_bind::Bindings;
use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{ClassId, Integer, Literal, Number, Printer, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

/// What is known of a file's names before its expressions are typed. Later passes
/// fill it in; an empty one knows nothing.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The type of a declaration, by its index in [`Bindings::declarations`].
    pub declarations: BTreeMap<usize, TypeId>,
    /// What the `toString()` of a class returns, for the classes that have one.
    pub to_string: BTreeMap<ClassId, TypeId>,
    /// The names of the classes, for findings.
    pub class_names: BTreeMap<ClassId, String>,
}

/// Types the expressions of one file.
pub struct Typer<'a> {
    source: &'a SourceFile,
    bindings: &'a Bindings,
    facts: &'a Facts,
    types: &'a mut Types,
    /// The reference each name is, by where the name begins.
    references: BTreeMap<usize, usize>,
    /// The declarations that are default imports: the one such form is a JSON module.
    json: BTreeSet<usize>,
    /// The templates already judged, by where each begins.
    templates: BTreeSet<usize>,
    diagnostics: Diagnostics,
}

fn nodes(node: NodeRef<'_>) -> impl Iterator<Item = NodeRef<'_>> {
    node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner),
        Element::Token(_) => None,
    })
}

fn punct(element: Option<&Element<'_>>) -> Option<Punct> {
    match element {
        Some(Element::Token(token)) => match token.token().kind {
            TokenKind::Punct(punct) => Some(punct),
            _ => None,
        },
        _ => None,
    }
}

impl<'a> Typer<'a> {
    /// A typer of the file `source`, whose names `bindings` resolved.
    pub fn new(
        source: &'a SourceFile,
        tree: &'a SyntaxTree,
        bindings: &'a Bindings,
        facts: &'a Facts,
        types: &'a mut Types,
    ) -> Self {
        let references = bindings.references.iter().enumerate();
        let imports = nodes(tree.root()).filter(|node| node.kind() == NodeKind::ImportDecl);
        let defaults = imports
            .flat_map(nodes)
            .filter(|clause| clause.kind() == NodeKind::DefaultImport);
        let starts: BTreeSet<usize> = defaults.map(|clause| clause.span().byte_start).collect();
        let declarations = bindings.declarations.iter().enumerate();
        Self {
            source,
            bindings,
            facts,
            types,
            references: references
                .map(|(at, reference)| (reference.span.byte_start, at))
                .collect(),
            json: declarations
                .filter(|(_, declaration)| starts.contains(&declaration.span.byte_start))
                .map(|(at, _)| at)
                .collect(),
            templates: BTreeSet::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// What typing found: E125 for each template filled with what is no text.
    #[must_use]
    pub fn finish(self) -> Diagnostics {
        self.diagnostics
    }

    /// A type without its literals: each literal as its primitive.
    fn widened(&mut self, id: TypeId) -> TypeId {
        let members: Vec<TypeId> = match self.types.get(id) {
            Type::Union(members) => members.clone(),
            _ => vec![id],
        };
        let widened: Vec<TypeId> = members
            .iter()
            .map(|&member| match self.types.get(member) {
                Type::Literal(literal) => literal.primitive(),
                _ => member,
            })
            .collect();
        self.types.union(widened)
    }

    /// The type of the name at `start`: of its declaration when that is known, of the
    /// global when it is one whose type is plain.
    fn name(&self, start: usize, text: &str) -> Option<TypeId> {
        let reference = self.references.get(&start);
        let declaration = reference.and_then(|&at| self.bindings.references[at].declaration);
        match (declaration, text) {
            (Some(declaration), _) if self.json.contains(&declaration) => Some(TypeId::UNKNOWN),
            (Some(declaration), _) => self.facts.declarations.get(&declaration).copied(),
            (None, "undefined") => Some(TypeId::UNDEFINED),
            (None, "NaN" | "Infinity") => Some(TypeId::NUMBER),
            (None, _) => None,
        }
    }

    /// The type of an expression, when it can be told yet; `None` for one a later pass
    /// types.
    pub fn type_of(&mut self, expression: Element<'_>) -> Option<TypeId> {
        match expression {
            Element::Node(node) => self.node(node),
            Element::Token(token) => {
                let span = token.token().span;
                match &token.token().kind {
                    // An integer past 2⁵³ is not the value the double holds.
                    TokenKind::Number(number) if number.exceeds_safe_integer => {
                        Some(TypeId::NUMBER)
                    }
                    TokenKind::Number(number) => Some(
                        self.types
                            .literal(Literal::Number(Number::new(number.value))),
                    ),
                    TokenKind::BigInt(digits) => Integer::parse(digits)
                        .map(|integer| self.types.literal(Literal::BigInt(integer))),
                    TokenKind::String(value) => {
                        Some(self.types.literal(Literal::String(value.to_string())))
                    }
                    TokenKind::Template(template) => Some(
                        self.types
                            .literal(Literal::String(template.cooked.to_string())),
                    ),
                    TokenKind::Bool(value) => Some(self.types.literal(Literal::Boolean(*value))),
                    TokenKind::Null => Some(TypeId::NULL),
                    TokenKind::Ident => {
                        let text = self.source.slice(&span).unwrap_or_default();
                        self.name(span.byte_start, text)
                    }
                    _ => None,
                }
            }
        }
    }

    fn node(&mut self, node: NodeRef<'_>) -> Option<TypeId> {
        let children: Vec<Element<'_>> = node.children().collect();
        match node.kind() {
            NodeKind::ParenExpr => {
                let inside = children.get(1).copied();
                inside.and_then(|inside| self.type_of(inside))
            }
            NodeKind::TemplateLit => {
                self.template(node.span().byte_start, &children);
                Some(TypeId::STRING)
            }
            NodeKind::UnaryExpr => self.unary(node, &children),
            NodeKind::BinaryExpr => {
                let sides = children.first().copied().zip(children.last().copied());
                match (punct(children.get(1)), sides) {
                    (Some(Punct::QuestionQuestion), Some((left, right))) => {
                        let (left, right) = (self.type_of(left)?, self.type_of(right)?);
                        let present = self.present(left);
                        Some(self.types.union([present, right]))
                    }
                    (Some(Punct::EqEqEq | Punct::BangEqEq), _) => Some(TypeId::BOOLEAN),
                    _ => None,
                }
            }
            NodeKind::ArrayLit => {
                let elements: Vec<Element<'_>> = children
                    .iter()
                    .copied()
                    .filter(|child| punct(Some(child)).is_none())
                    .collect();
                let mut types = Vec::new();
                for element in &elements {
                    types.push(self.type_of(*element)?);
                }
                let element = self.types.union(types);
                // An empty literal takes its type from what is put in it (E110).
                let element = (!elements.is_empty()).then_some(self.widened(element))?;
                Some(self.types.intern(Type::Array {
                    element,
                    mutated: false,
                }))
            }
            NodeKind::CallExpr => {
                // `JSON.parse(…)`, of the global `JSON`: anything at all.
                let callee = match children.first() {
                    Some(Element::Node(callee)) => *callee,
                    _ => return None,
                };
                let text = self.source.slice(&callee.span()).unwrap_or_default();
                let reference = self.references.get(&callee.span().byte_start);
                let global =
                    reference.is_some_and(|&at| self.bindings.references[at].declaration.is_none());
                (text == "JSON.parse" && global).then_some(TypeId::UNKNOWN)
            }
            _ => None,
        }
    }

    /// A type without `undefined` and `null`: what `??` lets through from its left.
    fn present(&mut self, id: TypeId) -> TypeId {
        let members: Vec<TypeId> = match self.types.get(id) {
            Type::Union(members) => members.clone(),
            _ => vec![id],
        };
        let nullish = [TypeId::UNDEFINED, TypeId::NULL];
        self.types.union(
            members
                .into_iter()
                .filter(|member| !nullish.contains(member)),
        )
    }

    /// `-x`, `!x`, `typeof x`. The minus of a number or bigint literal is the literal
    /// of the negated value, so `-2147483648` is an `int32` and `-0` is not.
    fn unary(&mut self, node: NodeRef<'_>, children: &[Element<'_>]) -> Option<TypeId> {
        // The operator is the first token of the expression.
        match node.tokens().first().map(|operator| &operator.kind) {
            Some(TokenKind::Punct(Punct::Bang)) => Some(TypeId::BOOLEAN),
            Some(TokenKind::Keyword(Keyword::Typeof)) => Some(TypeId::STRING),
            Some(TokenKind::Punct(Punct::Minus)) => {
                let operand = children.last().copied();
                let operand = operand.and_then(|operand| self.type_of(operand))?;
                match self.types.get(operand).clone() {
                    Type::Literal(Literal::Number(number)) => Some(
                        self.types
                            .literal(Literal::Number(Number::new(-number.value()))),
                    ),
                    Type::Literal(Literal::BigInt(integer)) => {
                        let decimal = integer.decimal();
                        let negated = decimal
                            .strip_prefix('-')
                            .map_or(format!("-{decimal}"), str::to_owned);
                        Integer::parse(&negated)
                            .map(|integer| self.types.literal(Literal::BigInt(integer)))
                    }
                    // The negation of the least `int32` is no `int32`.
                    Type::Number | Type::Int32 => Some(TypeId::NUMBER),
                    Type::BigInt => Some(TypeId::BIGINT),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether a value of this type may fill a template: a string, a number, a bigint,
    /// a boolean, a union of those, or an instance of a class whose `toString()` is
    /// seen to return a string.
    fn is_text(&self, id: TypeId) -> bool {
        match self.types.get(id) {
            Type::String
            | Type::Number
            | Type::Int32
            | Type::BigInt
            | Type::Boolean
            | Type::Literal(_) => true,
            Type::Union(members) => members.iter().all(|&member| self.is_text(member)),
            Type::Class(class) => {
                let returns = self.facts.to_string.get(class);
                returns.is_some_and(|&returns| {
                    matches!(
                        self.types.get(returns),
                        Type::String | Type::Literal(Literal::String(_))
                    )
                })
            }
            _ => false,
        }
    }

    /// Types what fills a template; E125 for what is no text.
    fn template(&mut self, start: usize, children: &[Element<'_>]) {
        // Once: a template inside another is reached from both.
        if !self.templates.insert(start) {
            return;
        }
        let is_piece = |child: &Element<'_>| matches!(child, Element::Token(token) if matches!(token.token().kind, TokenKind::Template(_)));
        for filling in children.iter().filter(|child| !is_piece(child)) {
            let Some(ty) = self.type_of(*filling).filter(|&ty| !self.is_text(ty)) else {
                continue;
            };
            let span = match filling {
                Element::Node(node) => node.span(),
                Element::Token(token) => token.token().span,
            };
            let name = |class: ClassId| {
                let name = self.facts.class_names.get(&class);
                name.map_or(format!("class #{}", class.0), Clone::clone)
            };
            let message = format!(
                "a template is filled with a `{}`: it takes a string, a number, a bigint or a boolean, or an object whose `toString()` returns a string (SPEC.md §4.1)",
                Printer::new(self.types, &name).print(ty)
            );
            let found = Diagnostic::new(Code::E125, self.source.path(), span, message);
            self.diagnostics.push(found);
        }
    }
}

/// E100 for each `null` that is not the right operand of `===` or `!==`, and E125 for
/// each template filled with a value that is no text, as far as the types of the
/// expressions can be told from `facts`.
pub fn check_expressions(
    source: &SourceFile,
    tree: &SyntaxTree,
    bindings: &Bindings,
    facts: &Facts,
    types: &mut Types,
) -> Diagnostics {
    let mut judge = Typer::new(source, tree, bindings, facts, types);
    let mut found = Diagnostics::new();
    let mut pending = vec![tree.root()];
    while let Some(node) = pending.pop() {
        let children: Vec<Element<'_>> = node.children().collect();
        pending.extend(nodes(node));
        if node.kind() == NodeKind::TemplateLit {
            judge.template(node.span().byte_start, &children);
        }
        let compares = node.kind() == NodeKind::BinaryExpr
            && matches!(
                punct(children.get(1)),
                Some(Punct::EqEqEq | Punct::BangEqEq)
            );
        for (at, child) in children.iter().enumerate() {
            let Element::Token(token) = child else {
                continue;
            };
            // A key named `null` is no `null`: `o.null`, `{ null: 1 }`, a class member.
            let is_key = match node.kind() {
                NodeKind::MemberExpr => at > 0,
                NodeKind::Property => match children.get(at + 1) {
                    // A method: `null() {}`.
                    Some(Element::Node(_)) => true,
                    after => punct(after) == Some(Punct::Colon),
                },
                NodeKind::ClassField | NodeKind::ClassMethod | NodeKind::ClassAccessor => true,
                _ => false,
            };
            let allowed = is_key || (compares && at + 1 == children.len());
            if token.token().kind == TokenKind::Null && !allowed {
                let message = "`null` is written only to the right of `===` or `!==`: the absent value a program makes is `undefined` (SPEC.md §3.1)";
                let span = token.token().span;
                found.push(Diagnostic::new(Code::E100, source.path(), span, message));
            }
        }
    }
    found.extend(judge.finish().iter().cloned());
    found
}
