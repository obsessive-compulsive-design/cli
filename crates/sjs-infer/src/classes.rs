//! Classes (SPEC.md §4.5). A class is typed member by member: a field is the union of
//! its initializer and of everything the class body assigns to it; a method, an
//! accessor and the constructor are functions, typed from their bodies and from their
//! calls (`o.m(…)`, `o.k = v` for a setter, `new C(…)` and `super(…)` for the
//! constructor). The class as a value has the type of its constructor, which gives an
//! instance; a constructor parameter the class treats opaquely is a type parameter of
//! the class (`Box<T>`). The static members are the members of a second class, the
//! static side, which `C.name` and a static `this` read.
//!
//! Judged here: a field without an initializer that the constructor does not assign,
//! straight down its body (E114); what is extended and is no class, a method that
//! does not go where the one it overrides does, a setter that does not take what its
//! getter gives (E138).

use crate::expressions::{nodes, punct, ClassFacts, Facts, Typer};
use crate::functions::{parameters_of, places, Call, Declared, Flavor};
use crate::members::Owner;
use crate::mutation::arguments;
use sjs_bind::{Bindings, DeclKind};
use sjs_diag::{Code, SourceFile, Span};
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{ClassId, Function, Type, TypeId, VarId};
use std::collections::{BTreeMap, BTreeSet};

/// What a class extends when that is no class of the program: nothing is known of it.
pub(crate) const UNKNOWN_BASE: ClassId = ClassId(u32::MAX - 1);

/// The static side of a class: the class its static members are members of.
pub(crate) fn statics_of(class: ClassId) -> ClassId {
    ClassId(class.0 + 1)
}

/// What a member of a class is.
pub(crate) enum What<'t> {
    /// A field, with its initializer.
    Field(Option<Element<'t>>),
    /// A method, with the declaration of its first parameter.
    Method(Option<usize>),
    Getter,
    /// A setter, with the declaration of its parameter.
    Setter(Option<usize>),
}

/// A member of a class that has a name.
pub(crate) struct Member<'t> {
    /// The class it is a member of: the class, or its static side.
    pub side: ClassId,
    /// Its name, a private one with its `#`.
    pub name: String,
    pub what: What<'t>,
    /// What stands for it among the declarations: an index past the last of them.
    pub key: usize,
}

/// A class, as it is written.
pub(crate) struct Class<'t> {
    /// The declaration of its name.
    pub declaration: usize,
    pub id: ClassId,
    pub name: String,
    /// Whether it extends anything.
    pub extends: bool,
    /// The declaration of what it extends, when that is a name of the file.
    pub base: Option<usize>,
    /// Whether it has a constructor of its own.
    pub constructs: bool,
    /// What its constructor passes to `super(…)`.
    pub passed_up: Vec<Element<'t>>,
    pub members: Vec<Member<'t>>,
    /// Every `this.name = value` of its body, and `Class.name = value`: the side, the
    /// name, the value.
    pub assigned: Vec<(ClassId, String, Element<'t>)>,
}

/// The classes of a file, with what stands for their functions.
pub(crate) struct Collected<'t> {
    pub classes: Vec<Class<'t>>,
    /// The constructors, by the declaration of their class; the methods and the
    /// accessors, by their key.
    pub functions: Vec<Declared<'t>>,
    /// The `super(…)` calls: calls of the class extended.
    pub calls: Vec<Call<'t>>,
    /// The key of each method, by its class and name; of each setter, by its class
    /// and `set name`.
    pub keys: BTreeMap<(ClassId, String), usize>,
    /// One past the last key.
    pub extent: usize,
}

/// How a member with a name is written.
pub(crate) struct Written {
    pub name: String,
    /// Where the name is.
    pub at: Span,
    /// The words before the name: `static`, `async`, `*`, `get`, `set`.
    pub before: Vec<String>,
}

/// The tokens a member begins with, up to its parameters or its value.
pub(crate) fn leading(member: NodeRef<'_>) -> Vec<Span> {
    let tokens = member.children().map_while(|child| match child {
        Element::Token(token) => Some(token.token()),
        Element::Node(_) => None,
    });
    let words = tokens.filter(|token| token.kind != TokenKind::Punct(Punct::Semicolon));
    words.map(|token| token.span).collect()
}

/// Whether a member is a static one: a static block, or one with `static` before its
/// name.
pub(crate) fn is_static(source: &SourceFile, member: NodeRef<'_>) -> bool {
    let words = leading(member);
    let first = words.first().and_then(|word| source.slice(word));
    member.kind() == NodeKind::StaticBlock || (words.len() > 1 && first == Some("static"))
}

/// How a member is written, when it has a name: not a static block, nor a member
/// with a computed key that is no symbol's.
pub(crate) fn written(source: &SourceFile, member: NodeRef<'_>) -> Option<Written> {
    let mut words = leading(member);
    // `[Symbol.iterator]` is a name too, written as it is without its spaces.
    let symbol = nodes(member).find(|part| part.kind() == NodeKind::SymbolKey);
    words.extend(symbol.map(NodeRef::span));
    let text = |span: &Span| {
        let text = source.slice(span).unwrap_or_default();
        text.split_whitespace().collect::<String>()
    };
    let at = words.pop().unwrap_or(member.span());
    let name = text(&at);
    let named = member.kind() != NodeKind::StaticBlock && name != "[" && name != "]";
    named.then(|| Written {
        name,
        at,
        before: words.iter().map(text).collect(),
    })
}

/// Whether a member is the constructor of its class.
pub(crate) fn is_constructor(source: &SourceFile, member: NodeRef<'_>) -> bool {
    let words = leading(member);
    let name = words.first().and_then(|word| source.slice(word));
    member.kind() == NodeKind::ClassMethod && words.len() == 1 && name == Some("constructor")
}

/// The key of `this.key`, when `element` is that.
pub(crate) fn this_key(element: &Element<'_>) -> Option<Span> {
    let Element::Node(target) = element else {
        return None;
    };
    let parts: Vec<Element<'_>> = target.children().collect();
    match (target.kind(), parts.as_slice()) {
        (NodeKind::MemberExpr, [Element::Token(this), _, Element::Token(key)])
            if this.token().kind == TokenKind::Keyword(Keyword::This) =>
        {
            Some(key.token().span)
        }
        _ => None,
    }
}

/// A node and every node inside it, but those of a class declared inside it.
pub(crate) fn within(node: NodeRef<'_>) -> Vec<NodeRef<'_>> {
    let mut found = Vec::new();
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        pending.extend(nodes(node).filter(|inner| inner.kind() != NodeKind::ClassDecl));
        found.push(node);
    }
    found
}

/// Where names are, for [`classes_in`].
struct Names<'a> {
    source: &'a SourceFile,
    bindings: &'a Bindings,
    declared_at: BTreeMap<usize, usize>,
    referenced_at: BTreeMap<usize, usize>,
}

impl Names<'_> {
    /// The declaration a name refers to, when `element` is a name.
    fn named(&self, element: Option<&Element<'_>>) -> Option<usize> {
        let Some(Element::Token(name)) = element else {
            return None;
        };
        let reference = self.referenced_at.get(&name.token().span.byte_start);
        reference.and_then(|&at| self.bindings.references[at].declaration)
    }

    /// Records what a member of `class` assigns to the fields and passes to
    /// `super(…)`.
    fn uses<'t>(
        &self,
        member: NodeRef<'t>,
        side: ClassId,
        class: &mut Class<'t>,
        calls: &mut Vec<Call<'t>>,
    ) {
        for inner in within(member) {
            let parts: Vec<Element<'t>> = inner.children().collect();
            let is_super = matches!(parts.first(), Some(Element::Token(token)) if token.token().kind == TokenKind::Keyword(Keyword::Super));
            match inner.kind() {
                NodeKind::AssignExpr if punct(parts.get(1)) == Some(Punct::Eq) => {
                    let own = this_key(&parts[0]).map(|key| (side, key));
                    // `Class.name = value`, for a static field.
                    let target: Vec<Element<'t>> = match parts[0] {
                        Element::Node(target) => target.children().collect(),
                        Element::Token(_) => Vec::new(),
                    };
                    let me = self.named(target.first()) == Some(class.declaration);
                    let key = match target.last() {
                        Some(Element::Token(key)) if me => Some(key.token().span),
                        _ => None,
                    };
                    let of_class = key.map(|key| (statics_of(class.id), key));
                    let value = parts.last().copied();
                    if let Some(((side, key), value)) = own.or(of_class).zip(value) {
                        let name = self.source.slice(&key).unwrap_or_default().to_owned();
                        class.assigned.push((side, name, value));
                    }
                }
                NodeKind::CallExpr if is_super => {
                    class.passed_up = arguments(&parts);
                    calls.extend(class.base.map(|callee| Call {
                        callee,
                        passed: arguments(&parts),
                    }));
                }
                _ => {}
            }
        }
    }

    /// Records one member of a class: what it is, the function that stands for it,
    /// and what it assigns and passes up.
    fn member<'t>(&self, member: NodeRef<'t>, class: &mut Class<'t>, found: &mut Collected<'t>) {
        let side = if is_static(self.source, member) {
            statics_of(class.id)
        } else {
            class.id
        };
        self.uses(member, side, class, &mut found.calls);
        let Some(written) = written(self.source, member) else {
            return;
        };
        let has = |word: &str| written.before.iter().any(|before| before == word);
        let constructor = is_constructor(self.source, member);
        let key = if constructor {
            class.declaration
        } else {
            found.extent
        };
        let parameters = parameters_of(member, &self.declared_at);
        let taken = parameters.first().and_then(|first| first.declaration);
        let what = match member.kind() {
            NodeKind::ClassField => {
                let value = nodes(member).find(|inner| inner.kind() == NodeKind::Initializer);
                Some(What::Field(value.and_then(|value| value.children().last())))
            }
            NodeKind::ClassMethod if constructor => None,
            NodeKind::ClassMethod => Some(What::Method(taken)),
            _ if has("get") => Some(What::Getter),
            _ => Some(What::Setter(taken)),
        };
        class.constructs |= constructor;
        let is_field = member.kind() == NodeKind::ClassField;
        if let Some(body) = member.children().last().filter(|_| !is_field) {
            found.functions.push(Declared {
                name: key,
                parameters,
                body,
                flavor: match (has("*"), has("async")) {
                    (true, false) => Flavor::Generator,
                    (true, true) => Flavor::AsyncGenerator,
                    (false, true) => Flavor::Async,
                    (false, false) => Flavor::Plain,
                },
                node: member,
                // A setter takes what is assigned, whatever its body does with it.
                generalizes: !has("set"),
                // An instance goes where the calls are not seen; a method that
                // is private is called in the class alone.
                exported: !written.name.starts_with('#') || has("set"),
                // The constructor makes the type parameters of its class.
                class: (side == class.id && !constructor).then_some(class.id),
            });
        }
        let Some(what) = what else {
            return;
        };
        found.extent += 1;
        let method = match what {
            What::Method(_) => Some(written.name.clone()),
            What::Setter(_) => Some(format!("set {}", written.name)),
            What::Field(_) | What::Getter => None,
        };
        found
            .keys
            .extend(method.map(|method| ((side, method), key)));
        class.members.push(Member {
            side,
            name: written.name,
            what,
            key,
        });
    }
}

/// The classes a file declares: `class_of` says which class each declaration names.
pub(crate) fn classes_in<'t>(
    source: &SourceFile,
    tree: &'t SyntaxTree,
    bindings: &Bindings,
    class_of: &BTreeMap<usize, ClassId>,
) -> Collected<'t> {
    let declarations = bindings.declarations.iter();
    let references = bindings.references.iter();
    let names = Names {
        source,
        bindings,
        declared_at: places(declarations.map(|d| d.span.byte_start)),
        referenced_at: places(references.map(|r| r.span.byte_start)),
    };
    let mut found = Collected {
        classes: Vec::new(),
        functions: Vec::new(),
        calls: Vec::new(),
        keys: BTreeMap::new(),
        extent: bindings.declarations.len(),
    };
    let mut pending = vec![tree.root()];
    while let Some(node) = pending.pop() {
        pending.extend(nodes(node));
        let declares = |child: Element<'t>| match child {
            Element::Token(name) => names
                .declared_at
                .get(&name.token().span.byte_start)
                .copied(),
            Element::Node(_) => None,
        };
        // A `new` of a name is a call of what the name names.
        let made = Some(node).filter(|node| node.kind() == NodeKind::NewExpr);
        let parts: Vec<Element<'t>> = made.into_iter().flat_map(NodeRef::children).collect();
        let callee = names.named(parts.get(1));
        found.calls.extend(callee.map(|callee| Call {
            callee,
            passed: arguments(&parts),
        }));
        let name = node.children().find_map(declares);
        let class = name.and_then(|name| class_of.get(&name).map(|id| (name, *id)));
        let body = nodes(node).find(|part| part.kind() == NodeKind::ClassBody);
        let is_class = node.kind() == NodeKind::ClassDecl;
        let Some(((declaration, id), body)) = class.zip(body).filter(|_| is_class) else {
            continue;
        };
        let heritage = nodes(node).find(|part| part.kind() == NodeKind::ClassHeritage);
        let base = heritage.and_then(|heritage| names.named(heritage.children().last().as_ref()));
        let mut class = Class {
            declaration,
            id,
            name: bindings.declarations[declaration].name.clone(),
            extends: heritage.is_some(),
            base,
            constructs: false,
            passed_up: Vec::new(),
            members: Vec::new(),
            assigned: Vec::new(),
        };
        for member in nodes(body) {
            names.member(member, &mut class, &mut found);
        }
        found.classes.push(class);
    }
    found
}

impl Typer<'_> {
    /// The class a class declaration declares.
    fn class_named(&self, class: NodeRef<'_>) -> Option<ClassId> {
        let name = class.children().find_map(|child| match child {
            Element::Token(name) => {
                let start = name.token().span.byte_start;
                let mut declarations = self.bindings.declarations.iter();
                declarations.position(|d| d.span.byte_start == start)
            }
            Element::Node(_) => None,
        });
        name.and_then(|name| self.facts.class_of.get(&name).copied())
    }

    /// The class `this` is an instance of at `from`: the class of the member it is
    /// in, or the static side of that class in a static member.
    pub(crate) fn side_of(&self, from: NodeRef<'_>) -> Option<ClassId> {
        let is_member = |node: &NodeRef<'_>| {
            let parent = node.parent();
            parent.is_some_and(|parent| parent.kind() == NodeKind::ClassBody)
        };
        let member = std::iter::successors(Some(from), |inner| inner.parent()).find(is_member)?;
        let class = member.parent().and_then(NodeRef::parent);
        let id = class.and_then(|class| self.class_named(class))?;
        Some(if is_static(self.source, member) {
            statics_of(id)
        } else {
            id
        })
    }

    /// The type of an instance of a class within the class: with its type
    /// parameters as they are.
    pub(crate) fn instance(&mut self, class: ClassId) -> TypeId {
        let shape = self.facts.classes.get(&class);
        let params = shape.map(|shape| shape.params.clone());
        let params = params.unwrap_or_default().into_iter();
        let arguments = params
            .map(|param| self.types.intern(Type::Var(param)))
            .collect();
        self.types.intern(Type::Class(class, arguments))
    }

    /// The type of `super` at `from`: an instance of the class extended.
    pub(crate) fn base_of(&mut self, from: NodeRef<'_>) -> Option<TypeId> {
        let side = self.side_of(from)?;
        let base = self.facts.superclass.get(&side).copied()?;
        let shape = self.facts.classes.get(&side);
        let arguments = shape.and_then(|shape| shape.base_arguments.clone())?;
        Some(self.types.intern(Type::Class(base, arguments)))
    }

    /// Whether a value of the type `ty` is a class, or holds one: a function that
    /// gives an instance.
    pub(crate) fn is_class(&self, ty: TypeId) -> bool {
        let makes = |member: &TypeId| match self.types.get(*member) {
            Type::Function(function) => {
                matches!(self.types.get(function.returns), Type::Class(..))
            }
            _ => false,
        };
        self.members_of(ty).iter().all(makes)
    }

    /// The type of what a call calls: for `super(…)`, the constructor of the class
    /// extended.
    pub(crate) fn called_type(&mut self, callee: &Element<'_>) -> Option<TypeId> {
        match callee {
            Element::Token(token) if token.token().kind == TokenKind::Keyword(Keyword::Super) => {
                let side = self.side_of(token.parent())?;
                let base = self.facts.superclass.get(&side)?;
                self.facts.classes.get(base)?.constructor
            }
            _ => self.type_of(*callee),
        }
    }

    /// The calls of the methods and the setters: `o.m(…)` and `o.k = v` where `o` is
    /// an instance of a class that has `m` as a method, `k` as a setter. `keys` says
    /// which is which.
    pub(crate) fn member_calls<'t>(
        &mut self,
        root: NodeRef<'t>,
        keys: &BTreeMap<(ClassId, String), usize>,
    ) -> Vec<Call<'t>> {
        let mut calls = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            pending.extend(nodes(node));
            let parts: Vec<Element<'t>> = node.children().collect();
            let assigns =
                node.kind() == NodeKind::AssignExpr && punct(parts.get(1)) == Some(Punct::Eq);
            let passed = if node.kind() == NodeKind::CallExpr {
                arguments(&parts)
            } else {
                parts.last().copied().into_iter().collect()
            };
            let Some(Element::Node(target)) = parts.first().copied() else {
                continue;
            };
            let calls_it = node.kind() == NodeKind::CallExpr || assigns;
            if target.kind() != NodeKind::MemberExpr || !calls_it {
                continue;
            }
            let (key, _) = self.key_of(target);
            let Some((receiver, _)) = self.receiver(target, false) else {
                continue;
            };
            for member in self.members_of(receiver) {
                let Type::Class(class, arguments) = self.types.get(member).clone() else {
                    continue;
                };
                let method = if assigns {
                    format!("set {key}")
                } else {
                    key.clone()
                };
                if let Owner::At(owner, _) = self.owner(class, &arguments, &key) {
                    let callee = keys.get(&(owner, method)).copied();
                    calls.extend(callee.map(|callee| Call {
                        callee,
                        passed: passed.clone(),
                    }));
                }
            }
        }
        calls
    }

    /// Works out the classes for a round, from `known` and from the functions of
    /// this round in `next`, into `next`: the members of each, its type parameters,
    /// and the type of its constructor, which is the type of the class as a value.
    pub(crate) fn classes(
        &mut self,
        classes: &[Class<'_>],
        (known, unknown): (&Facts, &BTreeSet<usize>),
        (next, partial): (&mut Facts, &mut BTreeSet<usize>),
    ) {
        for class in classes {
            let statics = statics_of(class.id);
            let sides = [
                (class.id, class.name.clone()),
                (statics, format!("typeof {}", class.name)),
            ];
            for (side, name) in sides {
                next.fields.insert(side, BTreeMap::new());
                next.methods.insert(side, BTreeSet::new());
                next.class_names.insert(side, name);
                let shape = ClassFacts {
                    base_arguments: Some(Vec::new()),
                    ..ClassFacts::default()
                };
                next.classes.insert(side, shape);
            }
            let base = class.extends.then(|| {
                let base = class.base.and_then(|base| known.class_of.get(&base));
                base.copied().unwrap_or(UNKNOWN_BASE)
            });
            if let Some(base) = base {
                next.superclass.insert(class.id, base);
                next.superclass.insert(statics, statics_of(base));
            }
            for member in &class.members {
                self.member(class, member, unknown, next, partial);
            }
            self.constructor(class, base, known, next);
        }
    }

    /// Works out one member of `class` into `next`.
    fn member(
        &mut self,
        class: &Class<'_>,
        member: &Member<'_>,
        unknown: &BTreeSet<usize>,
        next: &mut Facts,
        partial: &mut BTreeSet<usize>,
    ) {
        let name = member.name.clone();
        let shape = next.classes.entry(member.side).or_default();
        shape.declared.insert(name.clone());
        match &member.what {
            What::Field(value) => {
                let assigned = class.assigned.iter();
                let mine = assigned.filter(|(side, to, _)| *side == member.side && *to == name);
                let values = value
                    .iter()
                    .copied()
                    .chain(mine.map(|(_, _, value)| *value));
                let types: Vec<Option<TypeId>> = values
                    .map(|value| self.type_of(value).map(|ty| self.loosened(ty)))
                    .collect();
                // From the values that have a type so far: one that has none yet
                // may be read from the field itself.
                if types.contains(&None) {
                    partial.insert(member.key);
                }
                let mut parts: Vec<TypeId> = types.into_iter().flatten().collect();
                // A static field with no value holds `undefined` until it is given one.
                let bare = value.is_none() && member.side != class.id;
                parts.extend(bare.then_some(TypeId::UNDEFINED));
                if !parts.is_empty() && !unknown.contains(&member.key) {
                    let ty = self.types.union(parts);
                    next.declarations.insert(member.key, ty);
                    let fields = next.fields.entry(member.side).or_default();
                    fields.insert(name, ty);
                }
            }
            What::Method(taken) => {
                // What `then` passes on is what an `await` of an instance gives.
                let passes = taken.filter(|_| name == "then");
                let calls = passes.and_then(|taken| self.called.get(&taken).cloned());
                let mut given = Vec::new();
                for call in calls.unwrap_or_default() {
                    let parts: Vec<Element<'_>> = call.children().collect();
                    let value = arguments(&parts).first().copied();
                    let ty = match value {
                        Some(value) => self.type_of(value).map(|ty| self.loosened(ty)),
                        None => Some(TypeId::UNDEFINED),
                    };
                    given.extend(ty);
                }
                let shape = next.classes.entry(member.side).or_default();
                shape.resolves = (!given.is_empty()).then(|| self.types.union(given));
                let signature = next.declarations.get(&member.key).copied();
                shape
                    .signatures
                    .extend(signature.map(|signature| (name.clone(), signature)));
                next.methods.entry(member.side).or_default().insert(name);
            }
            What::Getter => {
                shape
                    .getters
                    .insert(name, next.returns.get(&member.key).copied());
            }
            What::Setter(taken) => {
                let taken = taken.and_then(|taken| next.declarations.get(&taken).copied());
                shape.setters.insert(name, taken);
            }
        }
    }

    /// What the class `base` is given for its type parameters by `class`, which
    /// extends it: by what the constructor passes to `super(…)`, or the parameters
    /// themselves when the class has no constructor of its own. `None` when what a
    /// type parameter is cannot be told.
    fn base_arguments(
        &mut self,
        class: &Class<'_>,
        base: ClassId,
        known: &Facts,
    ) -> Option<Vec<TypeId>> {
        let shape = known.classes.get(&base);
        let params = shape.map(|shape| shape.params.clone()).unwrap_or_default();
        if !class.constructs {
            let variable = |param: &VarId| self.types.intern(Type::Var(*param));
            return Some(params.iter().map(variable).collect());
        }
        let constructor = shape.and_then(|shape| shape.constructor);
        let mut taken = Vec::new();
        if let Some(Type::Function(function)) = constructor.map(|made| self.types.get(made)) {
            taken = function.params.iter().map(|param| param.ty).collect();
        }
        let mut pairs = Vec::new();
        for (taken, passed) in taken.into_iter().zip(&class.passed_up) {
            let given = self.type_of(*passed);
            pairs.extend(given.map(|given| (taken, given)));
        }
        let bound = self.bound(&pairs);
        params
            .iter()
            .map(|param| bound.get(param).copied())
            .collect()
    }

    /// Works out the constructor of `class` into `next`, and with it the type
    /// parameters of the class: those of the constructor that a member holds.
    fn constructor(
        &mut self,
        class: &Class<'_>,
        base: Option<ClassId>,
        known: &Facts,
        next: &mut Facts,
    ) {
        let nothing = Type::Function(Function {
            type_params: Vec::new(),
            params: Vec::new(),
            rest: None,
            returns: TypeId::UNDEFINED,
        });
        // Without one of its own it is constructed as what it extends is.
        let written = match (class.constructs, base) {
            (true, _) => next.declarations.get(&class.declaration).copied(),
            (false, Some(base)) => known.classes.get(&base).and_then(|base| base.constructor),
            (false, None) => Some(self.types.intern(nothing)),
        };
        let mut function = None;
        if let Some(Type::Function(found)) = written.map(|written| self.types.get(written)) {
            function = Some(found.clone());
        }
        let base_arguments = match base {
            Some(base) => self.base_arguments(class, base, known),
            None => Some(Vec::new()),
        };
        let shape = next.classes.entry(class.id).or_default();
        shape.base_arguments.clone_from(&base_arguments);
        let Some(function) = function else {
            return;
        };
        let fields = next.fields.get(&class.id);
        let mut held: Vec<TypeId> = fields
            .into_iter()
            .flat_map(BTreeMap::values)
            .copied()
            .collect();
        held.extend(shape.getters.values().copied().flatten());
        held.extend(shape.setters.values().copied().flatten());
        held.extend(shape.signatures.values().copied());
        held.extend(base_arguments.into_iter().flatten());
        let mut mentioned = Vec::new();
        for ty in held {
            self.variables(ty, &mut mentioned);
        }
        let params = function.type_params.iter().copied();
        let params: Vec<VarId> = params.filter(|param| mentioned.contains(param)).collect();
        let arguments = params.iter();
        let arguments = arguments.map(|param| self.types.intern(Type::Var(*param)));
        let instance = Type::Class(class.id, arguments.collect());
        let instance = self.types.intern(instance);
        let made = self.types.intern(Type::Function(Function {
            returns: instance,
            ..function
        }));
        next.declarations.insert(class.declaration, made);
        next.returns.insert(class.declaration, instance);
        shape.params = params;
        shape.constructor = Some(made);
    }

    /// Judges a class declaration: what it extends (E138), its fields (E114), its
    /// methods against those they override and its setters against their getters
    /// (E138).
    pub(crate) fn class_rules(&mut self, node: NodeRef<'_>) {
        let class = Some(node).filter(|node| node.kind() == NodeKind::ClassDecl);
        let Some(id) = class.and_then(|class| self.class_named(class)) else {
            return;
        };
        let first = self.first_time(node);
        for part in nodes(node) {
            if part.kind() == NodeKind::ClassBody {
                self.member_rules(first, id, part);
            }
            if part.kind() != NodeKind::ClassHeritage {
                continue;
            }
            // What is extended is named last: `Base`, or the `Base` of `ns.Base`.
            let base = part.tokens().last();
            let span = base.map_or(part.span(), |base| base.span);
            let reference = self.references.get(&span.byte_start);
            let declared = reference.and_then(|&at| self.bindings.references[at].declaration);
            // A function is no class, nor is anything that has a type and is not
            // known to be one.
            let no_class = declared.is_some_and(|declared| {
                let kind = self.bindings.declarations[declared].kind;
                let typed = self.facts.declarations.contains_key(&declared);
                !self.facts.class_of.contains_key(&declared)
                    && (kind == DeclKind::Function || typed)
            });
            if no_class {
                let message = format!(
                    "`{}` is extended, and is no class: a class extends a class (SPEC.md §4.5)",
                    self.source.slice(&span).unwrap_or_default()
                );
                self.report(first, Code::E138, span, message);
            }
        }
    }

    /// Judges the members of the class `id`, whose body is `body`.
    fn member_rules(&mut self, first: bool, id: ClassId, body: NodeRef<'_>) {
        let members: Vec<NodeRef<'_>> = nodes(body).collect();
        // What the constructor assigns, straight down its body.
        let source = self.source;
        let constructor = members
            .iter()
            .find(|member| is_constructor(source, **member));
        let block = constructor.and_then(|constructor| nodes(*constructor).last());
        let statements = block.into_iter().flat_map(nodes);
        let mut assigned: Vec<&str> = Vec::new();
        for statement in statements.flat_map(nodes) {
            let parts: Vec<Element<'_>> = statement.children().collect();
            let stated = statement
                .parent()
                .is_some_and(|parent| parent.kind() == NodeKind::ExprStmt);
            let assigns = stated
                && statement.kind() == NodeKind::AssignExpr
                && punct(parts.get(1)) == Some(Punct::Eq);
            let key = parts.first().filter(|_| assigns).and_then(this_key);
            assigned.extend(key.and_then(|key| source.slice(&key)));
        }
        for member in members {
            let Some(written) = written(source, member) else {
                continue;
            };
            let name = written.name.as_str();
            let side = if is_static(source, member) {
                statics_of(id)
            } else {
                id
            };
            let valued = nodes(member).any(|inner| inner.kind() == NodeKind::Initializer);
            let shape = self.facts.classes.get(&side).cloned().unwrap_or_default();
            match member.kind() {
                NodeKind::ClassField if side == id && !valued && !assigned.contains(&name) => {
                    let message = format!(
                        "the field `{name}` has no initializer, and the constructor does not assign it straight down its body: give it a value, `undefined` if it has none yet (SPEC.md §4.5)"
                    );
                    self.report(first, Code::E114, written.at, message);
                }
                NodeKind::ClassMethod => self.override_rule(first, side, &written, &shape),
                NodeKind::ClassAccessor => {
                    let taken = shape.setters.get(name).copied().flatten();
                    let given = shape.getters.get(name).copied().flatten();
                    let is_setter = written.before.iter().any(|word| word == "set");
                    let Some((taken, given)) = taken.zip(given).filter(|_| is_setter) else {
                        continue;
                    };
                    let wider = self.any_number(taken);
                    if !self.assignable(given, wider) {
                        let message = format!(
                            "the setter of `{name}` takes a `{}`, and its getter gives a `{}`: a setter takes what its getter gives (SPEC.md §4.5)",
                            self.print_type(taken),
                            self.print_type(given)
                        );
                        self.report(first, Code::E138, written.at, message);
                    }
                }
                _ => {}
            }
        }
    }

    /// E138 for a method that does not go where the method it overrides does.
    fn override_rule(&mut self, first: bool, side: ClassId, written: &Written, shape: &ClassFacts) {
        let name = written.name.as_str();
        let signature = shape.signatures.get(name).copied();
        let base = self.facts.superclass.get(&side).copied();
        let passed = shape.base_arguments.clone();
        let Some(((signature, base), passed)) = signature.zip(base).zip(passed) else {
            return;
        };
        let Owner::At(owner, bound) = self.owner(base, &passed, name) else {
            return;
        };
        let overridden = self.facts.classes.get(&owner);
        let overridden = overridden.and_then(|owner| owner.signatures.get(name).copied());
        let overridden = overridden.map(|overridden| self.substituted(overridden, &bound));
        let unfit = overridden.filter(|&overridden| {
            let (given, wanted) = self.alike(signature, overridden);
            !self.assignable(given, wanted)
        });
        if let Some(overridden) = unfit {
            let message = format!(
                "`{name}` overrides the `{name}` of `{}` and does not go where that does: it is a `{}`, and that a `{}` (SPEC.md §4.5)",
                self.class_name(owner),
                self.print_type(signature),
                self.print_type(overridden)
            );
            self.report(first, Code::E138, written.at, message);
        }
    }
}
