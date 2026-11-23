//! The types of literal and primitive expressions (SPEC.md §3.1, §4.1).

use crate::functions::{Passed, Site};
use crate::promises::consumes;
use sjs_bind::Bindings;
use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{ClassId, Integer, Literal, Number, Printer, Property, Type, TypeId, Types, VarId};
use std::collections::{BTreeMap, BTreeSet};

/// What is known of a class beyond its fields and the names of its methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClassFacts {
    /// The names of the members it declares itself, typed yet or not.
    pub declared: BTreeSet<String>,
    /// The types of its methods, by name.
    pub signatures: BTreeMap<String, TypeId>,
    /// What each of its getters gives, when that is known.
    pub getters: BTreeMap<String, Option<TypeId>>,
    /// What each of its setters takes, when that is known.
    pub setters: BTreeMap<String, Option<TypeId>>,
    /// Its type parameters.
    pub params: Vec<VarId>,
    /// What the class it extends is given for its type parameters, in terms of the
    /// class's own; `None` when that cannot be told.
    pub base_arguments: Option<Vec<TypeId>>,
    /// The type of its constructor, which is the type of the class as a value.
    pub constructor: Option<TypeId>,
    /// What its `then` method passes on, when it has one: what an `await` of an
    /// instance gives.
    pub resolves: Option<TypeId>,
}

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
    /// What each function declared by name returns, by the declaration of its name.
    pub returns: BTreeMap<usize, TypeId>,
    /// How many arguments each function declared by name takes: at least, and at
    /// most unless it has a rest.
    pub arity: BTreeMap<usize, (usize, Option<usize>)>,
    /// The parameters of each function declared by name, in order: the declaration
    /// of each that is a plain name.
    pub parameters: BTreeMap<usize, Vec<Option<usize>>>,
    /// The type of the elements of each rest parameter, by its declaration.
    pub rest: BTreeMap<usize, TypeId>,
    /// The call sites of each parameter, by its declaration: those of this file and
    /// those of [`Facts::external_sites`].
    pub sites: BTreeMap<usize, Vec<Site>>,
    /// Given to [`crate::infer_bindings`]: the call sites of this file's parameters
    /// in other files.
    pub external_sites: BTreeMap<usize, Vec<Site>>,
    /// The arguments this file passes to functions it imports.
    pub passed: Vec<Passed>,
    /// The parameters nothing says anything of (E112): the body treats each
    /// opaquely, nothing calls its function, and the function is not exported.
    pub unconstrained: BTreeSet<usize>,
    /// The parameters that are a number, a bigint or a string for being ordered,
    /// with no call to say which.
    pub ordered: BTreeSet<usize>,
    /// The parameters that are what the functions they are passed to take.
    pub pinned: BTreeSet<usize>,
    /// The names of the classes, for findings.
    pub class_names: BTreeMap<ClassId, String>,
    /// The class each declaration names: a class, or an import of one. The class
    /// after it is its static side, which holds its static members.
    pub class_of: BTreeMap<usize, ClassId>,
    /// What is known of each class beyond its fields and the names of its methods.
    pub classes: BTreeMap<ClassId, ClassFacts>,
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
    /// The types of the parameters of the arrows typed as values, by declaration.
    pub(crate) locals: BTreeMap<usize, TypeId>,
    /// Those of them that are a number, a bigint or a string for being ordered.
    pub(crate) loose: BTreeSet<usize>,
    /// The calls of each declaration by name.
    pub(crate) called: BTreeMap<usize, Vec<NodeRef<'a>>>,
    /// The declarations used somewhere as a promise is: awaited, returned, passed on
    /// or stored.
    pub(crate) consumed: BTreeSet<usize>,
    /// The arrows typed as values, by where each begins: `None` for one that has no
    /// type, or that is being typed.
    pub(crate) arrows: BTreeMap<usize, Option<TypeId>>,
    /// Where the parameters that are typed by what their arrow is expected to be
    /// take their types from, by declaration: each place, with a note.
    pub(crate) origins: BTreeMap<usize, Vec<(Span, String)>>,
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
        let references: BTreeMap<usize, usize> = references
            .map(|(at, reference)| (reference.span.byte_start, at))
            .collect();
        let mut calls: BTreeMap<usize, Vec<NodeRef<'a>>> = BTreeMap::new();
        let mut consumed = BTreeSet::new();
        let mut pending = vec![tree.root()];
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node));
            for (at, child) in node.children().enumerate() {
                let reference = match child {
                    Element::Token(name) => references.get(&name.token().span.byte_start),
                    Element::Node(_) => None,
                };
                let named = reference.and_then(|&at| bindings.references[at].declaration);
                consumed.extend(named.filter(|_| consumes(node, at)));
            }
            let name = match node.children().next() {
                Some(Element::Token(name)) if node.kind() == NodeKind::CallExpr => {
                    references.get(&name.token().span.byte_start)
                }
                _ => None,
            };
            let named = name.and_then(|&at| bindings.references[at].declaration);
            if let Some(named) = named {
                calls.entry(named).or_default().push(node);
            }
        }
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
            references,
            json: declarations
                .filter(|(_, declaration)| starts.contains(&declaration.span.byte_start))
                .map(|(at, _)| at)
                .collect(),
            judged: BTreeSet::new(),
            locals: BTreeMap::new(),
            loose: BTreeSet::new(),
            called: calls,
            consumed,
            arrows: BTreeMap::new(),
            origins: BTreeMap::new(),
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
        let local = declaration.and_then(|declaration| self.locals.get(&declaration).copied());
        match (declaration, text) {
            _ if local.is_some() => local,
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
                    TokenKind::Keyword(Keyword::This) => {
                        let side = self.side_of(token.parent());
                        side.map(|side| self.instance(side))
                    }
                    TokenKind::Keyword(Keyword::Super) => self.base_of(token.parent()),
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
            NodeKind::BinaryExpr => self.binary(node, &children),
            NodeKind::CondExpr => self.conditional(node, &children),
            NodeKind::ObjectLit => self.object(node),
            NodeKind::ArrayLit => {
                let elements: Vec<Element<'_>> = children
                    .iter()
                    .copied()
                    .filter(|child| punct(Some(child)).is_none())
                    .collect();
                let mut types = Vec::new();
                for element in &elements {
                    let ty = match element {
                        // A spread holds what iterating what it spreads gives.
                        Element::Node(spread) if spread.kind() == NodeKind::SpreadElement => {
                            let spread = spread.children().last();
                            let spread = spread.and_then(|spread| self.type_of(spread))?;
                            self.item_of(spread, false)?
                        }
                        _ => self.type_of(*element)?,
                    };
                    types.push(ty);
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
                let callee = children.first().map(element_span);
                let text = callee
                    .and_then(|span| self.source.slice(&span))
                    .unwrap_or_default();
                let reference = callee.and_then(|span| self.references.get(&span.byte_start));
                let global =
                    reference.is_some_and(|&at| self.bindings.references[at].declaration.is_none());
                match (global, text) {
                    (true, "JSON.parse") => Some(TypeId::UNKNOWN),
                    (true, "Object.keys" | "Object.entries") => {
                        let argument = children.get(2).copied().filter(|_| children.len() == 4);
                        let argument = argument.and_then(|argument| self.type_of(argument));
                        argument.and_then(|shape| self.keys_of(shape, text == "Object.entries"))
                    }
                    (true, name) if name.starts_with("Promise.") => {
                        self.promise_call(&name["Promise.".len()..], node, &children)
                    }
                    (true, "Iterator.from" | "Array.from") => {
                        let argument = children.get(2).copied().filter(|_| children.len() == 4);
                        let argument = argument.and_then(|argument| self.type_of(argument));
                        let item = argument.and_then(|iterable| self.item_of(iterable, false))?;
                        // What is taken out of a tuple is no literal any more.
                        let item = self.widened(item);
                        Some(if text == "Array.from" {
                            self.types.intern(Type::Array {
                                element: item,
                                mutated: false,
                            })
                        } else {
                            self.iterator(item, false)
                        })
                    }
                    _ => match self.helped(&children[0]) {
                        Some(helped) => self.helper_call(&helped, &children),
                        None => self.call(node, &children),
                    },
                }
            }
            NodeKind::ArrowFunction => self.arrow(node),
            NodeKind::NewExpr => self.call(node, &children),
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
            NodeKind::AwaitExpr => self.await_of(node, &children),
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

    /// Pushes a finding with related locations in this file.
    pub(crate) fn report_related(
        &mut self,
        code: Code,
        span: Span,
        message: String,
        related: &[(Span, String)],
    ) {
        let mut found = Diagnostic::new(code, self.source.path(), span, message);
        for (span, note) in related {
            found = found.with_related(self.source.path(), *span, note.clone());
        }
        self.diagnostics.push(found);
    }

    /// Pushes a finding about operands, the first time: when an operand is a
    /// parameter, each call that gave it a part of its type is a related location,
    /// or what its arrow is expected to be.
    pub(crate) fn report_about(
        &mut self,
        first: bool,
        code: Code,
        span: Span,
        message: String,
        operands: &[Element<'_>],
    ) {
        if !first {
            return;
        }
        let mut found = Diagnostic::new(code, self.source.path(), span, message);
        for operand in operands {
            let start = element_span(operand).byte_start;
            let reference = self.references.get(&start);
            let declaration = reference.and_then(|&at| self.bindings.references[at].declaration);
            let sites = declaration.and_then(|declaration| self.facts.sites.get(&declaration));
            for site in sites.into_iter().flatten() {
                let name = |class: ClassId| self.class_name(class);
                let note = format!(
                    "passed a `{}` here",
                    Printer::new(self.types, &name).print(site.ty)
                );
                found = found.with_related(site.path.clone(), site.span, note);
            }
            let origins = declaration.and_then(|declaration| self.origins.get(&declaration));
            for (span, note) in origins.into_iter().flatten() {
                found = found.with_related(self.source.path(), *span, note.clone());
            }
        }
        self.diagnostics.push(found);
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
                Type::Class(class, arguments) => {
                    let (class, arguments) = (*class, arguments.clone());
                    let fields = self.facts.fields.get(&class)?.clone();
                    let params = self.facts.classes.get(&class);
                    let params = params.map(|shape| shape.params.clone());
                    let params = params.unwrap_or_default().into_iter();
                    let bound: BTreeMap<VarId, TypeId> = params.zip(arguments).collect();
                    // What is private stays where it is.
                    let public = fields.into_iter().filter(|(key, _)| !key.starts_with('#'));
                    let mut spread = Vec::new();
                    for (key, ty) in public {
                        spread.push(Property {
                            key,
                            ty: self.substituted(ty, &bound),
                            literal: false,
                        });
                    }
                    spread
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

    /// Whether a value of this type may fill a template: a string, a number, a bigint,
    /// a boolean, a union of those, or an instance of a class whose `toString()` is
    /// seen to return a string.
    fn is_text(&mut self, id: TypeId) -> bool {
        match self.types.get(id).clone() {
            Type::String
            | Type::Number
            | Type::Int32
            | Type::BigInt
            | Type::Boolean
            | Type::Literal(_) => true,
            Type::Union(members) => members.iter().all(|&member| self.is_text(member)),
            Type::Class(class, _) => {
                let is_string = |typer: &Self, ty: TypeId| {
                    matches!(
                        typer.types.get(ty),
                        Type::String | Type::Literal(Literal::String(_))
                    )
                };
                let told = self.facts.to_string.get(&class).copied();
                let method = self.method_of(id, "toString");
                let gives = method.is_some_and(|method| {
                    matches!(self.types.get(method), Type::Function(function) if is_string(self, function.returns))
                });
                told.is_some_and(|told| is_string(self, told)) || gives
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
            let ty = self.type_of(*filling);
            let Some(ty) = ty.filter(|&ty| !self.is_text(ty)) else {
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
        judge.self_assignment(node, &children);
        judge.mixed_returns(node);
        judge.class_rules(node);
        judge.loop_rule(node, &children);
        judge.promise_rules(node, &children);
        let operates = matches!(
            node.kind(),
            NodeKind::BinaryExpr
                | NodeKind::UnaryExpr
                | NodeKind::CondExpr
                | NodeKind::CallExpr
                | NodeKind::NewExpr
                | NodeKind::ArrowFunction
                | NodeKind::AwaitExpr
        );
        if operates {
            judge.type_of(Element::Node(node));
        }
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
    for &parameter in &facts.unconstrained {
        let declared = &bindings.declarations[parameter];
        let message = format!(
            "nothing says what `{}` is: the body does nothing with it that tells, and nothing calls the function (SPEC.md §4.4)",
            declared.name
        );
        found.push(Diagnostic::new(
            Code::E112,
            source.path(),
            declared.span,
            message,
        ));
    }
    found.extend(judge.finish().iter().cloned());
    found
}
