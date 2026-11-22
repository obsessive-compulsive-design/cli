//! Types across modules (SPEC.md §3.2): a module's imports have the types of what
//! they are bound to, so each module is inferred after the modules it imports.

use crate::bindings::infer_bindings;
use crate::expressions::Facts;
use crate::functions::Site;
use crate::mutation::{settled, uses_of};
use sjs_bind::{bind, Bindings, Bound, Content, DeclKind, Links, Module, ModuleGraph};
use sjs_diag::{Code, SourceFile};
use sjs_syntax::SyntaxTree;
use sjs_types::{ClassId, Property, Type, TypeId, Types};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What inference knows of one module.
#[derive(Debug, Clone)]
pub struct ModuleTypes {
    /// Its scopes, declarations and references.
    pub bindings: Bindings,
    /// The types of its declarations.
    pub facts: Facts,
}

/// How many times the modules are inferred from one another at most.
const ROUNDS: usize = 8;

struct Program<'a> {
    links: &'a Links,
    modules: BTreeMap<PathBuf, ModuleTypes>,
    /// The namespaces being given a shape: one that holds itself has none.
    shaping: BTreeSet<PathBuf>,
}

impl Program<'_> {
    /// The type of what an import or an export is bound to, when it is known.
    fn type_of(&mut self, bound: &Bound, types: &mut Types) -> Option<TypeId> {
        match bound {
            Bound::Json(_) => Some(TypeId::UNKNOWN),
            Bound::Declaration { module, span } => {
                let module = self.modules.get(module)?;
                let mut declarations = module.bindings.declarations.iter();
                let at = declarations.position(|declaration| declaration.span == *span);
                at.and_then(|at| module.facts.declarations.get(&at).copied())
            }
            Bound::Namespace(module) => self.namespace(module, types),
        }
    }

    /// The shape of a module's exports: a key for each, with the type of what it is
    /// bound to. `None` until every export has a type.
    fn namespace(&mut self, module: &Path, types: &mut Types) -> Option<TypeId> {
        if !self.modules.contains_key(module) || !self.shaping.insert(module.to_path_buf()) {
            return None;
        }
        let links = self.links;
        let mut properties = Some(Vec::new());
        for name in links.exports(module) {
            let bound = links.export(module, name);
            let ty = bound.and_then(|bound| self.type_of(&bound, types));
            let property = ty.map(|ty| Property {
                key: name.to_owned(),
                ty,
                literal: matches!(types.get(ty), Type::Literal(_)),
            });
            properties = properties.zip(property).map(|(mut properties, property)| {
                properties.push(property);
                properties
            });
        }
        self.shaping.remove(module);
        properties.map(|properties| types.intern(Type::Object(properties)))
    }
}

/// A module the grammar accepts, with its text, its tree and its names.
type Scanned<'a> = (&'a Module, &'a SourceFile, &'a SyntaxTree, Bindings);

/// The modules the grammar accepts, in the order they are inferred in.
fn accepted(graph: &ModuleGraph) -> Vec<Scanned<'_>> {
    let mut scanned = Vec::new();
    for module in graph.evaluation_order() {
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
        if !rejected {
            let (bindings, _) = bind(source, tree);
            scanned.push((module, source, tree, bindings));
        }
    }
    scanned
}

/// The call sites each module's parameters have in the other modules, as the round
/// before found them: for the module at each place of `paths`, by parameter.
fn sites_elsewhere(
    paths: &[&Path],
    previous: &BTreeMap<PathBuf, ModuleTypes>,
    canonical: &dyn Fn(usize, usize) -> (usize, usize),
) -> Vec<BTreeMap<usize, Vec<Site>>> {
    let mut external: Vec<BTreeMap<usize, Vec<Site>>> = vec![BTreeMap::new(); paths.len()];
    for (at, path) in paths.iter().enumerate() {
        let passed = previous.get(*path).map(|inferred| &inferred.facts.passed);
        for passed in passed.into_iter().flatten() {
            let (home, function) = canonical(at, passed.callee);
            let declared = previous.get(paths[home]);
            let parameters = declared.and_then(|inferred| inferred.facts.parameters.get(&function));
            let parameter =
                parameters.and_then(|list| list.get(passed.position).copied().flatten());
            if let Some(parameter) = parameter {
                external[home]
                    .entry(parameter)
                    .or_default()
                    .push(passed.site.clone());
            }
        }
    }
    external
}

/// What is mutated, program-wide: each as the module and declaration `canonical`
/// gives for it, so that a binding is one binding wherever it is imported.
fn mutated_anywhere(
    scanned: &[Scanned<'_>],
    canonical: &dyn Fn(usize, usize) -> (usize, usize),
) -> BTreeSet<(usize, usize)> {
    let uses: Vec<_> = scanned
        .iter()
        .map(|(_, source, tree, bindings)| uses_of(source, tree, bindings))
        .collect();
    let mut outright = BTreeSet::new();
    let mut calls = Vec::new();
    for (at, used) in uses.iter().enumerate() {
        outright.extend(
            used.direct
                .iter()
                .map(|&declaration| canonical(at, declaration)),
        );
        calls.extend(used.calls.iter().map(|&(argument, callee, place)| {
            (canonical(at, argument), canonical(at, callee), place)
        }));
    }
    let parameter = |(at, callee): (usize, usize), place: usize| {
        let parameters = uses[at].functions.get(&callee);
        let taken = parameters.and_then(|parameters| parameters.get(place).copied().flatten());
        taken.map(|taken| (at, taken))
    };
    settled(outright, &calls, &parameter)
}

/// Copies what is known of the classes from one module's facts into another's: a
/// class is one class wherever it is imported.
fn share_classes(from: &Facts, to: &mut Facts) {
    to.fields.clone_from(&from.fields);
    to.methods.clone_from(&from.methods);
    to.superclass.clone_from(&from.superclass);
    to.class_names.clone_from(&from.class_names);
    to.classes.clone_from(&from.classes);
}

/// Infers every module of the program, each after the modules it imports
/// ([`ModuleGraph::evaluation_order`]): an imported name has the type of the
/// declaration it is bound to, a namespace import the shape of its module's exports
/// (once each has a type), a JSON import `unknown`. A module the grammar rejects is
/// left out, and so what is imported from it has no type; nor has what modules that
/// import one another take from each other. What is mutated is settled program-wide
/// first: a binding mutated in any module, or passed in any module to a parameter its
/// function mutates, is mutated wherever it is imported. A function's parameters
/// are what every module passes for them: the modules are inferred in rounds, each
/// with the call sites the round before found in the others, until nothing changes.
/// A class is one class in every module: an import of it names it, and what is known
/// of the classes goes from each module to those inferred after it.
pub fn infer_program(
    graph: &ModuleGraph,
    links: &Links,
    types: &mut Types,
) -> BTreeMap<PathBuf, ModuleTypes> {
    let mut program = Program {
        links,
        modules: BTreeMap::new(),
        shaping: BTreeSet::new(),
    };
    let scanned = accepted(graph);
    // What is mutated, program-wide: a binding is one binding wherever it is imported.
    let place_of: BTreeMap<&Path, usize> = scanned
        .iter()
        .enumerate()
        .map(|(at, (module, ..))| (module.path.as_path(), at))
        .collect();
    let canonical = |at: usize, declaration: usize| -> (usize, usize) {
        let (module, _, _, bindings) = &scanned[at];
        let declared = &bindings.declarations[declaration];
        let bound = Some(declared)
            .filter(|declared| declared.kind == DeclKind::Import)
            .and_then(|declared| links.import(&module.path, &declared.name));
        let Some(Bound::Declaration { module, span }) = bound else {
            return (at, declaration);
        };
        let home = place_of.get(module.as_path()).copied();
        let found = home.and_then(|home| {
            let declarations = &scanned[home].3.declarations;
            let index = declarations.iter().position(|d| d.span == span);
            index.map(|index| (home, index))
        });
        found.unwrap_or((at, declaration))
    };
    let mutated = mutated_anywhere(&scanned, &canonical);
    // The classes of the program: each a class of its own, with its static side
    // after it.
    let mut free = 0;
    let mut class_of: Vec<BTreeMap<usize, ClassId>> = Vec::new();
    for (.., bindings) in &scanned {
        let mut classes = BTreeMap::new();
        for (index, declaration) in bindings.declarations.iter().enumerate() {
            if declaration.kind == DeclKind::Class {
                classes.insert(index, ClassId(free));
                free += 2;
            }
        }
        class_of.push(classes);
    }
    let mut shared = Facts::default();
    // A parameter is what every module passes for it, and what a module passes
    // depends on the others: the modules are inferred until nothing changes.
    let mut previous: BTreeMap<PathBuf, ModuleTypes> = BTreeMap::new();
    for _ in 0..ROUNDS {
        let paths: Vec<&Path> = scanned
            .iter()
            .map(|(module, ..)| module.path.as_path())
            .collect();
        let mut external = sites_elsewhere(&paths, &previous, &canonical);
        program.modules.clear();
        for (at, (module, source, tree, bindings)) in scanned.iter().enumerate() {
            let mut facts = Facts::default();
            let declarations = 0..bindings.declarations.len();
            facts.mutated = declarations
                .filter(|&declaration| mutated.contains(&canonical(at, declaration)))
                .collect();
            facts.external_sites = std::mem::take(&mut external[at]);
            facts.class_of.clone_from(&class_of[at]);
            share_classes(&shared, &mut facts);
            for (index, declaration) in bindings.declarations.iter().enumerate() {
                if declaration.kind != DeclKind::Import {
                    continue;
                }
                let bound = links.import(&module.path, &declaration.name);
                let ty = bound
                    .as_ref()
                    .and_then(|bound| program.type_of(bound, types));
                facts.declarations.extend(ty.map(|ty| (index, ty)));
                // A function imported is called as it is declared.
                let (home, function) = canonical(at, index);
                let class = class_of[home].get(&function);
                facts.class_of.extend(class.map(|&class| (index, class)));
                let declared = program.modules.get(&scanned[home].0.path);
                let returns = declared.and_then(|inferred| inferred.facts.returns.get(&function));
                let arity = declared.and_then(|inferred| inferred.facts.arity.get(&function));
                facts
                    .returns
                    .extend(returns.map(|&returns| (index, returns)));
                facts.arity.extend(arity.map(|&arity| (index, arity)));
            }
            let facts = infer_bindings(source, tree, bindings, &facts, types);
            share_classes(&facts, &mut shared);
            let inferred = ModuleTypes {
                bindings: bindings.clone(),
                facts,
            };
            program.modules.insert(module.path.clone(), inferred);
        }
        let snapshot = |modules: &BTreeMap<PathBuf, ModuleTypes>| -> Vec<_> {
            let facts = modules.values().map(|inferred| &inferred.facts);
            facts
                .map(|facts| (facts.declarations.clone(), facts.returns.clone()))
                .collect()
        };
        let settled = snapshot(&program.modules) == snapshot(&previous);
        previous.clone_from(&program.modules);
        if settled {
            break;
        }
    }
    program.modules
}
