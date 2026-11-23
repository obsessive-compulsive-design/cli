//! The portable platform globals (SPEC.md §6.2), typed from `ambient/platform/*.toml`,
//! where their Web IDL is transcribed by hand. An interface is a class of the ambient
//! library, a namespace (`console`, `crypto`) an object of functions, and a function
//! (`fetch`, `setTimeout`) a function; what is nullable in IDL is `| null` here, and
//! `any` is `unknown`. What a function of the platform is passed is to be what it
//! takes (E138), and a member an interface does not list is E133.

use crate::expressions::{element_span_of, ClassFacts, Facts, Others, Typer};
use crate::typed_arrays::ambient_class;
use crate::typed_arrays::is_ambient;
use sjs_diag::Code;
use sjs_syntax::{Element, NodeKind};
use sjs_types::{Function, Param, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// What a table of the data files is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An interface: a class of the ambient library.
    Interface,
    /// An object with members, which is not constructed.
    Namespace,
    /// A function.
    Function,
}

/// One member of an interface or a namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Its name.
    pub name: String,
    /// What it is, in the notation of the data files.
    pub what: String,
    /// Whether it is a member of the interface itself, not of its instances.
    pub is_static: bool,
    /// Whether its line says the IDL type is nullable.
    pub nullable: bool,
}

/// One table of the data files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    /// The name of the global, or of the interface.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// The standard and the IDL definition it transcribes.
    pub source: String,
    /// The interface it inherits from.
    pub extends: Option<String>,
    /// An expression that gives an instance.
    pub sample: Option<String>,
    /// The parameters of its constructor.
    pub constructor: Option<String>,
    /// The signature of a function.
    pub call: Option<String>,
    /// Its members.
    pub members: Vec<Member>,
}

/// A type, as the data files write one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    /// A primitive, or an interface by name.
    Name(String),
    /// `T[]`.
    Array(Box<Spec>),
    /// `[A, B]`.
    Tuple(Vec<Spec>),
    /// `A | B`.
    Union(Vec<Spec>),
    /// `Promise<T>` and the like.
    Generic(String, Box<Spec>),
    /// `(name: T, other?: U, ...rest: V) => R`: each parameter with whether it may
    /// be left out and whether it takes the rest, and what is given.
    Function(Vec<(bool, bool, Spec)>, Box<Spec>),
}

/// Reads the tables of a data file.
#[must_use]
pub fn interfaces(text: &str) -> Vec<Interface> {
    let mut tables: Vec<Interface> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|header| header.strip_suffix(']'))
        {
            let (kind, name) = match header.split_once(' ') {
                Some(("namespace", name)) => (Kind::Namespace, name),
                Some((_, name)) => (Kind::Function, name),
                None => (Kind::Interface, header),
            };
            tables.push(Interface {
                name: name.to_owned(),
                kind,
                source: String::new(),
                extends: None,
                sample: None,
                constructor: None,
                call: None,
                members: Vec::new(),
            });
        }
        let entry = line.split_once(" = ").filter(|_| !line.starts_with('#'));
        let Some(((key, rest), table)) = entry.zip(tables.last_mut()) else {
            continue;
        };
        // The value is what the first and the last quotation marks hold.
        let from = rest.find('"').map_or(0, |at| at + 1);
        let to = rest.rfind('"').unwrap_or(rest.len()).max(from);
        let value = rest[from..to].replace("\\\"", "\"");
        let key = key.trim_matches('"');
        match key {
            "idl" => table.source = value,
            "extends" => table.extends = Some(value),
            "sample" => table.sample = Some(value),
            "new" => table.constructor = Some(value),
            "call" => table.call = Some(value),
            _ => table.members.push(Member {
                name: key.trim_start_matches("static ").to_owned(),
                what: value,
                is_static: key.starts_with("static "),
                nullable: rest[to..].contains("# nullable"),
            }),
        }
    }
    tables
}

/// The tables the compiler carries: every file of `ambient/platform`.
pub fn platform() -> &'static [Interface] {
    static TABLES: OnceLock<Vec<Interface>> = OnceLock::new();
    TABLES.get_or_init(|| {
        let files = [
            include_str!("../../../ambient/platform/fetch.toml"),
            include_str!("../../../ambient/platform/xhr.toml"),
            include_str!("../../../ambient/platform/url.toml"),
            include_str!("../../../ambient/platform/dom.toml"),
            include_str!("../../../ambient/platform/streams.toml"),
            include_str!("../../../ambient/platform/file.toml"),
            include_str!("../../../ambient/platform/messaging.toml"),
            include_str!("../../../ambient/platform/crypto.toml"),
            include_str!("../../../ambient/platform/globals.toml"),
        ];
        files.into_iter().flat_map(interfaces).collect()
    })
}

/// Reads types written in the notation of the data files.
struct Reader<'a> {
    rest: &'a str,
}

impl Reader<'_> {
    /// Takes `word` off the front, if it is there.
    fn eat(&mut self, word: &str) -> bool {
        let rest = self.rest.trim_start();
        let found = rest.strip_prefix(word);
        self.rest = found.unwrap_or(rest);
        found.is_some()
    }

    /// Takes a name off the front: letters, digits and `_`.
    fn name(&mut self) -> Option<String> {
        let rest = self.rest.trim_start();
        let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_');
        let (name, rest) = rest.split_at(end.unwrap_or(rest.len()));
        self.rest = rest;
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// `A | B`.
    fn union(&mut self) -> Option<Spec> {
        let first = self.postfix()?;
        if !self.eat("|") {
            return Some(first);
        }
        // The members after it, a union of them taken apart.
        let mut members = vec![first];
        match self.union()? {
            Spec::Union(more) => members.extend(more),
            other => members.push(other),
        }
        Some(Spec::Union(members))
    }

    /// `T[]`.
    fn postfix(&mut self) -> Option<Spec> {
        let spec = self.primary()?;
        Some(self.arrays(spec))
    }

    /// `spec` with each `[]` that follows it.
    fn arrays(&mut self, spec: Spec) -> Spec {
        if self.eat("[]") {
            self.arrays(Spec::Array(Box::new(spec)))
        } else {
            spec
        }
    }

    /// Types between commas, up to `close`.
    fn list(&mut self, close: &str) -> Option<Vec<Spec>> {
        if self.eat(close) {
            return Some(Vec::new());
        }
        let mut specs = vec![self.union()?];
        self.eat(",");
        specs.extend(self.list(close)?);
        Some(specs)
    }

    /// Parameters between commas, up to the `)`: each with whether it may be left
    /// out and whether it takes the rest.
    fn parameters(&mut self) -> Option<Vec<(bool, bool, Spec)>> {
        if self.eat(")") {
            return Some(Vec::new());
        }
        let rest = self.eat("...");
        self.name()?;
        let optional = self.eat("?");
        let typed = self.eat(":");
        let mut parameters = vec![(optional, rest, self.union().filter(|_| typed)?)];
        self.eat(",");
        parameters.extend(self.parameters()?);
        Some(parameters)
    }

    fn primary(&mut self) -> Option<Spec> {
        if self.eat("[") {
            return self.list("]").map(Spec::Tuple);
        }
        // A type in parentheses, when what follows is no parameter.
        let mut ahead = Reader { rest: self.rest };
        let opens = ahead.eat("(");
        let named = ahead.name().is_some() && (ahead.eat("?") || ahead.eat(":"));
        let listed = named || ahead.rest.trim_start().starts_with([')', '.']);
        if opens && !listed {
            self.eat("(");
            let inner = self.union()?;
            return self.eat(")").then_some(inner);
        }
        if self.eat("(") {
            let parameters = self.parameters()?;
            let arrow = self.eat("=>");
            return Some(Spec::Function(
                parameters,
                Box::new(self.union().filter(|_| arrow)?),
            ));
        }
        let name = self.name()?;
        if self.eat("<") {
            let inner = self.union()?;
            return self.eat(">").then(|| Spec::Generic(name, Box::new(inner)));
        }
        Some(Spec::Name(name))
    }
}

/// The type a text of the data files writes; `None` when it is not written right.
#[must_use]
pub fn spec(text: &str) -> Option<Spec> {
    let mut reader = Reader { rest: text };
    let spec = reader.union()?;
    reader.rest.trim().is_empty().then_some(spec)
}

/// The names of the interfaces and namespaces, in the order of their class ids.
pub(crate) fn platform_classes() -> impl Iterator<Item = &'static str> {
    let tables = platform().iter();
    let classes = tables.filter(|table| table.kind != Kind::Function);
    classes.map(|table| table.name.as_str())
}

/// The type a [`Spec`] writes; `unknown` for a name that is none.
#[must_use]
pub fn typed(types: &mut Types, spec: &Spec) -> TypeId {
    match spec {
        Spec::Name(name) => match name.as_str() {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "int32" => TypeId::INT32,
            "bigint" => TypeId::BIGINT,
            "boolean" => TypeId::BOOLEAN,
            "undefined" => TypeId::UNDEFINED,
            "null" => TypeId::NULL,
            name => {
                let class = ambient_class(name).filter(|_| name != "unknown");
                class.map_or(TypeId::UNKNOWN, |class| {
                    types.intern(Type::Class(class, Vec::new()))
                })
            }
        },
        Spec::Array(element) => {
            let element = typed(types, element);
            types.intern(Type::Array {
                element,
                mutated: false,
            })
        }
        Spec::Tuple(elements) => {
            let elements = elements.iter().map(|element| typed(types, element));
            let elements = elements.collect();
            types.intern(Type::Tuple(elements))
        }
        Spec::Union(members) => {
            let members: Vec<TypeId> = members.iter().map(|member| typed(types, member)).collect();
            types.union(members)
        }
        Spec::Generic(name, inner) => {
            let inner = typed(types, inner);
            let asynchronous = name == "AsyncIterator";
            types.intern(match name.as_str() {
                "Promise" => Type::Promise(inner),
                _ => Type::Iterator {
                    item: inner,
                    asynchronous,
                },
            })
        }
        Spec::Function(parameters, returns) => {
            let mut params = Vec::new();
            let mut rest = None;
            for (optional, takes_rest, ty) in parameters {
                let ty = typed(types, ty);
                if *takes_rest {
                    rest = Some(ty);
                } else {
                    params.push(Param {
                        ty,
                        optional: *optional,
                    });
                }
            }
            let returns = typed(types, returns);
            types.intern(Type::Function(Function {
                type_params: Vec::new(),
                params,
                rest,
                returns,
            }))
        }
    }
}

/// The type a text of the data files writes; `unknown` when it is not written right.
fn read(types: &mut Types, text: &str) -> TypeId {
    let spec = spec(text).unwrap_or(Spec::Name("unknown".to_owned()));
    typed(types, &spec)
}

/// Tells `facts` the globals of the platform: the interfaces and namespaces as
/// classes of the ambient library, and what each global name is.
pub(crate) fn add_platform(facts: &mut Facts, types: &mut Types) {
    for table in platform() {
        if table.kind == Kind::Function {
            let call = table.call.as_deref().unwrap_or_default();
            facts.globals.insert(table.name.clone(), read(types, call));
            continue;
        }
        let class = ambient_class(&table.name).unwrap_or(crate::classes::UNKNOWN_BASE);
        let statics = crate::classes::statics_of(class);
        let instance = types.intern(Type::Class(class, Vec::new()));
        // A namespace is an object of functions, which are passed on as values.
        let namespace = table.kind == Kind::Namespace;
        let mut fields = [BTreeMap::new(), BTreeMap::new()];
        let mut methods = [BTreeSet::new(), BTreeSet::new()];
        let mut known = [ClassFacts::default(), ClassFacts::default()];
        for member in &table.members {
            let side = usize::from(member.is_static);
            let ty = read(types, &member.what);
            let is_method = types.function(ty).is_some() && !namespace;
            known[side].declared.insert(member.name.clone());
            if is_method {
                known[side].signatures.insert(member.name.clone(), ty);
                methods[side].insert(member.name.clone());
            } else {
                fields[side].insert(member.name.clone(), ty);
            }
        }
        let [mut own, mut of_class] = known;
        let [fields, static_fields] = fields;
        let [methods, static_methods] = methods;
        // What an interface does not list is not kept; what a namespace does not
        // is found by the ambient table already.
        own.others = if namespace {
            Others::Unseen
        } else {
            Others::Removed
        };
        own.base_arguments = Some(Vec::new());
        own.constructor = table.constructor.as_deref().map(|parameters| {
            let made = read(types, &format!("{parameters} => unknown"));
            let function = types.function(made).cloned();
            let function = function.map(|function| Function {
                returns: instance,
                ..function
            });
            function.map_or(made, |function| types.intern(Type::Function(function)))
        });
        let base = table.extends.as_deref().and_then(ambient_class);
        facts.superclass.extend(base.map(|base| (class, base)));
        if namespace {
            facts.globals.insert(table.name.clone(), instance);
        }
        facts.class_names.insert(class, table.name.clone());
        facts.fields.insert(class, fields);
        facts.methods.insert(class, methods);
        facts.classes.insert(class, own);
        if !namespace {
            // Its static members are those listed, like those of its instances.
            of_class.others = Others::Removed;
            of_class.base_arguments = Some(Vec::new());
            let name = format!("typeof {}", table.name);
            facts.class_names.insert(statics, name);
            facts.fields.insert(statics, static_fields);
            facts.methods.insert(statics, static_methods);
            facts.classes.insert(statics, of_class);
        }
    }
}

impl Typer<'_> {
    /// Whether what a call calls is of the ambient library: a global, or a member
    /// of an instance of one of its classes.
    fn calls_ambient(&mut self, callee: &Element<'_>) -> bool {
        match callee {
            Element::Token(_) => self.is_global(callee),
            Element::Node(target) => {
                let member = Some(*target).filter(|target| target.kind() == NodeKind::MemberExpr);
                let receiver = member.and_then(|target| self.receiver(target, false));
                receiver.is_some_and(|(receiver, _)| {
                    let members = self.members_of(receiver);
                    members.iter().all(|member| {
                        matches!(self.types().get(*member), Type::Class(class, _) if is_ambient(*class))
                    })
                })
            }
        }
    }

    /// E138 for an argument that is not what a function of the ambient library
    /// takes at its place. A function passed is typed by what it is expected to
    /// be, and is not judged here; an array is taken whether or not it is mutated.
    pub(crate) fn taken_rule(
        &mut self,
        first: bool,
        callee: &Element<'_>,
        function: &Function,
        passed: &[Element<'_>],
    ) {
        if !self.calls_ambient(callee) {
            return;
        }
        let name = self
            .source
            .slice(&element_span_of(callee))
            .unwrap_or_default();
        for (param, argument) in function.params.iter().zip(passed) {
            let Some(given) = self.type_of(*argument) else {
                continue;
            };
            let given = self.widened(given);
            let mut fit = Vec::new();
            let mut judged = true;
            for member in self.members_of(given) {
                fit.push(match self.types().get(member).clone() {
                    Type::Function(_) => {
                        judged = false;
                        member
                    }
                    Type::Array { element, .. } => self.types_mut().intern(Type::Array {
                        element,
                        mutated: false,
                    }),
                    _ => member,
                });
            }
            let fit = self.types_mut().union(fit);
            let taken = self.any_number(param.ty);
            if judged && !self.assignable(fit, taken) {
                let message = format!(
                    "a `{}` is passed where `{name}` takes a `{}` (SPEC.md §6.2)",
                    self.print_type(given),
                    self.print_type(param.ty)
                );
                self.report(first, Code::E138, element_span_of(argument), message);
            }
        }
    }
}
