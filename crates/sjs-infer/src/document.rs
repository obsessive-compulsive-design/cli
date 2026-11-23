//! The `types.json` of a program (SPEC.md §7, §11; STORIES.md US-100): the inferred
//! type of every exported binding, in the notation of §13.7 and as a tree. The
//! document depends on the program alone: its modules are in the order of their
//! paths and their exports in the order of their names, whatever order the modules
//! were found in. It is made from inference alone, before anything is checked, and a
//! module the grammar rejects is in it too, its exports `unknown` and marked.

use crate::program::{infer_program, ModuleTypes};
use sjs_bind::{bind, Bound, Content, Links, ModuleGraph};
use sjs_diag::{Code, ExportedType, Json, ModuleExports, TypesDocument};
use sjs_types::{Builtin, ClassId, Function, Literal, Printer, Type, TypeId, Types};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The kinds a type has in the tree of `types.json`, which
/// `schema/types.schema.json` lists too.
pub const KINDS: [&str; 22] = [
    "number",
    "int32",
    "bigint",
    "string",
    "boolean",
    "undefined",
    "null",
    "unknown",
    "never",
    "literal",
    "object",
    "dictionary",
    "array",
    "tuple",
    "function",
    "class",
    "variable",
    "promise",
    "iterator",
    "generator",
    "collection",
    "union",
];

/// Writes types as trees.
struct Tree<'a> {
    types: &'a mut Types,
    names: &'a BTreeMap<ClassId, String>,
}

impl Tree<'_> {
    fn kind(kind: &str, members: Vec<(&str, Json)>) -> Json {
        let mut all = vec![("kind", Json::text(kind))];
        all.extend(members);
        Json::map(all)
    }

    fn list(&mut self, ids: &[TypeId]) -> Json {
        Json::List(ids.iter().map(|&id| self.tree(id)).collect())
    }

    /// The name of a class, or of a type variable.
    fn printed(&self, id: TypeId) -> String {
        let name = |class: ClassId| {
            let name = self.names.get(&class);
            name.map_or(format!("class #{}", class.0), Clone::clone)
        };
        Printer::new(self.types, &name).print(id)
    }

    /// The tree of a function type.
    fn function(&mut self, function: &Function) -> Json {
        let mut variables = Vec::new();
        for variable in &function.type_params {
            let variable = self.types.intern(Type::Var(*variable));
            variables.push(Json::Text(self.printed(variable)));
        }
        let mut parameters = Vec::new();
        for param in &function.params {
            parameters.push(Json::map(vec![
                ("optional", Json::Bool(param.optional)),
                ("type", self.tree(param.ty)),
            ]));
        }
        let rest = function.rest.map_or(Json::Null, |rest| self.tree(rest));
        Self::kind(
            "function",
            vec![
                ("typeParameters", Json::List(variables)),
                ("parameters", Json::List(parameters)),
                ("rest", rest),
                ("returns", self.tree(function.returns)),
            ],
        )
    }

    /// The tree of a type.
    fn tree(&mut self, id: TypeId) -> Json {
        match self.types.get(id).clone() {
            Type::Number => Self::kind("number", Vec::new()),
            Type::Int32 => Self::kind("int32", Vec::new()),
            Type::BigInt => Self::kind("bigint", Vec::new()),
            Type::String => Self::kind("string", Vec::new()),
            Type::Boolean => Self::kind("boolean", Vec::new()),
            Type::Undefined => Self::kind("undefined", Vec::new()),
            Type::Null => Self::kind("null", Vec::new()),
            Type::Unknown => Self::kind("unknown", Vec::new()),
            Type::Never => Self::kind("never", Vec::new()),
            Type::Literal(literal) => {
                let (of, value) = match &literal {
                    Literal::String(value) => ("string", value.clone()),
                    Literal::Number(value) => ("number", value.text()),
                    Literal::BigInt(value) => ("bigint", value.decimal()),
                    Literal::Boolean(value) => ("boolean", value.to_string()),
                };
                Self::kind(
                    "literal",
                    vec![("of", Json::text(of)), ("value", Json::Text(value))],
                )
            }
            Type::Object(properties) => {
                let mut listed = Vec::new();
                for property in properties {
                    listed.push(Json::map(vec![
                        ("key", Json::Text(property.key)),
                        ("literal", Json::Bool(property.literal)),
                        ("type", self.tree(property.ty)),
                    ]));
                }
                Self::kind("object", vec![("properties", Json::List(listed))])
            }
            Type::Dictionary(value) => Self::kind("dictionary", vec![("value", self.tree(value))]),
            Type::Array { element, mutated } => Self::kind(
                "array",
                vec![
                    ("readonly", Json::Bool(!mutated)),
                    ("element", self.tree(element)),
                ],
            ),
            Type::Tuple(elements) => Self::kind("tuple", vec![("elements", self.list(&elements))]),
            Type::Function(function) => self.function(&function),
            Type::Class(class, arguments) => {
                let bare = self.types.intern(Type::Class(class, Vec::new()));
                Self::kind(
                    "class",
                    vec![
                        ("name", Json::Text(self.printed(bare))),
                        ("arguments", self.list(&arguments)),
                    ],
                )
            }
            Type::Var(_) => Self::kind("variable", vec![("name", Json::Text(self.printed(id)))]),
            Type::Promise(value) => Self::kind("promise", vec![("value", self.tree(value))]),
            Type::Iterator { item, asynchronous } => Self::kind(
                "iterator",
                vec![
                    ("async", Json::Bool(asynchronous)),
                    ("item", self.tree(item)),
                ],
            ),
            Type::Generator {
                yields,
                returns,
                asynchronous,
            } => Self::kind(
                "generator",
                vec![
                    ("async", Json::Bool(asynchronous)),
                    ("yields", self.tree(yields)),
                    ("returns", self.tree(returns)),
                ],
            ),
            Type::Builtin(builtin, arguments) => {
                let name = match builtin {
                    Builtin::Map => "Map",
                    Builtin::Set => "Set",
                    Builtin::WeakMap => "WeakMap",
                };
                Self::kind(
                    "collection",
                    vec![
                        ("name", Json::text(name)),
                        ("arguments", self.list(&arguments)),
                    ],
                )
            }
            Type::Union(members) => Self::kind("union", vec![("members", self.list(&members))]),
        }
    }
}

/// The type of what a module's export is bound to, with the module that declares
/// it and the place of its declaration there.
fn exported<'a>(
    bound: &Bound,
    modules: &'a BTreeMap<PathBuf, ModuleTypes>,
) -> Option<(&'a ModuleTypes, usize, TypeId)> {
    let Bound::Declaration { module, span } = bound else {
        return None;
    };
    let home = modules.get(module);
    let found = home.and_then(|home| {
        let mut declarations = home.bindings.declarations.iter();
        let at = declarations.position(|declaration| declaration.span == *span);
        // A declaration is found where it is; one that is not has no type.
        let at = at.unwrap_or(usize::MAX);
        let ty = home.facts.declarations.get(&at).copied();
        ty.map(|ty| (home, at, ty))
    });
    found
}

/// The `types.json` of an inferred program: `modules` is what [`infer_program`]
/// found of `graph`, in `types`.
#[must_use]
pub fn types_document(
    graph: &ModuleGraph,
    links: &Links,
    modules: &BTreeMap<PathBuf, ModuleTypes>,
    types: &mut Types,
) -> TypesDocument {
    let mut listed = Vec::new();
    for module in graph.modules() {
        let Content::Script {
            source,
            tree,
            parsed,
        } = &module.content
        else {
            continue;
        };
        let rejected = parsed
            .iter()
            .any(|d| matches!(d.code, Code::E000 | Code::E001 | Code::E002));
        // What a module the grammar rejects exports is read off its declarations:
        // a guess, for a tree that was recovered, and enough to name them.
        let (bindings, _) = bind(source, tree);
        let declared = bindings
            .declarations
            .iter()
            .filter(|d| d.exported && rejected);
        let mut names: Vec<&str> = declared.map(|d| d.name.as_str()).collect();
        names.extend(links.exports(&module.path));
        names.sort_unstable();
        names.dedup();
        let mut exports = Vec::new();
        for name in names {
            let bound = links.export(&module.path, name);
            let found = bound.and_then(|bound| exported(&bound, modules));
            let inferred = found.filter(|_| !rejected);
            let (notation, structure) = match inferred {
                Some((home, at, ty)) => {
                    let mut tree = Tree {
                        types: &mut *types,
                        names: &home.facts.class_names,
                    };
                    let structure = tree.tree(ty);
                    // A function is written with the names of its parameters.
                    let parameters = home.facts.parameters.get(&at);
                    let declarations = &home.bindings.declarations;
                    let labels: Vec<&str> = parameters
                        .into_iter()
                        .flatten()
                        .map(|parameter| parameter.map_or("_", |at| declarations[at].name.as_str()))
                        .collect();
                    let class = |class: ClassId| {
                        let name = home.facts.class_names.get(&class);
                        name.map_or(format!("class #{}", class.0), Clone::clone)
                    };
                    let printer = Printer::new(types, &class);
                    (printer.signature(name, &labels, ty), structure)
                }
                None => (
                    format!("{name}: unknown"),
                    Tree::kind("unknown", Vec::new()),
                ),
            };
            exports.push(ExportedType {
                name: name.to_owned(),
                inferred: inferred.is_some(),
                notation,
                structure,
            });
        }
        listed.push(ModuleExports {
            path: module.name.clone(),
            syntax_error: rejected,
            exports,
        });
    }
    listed.sort_by(|a, b| a.path.cmp(&b.path));
    TypesDocument { modules: listed }
}

/// The `types.json` of the program `graph` holds: inferred, then written.
#[must_use]
pub fn types_json(graph: &ModuleGraph, links: &Links) -> String {
    let mut types = Types::new();
    let modules = infer_program(graph, links, &mut types);
    types_document(graph, links, &modules, &mut types).to_json()
}
