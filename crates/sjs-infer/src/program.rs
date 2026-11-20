//! Types across modules (SPEC.md §3.2): a module's imports have the types of what
//! they are bound to, so each module is inferred after the modules it imports.

use crate::bindings::infer_bindings;
use crate::expressions::Facts;
use crate::mutation::{settled, uses_of};
use sjs_bind::{bind, Bindings, Bound, Content, DeclKind, Links, ModuleGraph};
use sjs_diag::Code;
use sjs_types::{Property, Type, TypeId, Types};
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

/// Infers every module of the program, each after the modules it imports
/// ([`ModuleGraph::evaluation_order`]): an imported name has the type of the
/// declaration it is bound to, a namespace import the shape of its module's exports
/// (once each has a type), a JSON import `unknown`. A module the grammar rejects is
/// left out, and so what is imported from it has no type; nor has what modules that
/// import one another take from each other. What is mutated is settled program-wide
/// first: a binding mutated in any module, or passed in any module to a parameter its
/// function mutates, is mutated wherever it is imported.
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
    // The modules the grammar accepts, in the order they are inferred in.
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
    let mutated = settled(outright, &calls, &parameter);
    for (at, (module, source, tree, bindings)) in scanned.iter().enumerate() {
        let mut facts = Facts::default();
        let declarations = 0..bindings.declarations.len();
        facts.mutated = declarations
            .filter(|&declaration| mutated.contains(&canonical(at, declaration)))
            .collect();
        let imports = bindings.declarations.iter().enumerate();
        for (index, declaration) in imports.filter(|(_, d)| d.kind == DeclKind::Import) {
            let bound = links.import(&module.path, &declaration.name);
            let ty = bound.and_then(|bound| program.type_of(&bound, types));
            facts.declarations.extend(ty.map(|ty| (index, ty)));
        }
        let facts = infer_bindings(source, tree, bindings, &facts, types);
        let inferred = ModuleTypes {
            bindings: bindings.clone(),
            facts,
        };
        program.modules.insert(module.path.clone(), inferred);
    }
    program.modules
}
