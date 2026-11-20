//! The types of literal and primitive expressions (SPEC.md §3.1, §4.1).

use sjs_bind::Bindings;
use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{ClassId, Integer, Literal, Number, Printer, Property, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};

/// What is known of a file's names before its expressions are typed. Later passes
/// fill it in; an empty one knows nothing.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    /// The type of a declaration, by its index in [`Bindings::declarations`].
    pub declarations: BTreeMap<usize, TypeId>,
    /// What the `toString()` of a class returns, for the classes that have one.
    pub to_string: BTreeMap<ClassId, TypeId>,
    /// The types of the public fields of a class, by name.
    pub fields: BTreeMap<ClassId, BTreeMap<String, TypeId>>,
    /// The names of the methods and accessors' functions of a class: members that are
    /// called, not read.
    pub methods: BTreeMap<ClassId, BTreeSet<String>>,
    /// The class a class extends.
    pub superclass: BTreeMap<ClassId, ClassId>,
    /// The declarations whose value the program mutates. Given to
    /// [`crate::infer_bindings`], the ones mutated from other files; returned by it,
    /// all of them.
    pub mutated: BTreeSet<usize>,
    /// The declarations something is put into: `push`, `set`, `add`, `name[i] = v`.
    pub filled: BTreeSet<usize>,
    /// The names of the classes, for findings.
    pub class_names: BTreeMap<ClassId, String>,
}

/// Types the expressions of one file.
pub struct Typer<'a> {
    pub(crate) source: &'a SourceFile,
    pub(crate) bindings: &'a Bindings,
    pub(crate) facts: &'a Facts,
    pub(crate) types: &'a mut Types,
    /// The reference each name is, by where the name begins.
    pub(crate) references: BTreeMap<usize, usize>,
    /// The declarations that are default imports: the one such form is a JSON module.
    json: BTreeSet<usize>,
    /// The expressions already judged, by where each begins and ends.
    pub(crate) judged: BTreeSet<(usize, usize)>,
    diagnostics: Diagnostics,
}

pub(crate) fn nodes(node: NodeRef<'_>) -> impl Iterator<Item = NodeRef<'_>> {
    node.children().filter_map(|child| match child {
        Element::Node(inner) => Some(inner),
        Element::Token(_) => None,
    })
}

fn element_span(element: &Element<'_>) -> Span {
    match element {
        Element::Node(node) => node.span(),
        Element::Token(token) => token.token().span,
    }
}

pub(crate) fn element_span_of(element: &Element<'_>) -> Span {
    element_span(element)
}

pub(crate) fn punct(element: Option<&Element<'_>>) -> Option<Punct> {
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
            judged: BTreeSet::new(),
            diagnostics: Diagnostics::new(),
        }
    }

    /// The types the typer interns in.
    pub(crate) fn types(&self) -> &Types {
        self.types
    }

    /// The types the typer interns in, to intern more.
    pub(crate) fn types_mut(&mut self) -> &mut Types {
        self.types
    }

    /// What typing found: E125 for each template filled with what is no text.
    #[must_use]
    pub fn finish(self) -> Diagnostics {
        self.diagnostics
    }

    /// A type without its literals: each literal as its primitive.
    pub(crate) fn widened(&mut self, id: TypeId) -> TypeId {
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
                    (
                        Some(
                            operator @ (Punct::Plus
                            | Punct::Minus
                            | Punct::Star
                            | Punct::Slash
                            | Punct::Percent
                            | Punct::StarStar),
                        ),
                        Some((left, right)),
                    ) => {
                        let (left, right) = (self.type_of(left)?, self.type_of(right)?);
                        self.arithmetic(operator == Punct::Plus, left, right)
                    }
                    _ => None,
                }
            }
            NodeKind::ObjectLit => self.object(node),
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
                match (global, text) {
                    (true, "JSON.parse") => Some(TypeId::UNKNOWN),
                    (true, "Object.keys" | "Object.entries") => {
                        let argument = children.get(2).copied().filter(|_| children.len() == 4);
                        let argument = argument.and_then(|argument| self.type_of(argument));
                        argument.and_then(|shape| self.keys_of(shape, text == "Object.entries"))
                    }
                    _ => None,
                }
            }
            NodeKind::MemberExpr | NodeKind::IndexExpr => {
                let (ty, short) = self.chain(node)?;
                // Past a `?.` that found nothing, the whole chain is `undefined`.
                Some(if short {
                    self.types.union([ty, TypeId::UNDEFINED])
                } else {
                    ty
                })
            }
            NodeKind::OptionalChain => {
                let inside = children.first().copied();
                inside.and_then(|inside| self.type_of(inside))
            }
            _ => None,
        }
    }

    /// The type of an arithmetic operation on two types: `number` for numbers (an
    /// `int32` may leave its range), `bigint` for bigints, and for `+` on strings
    /// `string`; `None` for operands the checker judges.
    pub(crate) fn arithmetic(&mut self, plus: bool, left: TypeId, right: TypeId) -> Option<TypeId> {
        let (left, right) = (self.widened(left), self.widened(right));
        let within =
            |sides: [TypeId; 2], kinds: &[TypeId]| sides.iter().all(|side| kinds.contains(side));
        let sides = [left, right];
        if within(sides, &[TypeId::NUMBER, TypeId::INT32]) {
            Some(TypeId::NUMBER)
        } else if within(sides, &[TypeId::BIGINT]) {
            Some(TypeId::BIGINT)
        } else {
            (plus && within(sides, &[TypeId::STRING])).then_some(TypeId::STRING)
        }
    }

    /// Whether this is the first time `node` is judged: its findings are made once.
    pub(crate) fn first_time(&mut self, node: NodeRef<'_>) -> bool {
        let span = node.span();
        self.judged.insert((span.byte_start, span.byte_end))
    }

    /// Pushes a finding, when it is the first time its expression is typed.
    pub(crate) fn report(&mut self, first: bool, code: Code, span: Span, message: String) {
        if first {
            let found = Diagnostic::new(code, self.source.path(), span, message);
            self.diagnostics.push(found);
        }
    }

    /// The type of an object literal (SPEC.md §4.6): its exact shape, a key with a
    /// literal value retaining it. A spread merges what it spreads, later keys
    /// winning: a shape key by key, a class instance by its public fields, `undefined`
    /// nothing, a union each member in turn (so the literal is a union of shapes);
    /// anything else is E138. A computed key is the literal its expression is (E000
    /// when the expression is known to be none), and E134 when the key is spelled
    /// already, as a key spelled outright after a computed one is. `None` while a
    /// part of it has no type yet.
    fn object(&mut self, node: NodeRef<'_>) -> Option<TypeId> {
        let first = self.first_time(node);
        // Each shape the literal may be: more than one after a union is spread.
        let mut shapes: Vec<Vec<Property>> = vec![Vec::new()];
        // The keys spelled since the start or the last spread: whether outright, and
        // whether computed.
        let mut spelled: BTreeMap<String, (bool, bool)> = BTreeMap::new();
        let mut known = true;
        for member in nodes(node) {
            let parts: Vec<Element<'_>> = member.children().collect();
            let value = parts.last().copied();
            let ty = value.and_then(|value| self.type_of(value));
            if member.kind() == NodeKind::SpreadElement {
                spelled.clear();
                let spread = ty.and_then(|ty| self.spread(first, member, ty, &shapes));
                known &= spread.is_some();
                shapes = spread.unwrap_or(shapes);
                continue;
            }
            let computed = parts.len() == 5;
            let key = if computed {
                let written = self.type_of(parts[1]);
                let key = written.and_then(|written| match self.types.get(written) {
                    Type::Literal(Literal::String(value)) => Some(value.clone()),
                    Type::Literal(Literal::Number(value)) => Some(value.text()),
                    Type::Literal(Literal::BigInt(value)) => Some(value.decimal()),
                    Type::Literal(Literal::Boolean(value)) => Some(value.to_string()),
                    _ => None,
                });
                if written.is_some() && key.is_none() {
                    let message = "a computed key must be a literal the checker can tell: a string, number, bigint or boolean literal, or a `const` of one (SPEC.md §4.6)";
                    self.report(
                        first,
                        Code::E000,
                        element_span(&parts[1]),
                        message.to_owned(),
                    );
                }
                key
            } else {
                member.tokens().first().map(|key| match &key.kind {
                    TokenKind::String(value) => value.to_string(),
                    TokenKind::Number(value) => Number::new(value.value).text(),
                    TokenKind::BigInt(digits) => digits.to_string(),
                    _ => self.source.slice(&key.span).unwrap_or_default().to_owned(),
                })
            };
            let Some(key) = key else {
                known = false;
                continue;
            };
            let before = spelled.entry(key.clone()).or_default();
            // A key spelled outright twice is the parser's finding already.
            let repeated = if computed {
                before.0 || before.1
            } else {
                before.1 && !before.0
            };
            *before = (before.0 || !computed, before.1 || computed);
            if repeated {
                let at = if computed {
                    element_span(&parts[1])
                } else {
                    element_span(&parts[0])
                };
                let message =
                    format!("the key `{key}` is spelled twice in this literal (SPEC.md §4.6)");
                self.report(first, Code::E134, at, message);
            }
            let Some(ty) = ty else {
                known = false;
                continue;
            };
            let literal = matches!(self.types.get(ty), Type::Literal(_));
            for shape in &mut shapes {
                shape.retain(|property| property.key != key);
                shape.push(Property {
                    key: key.clone(),
                    ty,
                    literal,
                });
            }
        }
        let shapes: Vec<TypeId> = shapes
            .into_iter()
            .map(|shape| self.types.intern(Type::Object(shape)))
            .collect();
        known.then(|| self.types.union(shapes))
    }

    /// The shapes after a value of type `ty` is spread into each of `shapes`; `None`
    /// when what it spreads cannot be told, or is nothing that spreads (E138).
    fn spread(
        &mut self,
        first: bool,
        member: NodeRef<'_>,
        ty: TypeId,
        shapes: &[Vec<Property>],
    ) -> Option<Vec<Vec<Property>>> {
        let members: Vec<TypeId> = match self.types.get(ty) {
            Type::Union(members) => members.clone(),
            _ => vec![ty],
        };
        let mut spread: Vec<Vec<Property>> = Vec::new();
        for member_type in members {
            let added: Vec<Property> = match self.types.get(member_type) {
                Type::Undefined => Vec::new(),
                Type::Object(properties) => properties.clone(),
                Type::Class(class) => {
                    let fields = self.facts.fields.get(class)?;
                    let field = |(key, ty): (&String, &TypeId)| Property {
                        key: key.clone(),
                        ty: *ty,
                        literal: false,
                    };
                    fields.iter().map(field).collect()
                }
                // A dictionary spreads keys no shape can name.
                Type::Dictionary(_) => return None,
                _ => {
                    let name = |class: ClassId| self.class_name(class);
                    let message = format!(
                        "a `{}` is spread into an object literal: only a shape, a class instance or `undefined` spreads there (SPEC.md §4.6)",
                        Printer::new(self.types, &name).print(ty)
                    );
                    self.report(first, Code::E138, member.span(), message);
                    return None;
                }
            };
            for shape in shapes {
                let mut merged: Vec<Property> = shape
                    .iter()
                    .filter(|property| !added.iter().any(|new| new.key == property.key))
                    .cloned()
                    .collect();
                merged.extend(added.iter().cloned());
                spread.push(merged);
            }
        }
        // A literal that may be one of very many shapes is given up on.
        (spread.len() <= 64).then_some(spread)
    }

    /// The name of a class, for findings.
    pub(crate) fn class_name(&self, class: ClassId) -> String {
        let name = self.facts.class_names.get(&class);
        name.map_or(format!("class #{}", class.0), Clone::clone)
    }

    /// A type as a `let` or a mutated object holds it: without its literals, the
    /// keys of its shapes without theirs.
    pub(crate) fn loosened(&mut self, id: TypeId) -> TypeId {
        let members: Vec<TypeId> = match self.types.get(id) {
            Type::Union(members) => members.clone(),
            _ => vec![id],
        };
        let mut loosened = Vec::new();
        for member in members {
            let ty = match self.types.get(member).clone() {
                Type::Literal(literal) => literal.primitive(),
                Type::Object(properties) => {
                    let mut wider = Vec::new();
                    for property in properties {
                        wider.push(Property {
                            ty: self.loosened(property.ty),
                            literal: false,
                            key: property.key,
                        });
                    }
                    self.types.intern(Type::Object(wider))
                }
                _ => member,
            };
            loosened.push(ty);
        }
        self.types.union(loosened)
    }

    /// A type without `undefined` and `null`: what `??` lets through from its left.
    pub(crate) fn present(&mut self, id: TypeId) -> TypeId {
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
        if !self.judged.insert((start, start)) {
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
            let name = |class: ClassId| self.class_name(class);
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
        if node.kind() == NodeKind::ObjectLit {
            judge.object(node);
        }
        judge.member_access(node, &children);
        judge.empty_untyped(node);
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
