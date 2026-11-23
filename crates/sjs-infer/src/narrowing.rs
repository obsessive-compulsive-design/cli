//! Narrowing (SPEC.md §3.5): what a condition tells of a name or of a path of keys
//! from one (`x`, `x.kind`, `x.a.b`), along the control-flow graph of its function.
//!
//! A test is `typeof p === "kind"` (through a `const` of `typeof p` too), `p === e`
//! and `p !== e` (a discriminant when `p` ends in a key every member of the union has
//! a literal for), `p instanceof C`, `Array.isArray(p)`, `Number.isNaN(p)`,
//! `Object.hasOwn(p, "key")`, and each `case` of a `switch` on any of these. Where
//! the graph branches on a test, the type of the path is narrowed on the one edge and
//! what the test rules out is removed on the other; `!`, `&&`, `||` and `?:` are
//! branches of the graph already. Where edges meet, what each brings is united.
//!
//! What undoes a narrowing: an assignment to the name, or to anything reached from
//! it; a declaration running again (a loop's); a call, for a `let` that a closure
//! assigns, and for a path of keys from an object the program mutates.

use crate::expressions::{nodes, punct, Typer};
use crate::mutation::arguments;
use crate::typed_arrays::ambient_class;
use sjs_bind::{body_of, running, Bindings, Cfg, ENTRY};
use sjs_diag::SourceFile;
use sjs_syntax::{Element, Keyword, NodeKind, NodeRef, Punct, SyntaxTree, TokenKind};
use sjs_types::{ClassId, Literal, Property, Type, TypeId};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

/// A name, by its declaration, and the keys read from it: `""`, `".kind"`, `".a.b"`.
pub(crate) type Path = (usize, String);

/// The bytes of a stretch of the text.
type Range = (usize, usize);

/// What a test asks of a path.
#[derive(Clone)]
enum Check<'t> {
    /// `typeof p === "kind"`.
    Typeof(String),
    /// `p === e`, or `p === -e` in a `case`.
    Equals(Element<'t>, bool),
    /// `p.key === e`, of `p`.
    Discriminant(String, Element<'t>, bool),
    /// `p instanceof C`.
    Instance(Element<'t>),
    /// `Object.hasOwn(p, "key")`.
    HasOwn(String),
    /// `Array.isArray(p)`.
    IsArray,
    /// `Number.isNaN(p)`.
    IsNan,
}

/// A test of a path: what holds of it when the test does, or when it does not
/// (`!==`).
#[derive(Clone)]
struct Test<'t> {
    path: Path,
    check: Check<'t>,
    negated: bool,
}

/// The graph of a function, with what each node undoes.
struct Flow {
    cfg: Cfg,
    /// The edges that leave each node: where to, and how.
    leaving: Vec<Vec<(usize, &'static str)>>,
    /// The nodes that call something: other code runs there.
    calls: Vec<bool>,
    /// The declarations each node assigns to, or declares.
    kills: Vec<BTreeSet<usize>>,
}

/// What reaches each node of a graph: the type of one path there, `None` where
/// control does not come.
type States = Vec<Option<TypeId>>;

/// What the conditions of a file tell, and the flows worked out so far.
pub(crate) struct Narrowing<'t> {
    /// The tests, by the bytes of the expression that is each.
    tests: BTreeMap<Range, Vec<Test<'t>>>,
    /// The expressions that are the value of a `case`: the edge without a label
    /// from one is where the value was not matched.
    cases: BTreeSet<Range>,
    /// The paths some test is of.
    tested: BTreeSet<Path>,
    /// The declarations assigned to anywhere.
    assigned: BTreeSet<usize>,
    /// Those assigned to in a function other than the one that declares them.
    shared: BTreeSet<usize>,
    flows: BTreeMap<Range, Rc<Flow>>,
    /// What reaches each node, by function, path and the type before narrowing;
    /// `None` while it is being worked out.
    states: BTreeMap<(Range, Path, TypeId), Option<Rc<States>>>,
}

/// What resolves the names of a file.
struct Names<'a> {
    source: &'a SourceFile,
    bindings: &'a Bindings,
    references: &'a BTreeMap<usize, usize>,
}

fn range_of(node: NodeRef<'_>) -> Range {
    let span = node.span();
    (span.byte_start, span.byte_end)
}

/// What `typeof` is applied to, when an expression is `typeof e`.
fn typeof_operand<'t>(element: &Element<'t>) -> Option<Element<'t>> {
    let Element::Node(node) = element else {
        return None;
    };
    let operator = node.tokens().first().map(|operator| &operator.kind);
    let is_typeof = node.kind() == NodeKind::UnaryExpr
        && operator == Some(&TokenKind::Keyword(Keyword::Typeof));
    node.children().last().filter(|_| is_typeof)
}

impl Names<'_> {
    /// The declaration a name refers to.
    fn declared(&self, start: usize) -> Option<usize> {
        let reference = self.references.get(&start);
        reference.and_then(|&at| self.bindings.references[at].declaration)
    }

    /// The path an expression is: a name, or keys read from one by `.`.
    fn path(&self, element: &Element<'_>) -> Option<Path> {
        let node = match element {
            Element::Token(name) => {
                let declared = self.declared(name.token().span.byte_start);
                return declared.map(|declared| (declared, String::new()));
            }
            Element::Node(node) => node,
        };
        let parts: Vec<Element<'_>> = node.children().collect();
        match (node.kind(), parts.as_slice()) {
            (NodeKind::ParenExpr, [_, inside, _]) => self.path(inside),
            // `a?.b` is read in an optional chain, which is no path.
            (NodeKind::MemberExpr, [object, _, Element::Token(key)]) => {
                let (root, keys) = self.path(object)?;
                let key = self.source.slice(&key.token().span).unwrap_or_default();
                Some((root, format!("{keys}.{key}")))
            }
            _ => None,
        }
    }

    /// The declaration a place is reached from: the first name written in it.
    fn root(&self, place: NodeRef<'_>) -> Option<usize> {
        let mut names = place.tokens().iter();
        let first = names.find(|token| token.kind == TokenKind::Ident);
        first.and_then(|first| self.declared(first.span.byte_start))
    }
}

/// What builds the tests of a file.
struct Reader<'a, 't> {
    names: Names<'a>,
    /// What each `const` is declared with, by its declaration.
    consts: BTreeMap<usize, Element<'t>>,
    assigned: BTreeSet<usize>,
}

impl<'t> Reader<'_, 't> {
    /// The path `typeof` is applied to in `element`: written there, or in the
    /// `const` it names, when nothing is assigned to that path since.
    fn typeof_target(&self, element: &Element<'t>) -> Option<Path> {
        if let Some(operand) = typeof_operand(element) {
            return self.names.path(&operand);
        }
        let Element::Token(name) = element else {
            return None;
        };
        let declared = self.names.declared(name.token().span.byte_start);
        let value = declared.and_then(|declared| self.consts.get(&declared));
        let path = value.and_then(typeof_operand);
        let path = path.and_then(|operand| self.names.path(&operand));
        path.filter(|(root, keys)| keys.is_empty() && !self.assigned.contains(root))
    }

    /// The tests `a === b` is, or `a !== b`; `a === -b` for a `case` with a sign.
    fn equality(
        &self,
        a: &Element<'t>,
        b: &Element<'t>,
        negated: bool,
        minus: bool,
    ) -> Vec<Test<'t>> {
        let kind = match b {
            Element::Token(text) => match &text.token().kind {
                TokenKind::String(value) => Some(value.to_string()),
                _ => None,
            },
            Element::Node(_) => None,
        };
        if let Some((path, kind)) = self.typeof_target(a).zip(kind) {
            let check = Check::Typeof(kind);
            return vec![Test {
                path,
                check,
                negated,
            }];
        }
        let mut tests = Vec::new();
        for (tested, other) in [(a, b), (b, a)] {
            let Some(path) = self.names.path(tested) else {
                continue;
            };
            // What the last key is compared with tells which member the object is.
            if let Some((object, key)) = path.1.rsplit_once('.') {
                tests.push(Test {
                    path: (path.0, object.to_owned()),
                    check: Check::Discriminant(key.to_owned(), *other, minus),
                    negated,
                });
            }
            tests.push(Test {
                path,
                check: Check::Equals(*other, minus),
                negated,
            });
        }
        tests
    }

    /// The tests a binary expression is.
    fn binary(&self, node: NodeRef<'t>) -> Vec<Test<'t>> {
        let parts: Vec<Element<'t>> = node.children().collect();
        let last = parts.len() - 1;
        let (a, operator, b) = (parts[0], parts[last / 2], parts[last]);
        match punct(Some(&operator)) {
            Some(Punct::EqEqEq) => self.equality(&a, &b, false, false),
            Some(Punct::BangEqEq) => self.equality(&a, &b, true, false),
            Some(_) => Vec::new(),
            // The one operator that is a word: `instanceof`.
            None => {
                let path = self.names.path(&a);
                let test = path.map(|path| Test {
                    path,
                    check: Check::Instance(b),
                    negated: false,
                });
                test.into_iter().collect()
            }
        }
    }

    /// The test a call is: of a static of a global that tells what its argument is.
    fn call(&self, node: NodeRef<'t>) -> Vec<Test<'t>> {
        let parts: Vec<Element<'t>> = node.children().collect();
        let passed = arguments(&parts);
        let callee = node.tokens().first().map(|first| first.span);
        let global = callee.is_some_and(|first| {
            let known = self.names.references.contains_key(&first.byte_start);
            known && self.names.declared(first.byte_start).is_none()
        });
        let written = match parts.first() {
            Some(Element::Node(callee)) => self.names.source.slice(&callee.span()),
            _ => None,
        };
        let check = match (written.unwrap_or_default(), passed.as_slice()) {
            ("Array.isArray", [_]) => Some(Check::IsArray),
            ("Number.isNaN", [_]) => Some(Check::IsNan),
            ("Object.hasOwn", [_, Element::Token(key)]) => match &key.token().kind {
                TokenKind::String(key) => Some(Check::HasOwn(key.to_string())),
                _ => None,
            },
            _ => None,
        };
        let path = passed.first().and_then(|first| self.names.path(first));
        let test = check.zip(path).filter(|_| global);
        let test = test.map(|(check, path)| Test {
            path,
            check,
            negated: false,
        });
        test.into_iter().collect()
    }
}

impl<'t> Narrowing<'t> {
    /// Reads the tests of a file, and what it assigns to.
    pub(crate) fn new(
        source: &SourceFile,
        tree: &'t SyntaxTree,
        bindings: &Bindings,
        references: &BTreeMap<usize, usize>,
    ) -> Self {
        let declarations = bindings.declarations.iter().enumerate();
        let declared_at: BTreeMap<usize, usize> = declarations
            .map(|(at, declared)| (declared.span.byte_start, at))
            .collect();
        let mut reader = Reader {
            names: Names {
                source,
                bindings,
                references,
            },
            consts: BTreeMap::new(),
            assigned: BTreeSet::new(),
        };
        let mut shared = BTreeSet::new();
        let mut testing: Vec<NodeRef<'t>> = Vec::new();
        // Each node, with the function it is in.
        let mut pending = vec![(tree.root(), (0, usize::MAX))];
        while let Some((node, around)) = pending.pop() {
            let owner = body_of(node).map_or(around, |_| range_of(node));
            for child in node.children() {
                let name = match child {
                    Element::Node(inner) => {
                        pending.push((inner, owner));
                        continue;
                    }
                    Element::Token(name) => name.token().span.byte_start,
                };
                let reference = references.get(&name).map(|&at| &bindings.references[at]);
                let written = reference.filter(|reference| reference.write);
                if let Some(declared) = written.and_then(|reference| reference.declaration) {
                    reader.assigned.insert(declared);
                    let at = bindings.declarations[declared].span.byte_start;
                    if !(owner.0..owner.1).contains(&at) {
                        shared.insert(declared);
                    }
                }
            }
            let parts: Vec<Element<'t>> = node.children().collect();
            match (node.kind(), parts.as_slice()) {
                (NodeKind::ConstDecl, [_, Element::Token(name), Element::Node(value)]) => {
                    let declared = declared_at.get(&name.token().span.byte_start);
                    let value = value.children().last();
                    reader.consts.extend(declared.copied().zip(value));
                }
                (NodeKind::BinaryExpr | NodeKind::CallExpr | NodeKind::SwitchStmt, _) => {
                    testing.push(node);
                }
                _ => {}
            }
        }
        let mut tests: BTreeMap<Range, Vec<Test<'t>>> = BTreeMap::new();
        let mut cases = BTreeSet::new();
        for node in testing {
            let found = match node.kind() {
                NodeKind::BinaryExpr => reader.binary(node),
                NodeKind::CallExpr => reader.call(node),
                // Each `case` is the subject compared with its value.
                _ => {
                    let mut parts = node.children();
                    let subject = parts.find(|part| punct(Some(part)) == Some(Punct::LParen));
                    let subject = subject.and_then(|_| parts.next());
                    let clauses = nodes(node).filter(|part| part.kind() == NodeKind::CaseClause);
                    let labels = clauses.flat_map(nodes);
                    let labels = labels.filter(|part| part.kind() == NodeKind::CaseLabel);
                    for label in labels {
                        // `case`, the value, `:`: a literal, with its sign.
                        let written = label.tokens();
                        let value = written.get(1..written.len().saturating_sub(1));
                        let value = value.unwrap_or_default();
                        let ends = value.first().zip(value.last());
                        let parts: Vec<Element<'t>> = label.children().collect();
                        let literal = parts.iter().rev().nth(1).copied();
                        let Some(((first, last), literal)) = ends.zip(literal) else {
                            continue;
                        };
                        let at = (first.span.byte_start, last.span.byte_end);
                        let minus = first.kind == TokenKind::Punct(Punct::Minus);
                        let found = subject
                            .map(|subject| reader.equality(&subject, &literal, false, minus));
                        tests
                            .entry(at)
                            .or_default()
                            .extend(found.unwrap_or_default());
                        cases.insert(at);
                    }
                    Vec::new()
                }
            };
            if !found.is_empty() {
                tests.insert(range_of(node), found);
            }
        }
        let all = tests.values().flatten();
        Self {
            tested: all.map(|test| test.path.clone()).collect(),
            tests,
            cases,
            assigned: reader.assigned,
            shared,
            flows: BTreeMap::new(),
            states: BTreeMap::new(),
        }
    }
}

/// The primitive `typeof` gives a name for, when it gives it for that alone.
fn primitive_named(kind: &str) -> Option<TypeId> {
    match kind {
        "number" => Some(TypeId::NUMBER),
        "bigint" => Some(TypeId::BIGINT),
        "string" => Some(TypeId::STRING),
        "boolean" => Some(TypeId::BOOLEAN),
        "undefined" => Some(TypeId::UNDEFINED),
        _ => None,
    }
}

impl<'a> Typer<'a> {
    fn names(&self) -> Names<'_> {
        Names {
            source: self.source,
            bindings: self.bindings,
            references: &self.references,
        }
    }

    /// The type of the name or the member access `at`, which is a `ty` before the
    /// conditions on the way to it are counted: what is left of it there.
    pub(crate) fn narrowed(&mut self, at: &Element<'_>, ty: TypeId) -> TypeId {
        let path = self.names().path(at);
        let path = path.filter(|path| self.narrowing.tested.contains(path));
        let Some(path) = path else {
            return ty;
        };
        let (from, span) = match at {
            Element::Token(name) => (name.parent(), name.token().span),
            Element::Node(node) => (*node, node.span()),
        };
        self.flow_type(from, (span.byte_start, span.byte_end), &path, ty)
    }

    /// The type of `path` at the bytes `range`, read inside `from`.
    fn flow_type(&mut self, from: NodeRef<'_>, range: Range, path: &Path, top: TypeId) -> TypeId {
        // The function it is in. The value of a field runs when an instance is
        // made: nothing is told of then.
        let mut around = std::iter::successors(Some(from), |node| node.parent());
        let owner = around.find_map(|node| {
            let is_class = node.kind() == NodeKind::ClassBody;
            let body = body_of(node).filter(|_| !is_class);
            (is_class || body.is_some()).then_some(body.map(|body| (node, body)))
        });
        let Some((owner, body)) = owner.flatten() else {
            return top;
        };
        let flow = self.flow_of(owner, body);
        let key = (range_of(owner), path.clone(), top);
        let states = match self.narrowing.states.get(&key) {
            Some(Some(states)) => Rc::clone(states),
            // Being worked out: what is asked of it meanwhile is not narrowed.
            Some(None) => return top,
            None => {
                self.narrowing.states.insert(key.clone(), None);
                let entry = self.entry(owner, path, top);
                let mut states = vec![None; flow.cfg.nodes.len()];
                states[ENTRY] = Some(entry);
                let states = Rc::new(self.settled(&flow, path, top, states));
                self.narrowing.states.insert(key, Some(Rc::clone(&states)));
                states
            }
        };
        let held = flow.cfg.nodes_holding(range);
        let reaching: Vec<TypeId> = held.iter().filter_map(|node| states[*node]).collect();
        if reaching.is_empty() {
            top
        } else {
            self.types.union(reaching)
        }
    }

    /// The type of `path` where the function `owner` begins: for an arrow, what a
    /// name that is never assigned to was narrowed to where the arrow is written.
    fn entry(&mut self, owner: NodeRef<'_>, path: &Path, top: TypeId) -> TypeId {
        let (start, end) = range_of(owner);
        let declared = self.bindings.declarations[path.0].span.byte_start;
        let outside = !(start..end).contains(&declared);
        let settled = path.1.is_empty() && !self.narrowing.assigned.contains(&path.0);
        let is_arrow = owner.kind() == NodeKind::ArrowFunction;
        match owner.parent() {
            Some(around) if is_arrow && outside && settled => {
                self.flow_type(around, (start, end), path, top)
            }
            _ => top,
        }
    }

    /// The graph of a function, with what each node undoes.
    fn flow_of(&mut self, owner: NodeRef<'_>, body: Element<'_>) -> Rc<Flow> {
        let key = range_of(owner);
        if let Some(flow) = self.narrowing.flows.get(&key) {
            return Rc::clone(flow);
        }
        let cfg = Cfg::of_body(self.source, body);
        let count = cfg.nodes.len();
        let mut leaving = vec![Vec::new(); count];
        for edge in &cfg.edges {
            leaving[edge.from].push((edge.to, edge.label));
        }
        let mut calls = vec![false; count];
        let mut kills = vec![BTreeSet::new(); count];
        let inside = match body {
            Element::Node(body) => running(body),
            Element::Token(_) => Vec::new(),
        };
        for node in inside {
            let range = range_of(node);
            // Other code runs at a call, at an `await` and at a `yield`; a test
            // that is a call runs none.
            let runs = matches!(
                node.kind(),
                NodeKind::CallExpr | NodeKind::NewExpr | NodeKind::AwaitExpr | NodeKind::YieldExpr
            ) && !self.narrowing.tests.contains_key(&range);
            let assigns = matches!(node.kind(), NodeKind::AssignExpr | NodeKind::UpdateExpr);
            let target = self.names().root(node).filter(|_| assigns);
            if !runs && target.is_none() {
                continue;
            }
            for at in cfg.nodes_holding(range) {
                calls[at] = calls[at] || runs;
                kills[at].extend(target);
            }
        }
        // A declaration that runs again holds a new value.
        for (at, declared) in self.bindings.declarations.iter().enumerate() {
            let span = declared.span;
            if (key.0..key.1).contains(&span.byte_start) {
                for node in cfg.nodes_holding((span.byte_start, span.byte_end)) {
                    kills[node].insert(at);
                }
            }
        }
        let flow = Rc::new(Flow {
            cfg,
            leaving,
            calls,
            kills,
        });
        self.narrowing.flows.insert(key, Rc::clone(&flow));
        flow
    }

    /// What reaches each node once nothing more is added: a round adds what each
    /// edge brings to what its end has, and there is only so much to add.
    fn settled(&mut self, flow: &Flow, path: &Path, top: TypeId, states: States) -> States {
        let (root, keys) = path;
        // What a call may undo: a name a closure assigns, and what is read from an
        // object the program mutates.
        let mutated = self.facts.mutated.contains(root) && !keys.is_empty();
        let volatile = mutated || self.narrowing.shared.contains(root);
        let mut next = states.clone();
        for at in 0..next.len() {
            let Some(reaching) = next[at] else {
                continue;
            };
            let undone = flow.kills[at].contains(root) || (flow.calls[at] && volatile);
            let leaves = if undone { top } else { reaching };
            for &(to, label) in &flow.leaving[at] {
                let given = self.along(flow, at, label, path, leaves);
                let merged = next[to].map_or(given, |before| self.types.union([before, given]));
                next[to] = Some(merged);
            }
        }
        if next == states {
            states
        } else {
            self.settled(flow, path, top, next)
        }
    }

    /// What a `ty` at the node `at` is past the edge that leaves it by `label`.
    fn along(&mut self, flow: &Flow, at: usize, label: &str, path: &Path, ty: TypeId) -> TypeId {
        let Some(range) = flow.cfg.nodes[at].range else {
            return ty;
        };
        let holds = match label {
            "true" | "case" => true,
            "false" => false,
            // Past a `case` that did not match.
            "" if self.narrowing.cases.contains(&range) => false,
            _ => return ty,
        };
        let tests = self.narrowing.tests.get(&range);
        let tests = tests.into_iter().flatten();
        let tests: Vec<Test<'a>> = tests.filter(|test| test.path == *path).cloned().collect();
        let mut ty = ty;
        for test in tests {
            ty = self.apply(ty, &test.check, holds != test.negated);
        }
        ty
    }

    fn is_unit(&self, id: TypeId) -> bool {
        matches!(
            self.types.get(id),
            Type::Literal(_) | Type::Undefined | Type::Null
        )
    }

    fn is_open(&self, id: TypeId) -> bool {
        matches!(self.types.get(id), Type::Unknown | Type::Var(_))
    }

    /// Whether the class `class` is `base`, or extends it however far up.
    fn extends(&self, class: ClassId, base: ClassId) -> bool {
        let above = |class: &ClassId| self.facts.superclass.get(class).copied();
        let mut line = std::iter::successors(Some(class), above).take(256);
        line.any(|class| class == base)
    }

    /// The class a name names: one of the file, or of the ambient library.
    fn class_named_by(&self, name: &Element<'_>) -> Option<ClassId> {
        let Element::Token(name) = name else {
            return None;
        };
        let span = name.token().span;
        let declared = self.names().declared(span.byte_start);
        let own = declared.and_then(|declared| self.facts.class_of.get(&declared).copied());
        let text = self.source.slice(&span).unwrap_or_default();
        own.or(ambient_class(text).filter(|_| declared.is_none()))
    }

    /// What is left of a `ty` when a test of it holds, or when it does not.
    fn apply(&mut self, ty: TypeId, check: &Check<'_>, holds: bool) -> TypeId {
        let members = self.members_of(ty);
        let kept = match check {
            Check::Typeof(kind) => {
                let mut kept = Vec::new();
                for member in members {
                    let names = self.typeof_names(member);
                    // What may be anything stays, and is what the test says when a
                    // type says that.
                    let open = names.len() > 1;
                    let given = names.contains(&kind.as_str());
                    let exact =
                        primitive_named(kind).filter(|_| holds && member == TypeId::UNKNOWN);
                    let stays = if holds { given } else { open || !given };
                    kept.extend(stays.then_some(exact.unwrap_or(member)));
                }
                Some(kept)
            }
            Check::Equals(other, minus) => {
                let other = self.compared(*other, *minus);
                other.and_then(|other| self.equal(&members, other, holds))
            }
            Check::Discriminant(key, other, minus) => {
                let other = self.compared(*other, *minus);
                other.and_then(|other| self.discriminated(&members, key, other, holds))
            }
            Check::Instance(class) => self.class_named_by(class).map(|class| {
                let made = self.instance(class);
                let mut kept = Vec::new();
                for member in members {
                    let of = match self.types.get(member) {
                        Type::Class(of, _) => Some(*of),
                        _ => None,
                    };
                    let under = of.is_some_and(|of| self.extends(of, class));
                    let over = of.is_some_and(|of| self.extends(class, of));
                    let stays = if holds {
                        under || over || self.is_open(member)
                    } else {
                        !under
                    };
                    let given = if holds && !under { made } else { member };
                    kept.extend(stays.then_some(given));
                }
                kept
            }),
            Check::HasOwn(key) => {
                let mut kept = Vec::new();
                for member in members {
                    let shape = match self.types.get(member).clone() {
                        Type::Object(properties) => Some(properties),
                        Type::Unknown => Some(Vec::new()),
                        _ => None,
                    };
                    let has = shape
                        .as_ref()
                        .map(|shape| shape.iter().any(|property| property.key == *key));
                    match (has, shape) {
                        // What has the key is a shape with it, whatever else it has.
                        (Some(false), Some(mut shape)) if holds => {
                            shape.push(Property {
                                key: key.clone(),
                                ty: TypeId::UNKNOWN,
                                literal: false,
                            });
                            kept.push(self.types.intern(Type::Object(shape)));
                        }
                        // A shape has each of its keys.
                        (Some(true), _) if !holds => {}
                        _ => kept.push(member),
                    }
                }
                Some(kept)
            }
            Check::IsArray => {
                let mut kept = Vec::new();
                for member in members {
                    let list =
                        matches!(self.types.get(member), Type::Array { .. } | Type::Tuple(_));
                    let any = Type::Array {
                        element: TypeId::UNKNOWN,
                        mutated: false,
                    };
                    let made = (holds && member == TypeId::UNKNOWN).then(|| self.types.intern(any));
                    let stays = if holds { list || made.is_some() } else { !list };
                    kept.extend(stays.then_some(made.unwrap_or(member)));
                }
                Some(kept)
            }
            // `NaN` is a `number` that is no `int32` and no literal.
            Check::IsNan => Some(if holds {
                let numbers = [TypeId::NUMBER, TypeId::UNKNOWN];
                let is_number = members.iter().any(|member| numbers.contains(member));
                is_number.then_some(TypeId::NUMBER).into_iter().collect()
            } else {
                members
            }),
        };
        kept.map_or(ty, |kept| self.types.union(kept))
    }

    /// The type of what a path is compared with: of `-e` for a `case` with a sign.
    fn compared(&mut self, other: Element<'_>, minus: bool) -> Option<TypeId> {
        let ty = self.type_of(other)?;
        if minus {
            self.opposite(ty)
        } else {
            Some(ty)
        }
    }

    /// What is left of `members` when they are, or are not, what `other` is.
    fn equal(&mut self, members: &[TypeId], other: TypeId, holds: bool) -> Option<Vec<TypeId>> {
        let mut kept = Vec::new();
        if holds {
            // What both may be: of two that share a value, the one that says more.
            for &member in members {
                for theirs in self.members_of(other) {
                    let narrower =
                        self.is_open(member) || (self.is_unit(theirs) && !self.is_unit(member));
                    let shared = if narrower { theirs } else { member };
                    kept.extend(self.members_overlap(member, theirs).then_some(shared));
                }
            }
            return Some(kept);
        }
        // Only one value is ruled out by not being it.
        let flipped = match self.types.get(other) {
            Type::Literal(Literal::Boolean(value)) => Some(!*value),
            _ => None,
        };
        for &member in members {
            match flipped {
                Some(left) if member == TypeId::BOOLEAN => {
                    kept.push(self.types.literal(Literal::Boolean(left)));
                }
                _ => kept.extend((member != other).then_some(member)),
            }
        }
        self.is_unit(other).then_some(kept)
    }

    /// What is left of `members`, shapes that each have a literal at `key`, when
    /// that key is, or is not, what `other` is.
    fn discriminated(
        &mut self,
        members: &[TypeId],
        key: &str,
        other: TypeId,
        holds: bool,
    ) -> Option<Vec<TypeId>> {
        let other = Some(other).filter(|&other| self.is_unit(other))?;
        let mut kept = Vec::new();
        for &member in members {
            let tag = match self.types.get(member) {
                Type::Object(properties) => {
                    let tagged = properties.iter().find(|property| property.key == key);
                    tagged.map(|property| property.ty)
                }
                _ => None,
            };
            let tags = tag.map(|tag| self.members_of(tag));
            let tags = tags.filter(|tags| tags.iter().all(|&tag| self.is_unit(tag)))?;
            let stays = if holds {
                tags.contains(&other)
            } else {
                tags != [other]
            };
            kept.extend(stays.then_some(member));
        }
        Some(kept)
    }
}
