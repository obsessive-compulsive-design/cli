//! Export and import resolution (SPEC.md §5.1): what every imported name is bound to,
//! through `export { … } from` chains, with E206 for a name the module asked does not
//! export and E137 for a namespace read by a key that is no string literal.

use crate::graph::{Content, Import, Module, ModuleGraph};
use crate::scopes::{bind, DeclKind};
use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};
use sjs_syntax::{Element, NodeKind, NodeRef, SyntaxTree, TokenKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What a name is bound to, once followed to where it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bound {
    /// A declaration of a module.
    Declaration {
        /// The canonical path of the module.
        module: PathBuf,
        /// The declared name, where it is written.
        span: Span,
    },
    /// The namespace of the module at this path: `import * as ns`, `import(…)`.
    Namespace(PathBuf),
    /// The value of the JSON module at this path, of type `unknown`.
    Json(PathBuf),
}

/// Where a name of a module comes from, one step back.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// A declaration of the module at this path.
    Local(PathBuf, Span),
    /// The export of this name by the module at this path.
    Named(PathBuf, String),
    Namespace(PathBuf),
    Json(PathBuf),
    /// Nothing that can be told: a path that names no module, a name not declared.
    Unknown,
}

/// The names of one module the grammar accepts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Linked {
    /// What each name an `import` binds comes from.
    imports: BTreeMap<String, Source>,
    /// What each exported name comes from.
    exports: BTreeMap<String, Source>,
}

/// What the imports and exports of a program are bound to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Links {
    modules: BTreeMap<PathBuf, Linked>,
}

impl Links {
    /// What the module at `module` exports as `name`. `None` when it exports no such
    /// name, or when what it exports cannot be told (a file the grammar rejects, a
    /// path that names no module, re-exports that go round).
    #[must_use]
    pub fn export(&self, module: &Path, name: &str) -> Option<Bound> {
        self.follow(&Source::Named(module.to_path_buf(), name.to_owned()))
    }

    /// What the name `local`, bound by an `import` of the module at `module`, is bound
    /// to.
    #[must_use]
    pub fn import(&self, module: &Path, local: &str) -> Option<Bound> {
        let source = self.modules.get(module)?.imports.get(local)?;
        self.follow(source)
    }

    fn follow<'a>(&'a self, mut source: &'a Source) -> Option<Bound> {
        let mut seen = BTreeSet::new();
        loop {
            match source {
                Source::Unknown => return None,
                Source::Local(module, span) => {
                    return Some(Bound::Declaration {
                        module: module.clone(),
                        span: *span,
                    })
                }
                Source::Namespace(path) => return Some(Bound::Namespace(path.clone())),
                Source::Json(path) => return Some(Bound::Json(path.clone())),
                Source::Named(path, name) => {
                    // Re-exports that go round export nothing.
                    if !seen.insert((path, name)) {
                        return None;
                    }
                    source = self.modules.get(path)?.exports.get(name)?;
                }
            }
        }
    }
}

impl Import {
    /// What `import(…)` of this path resolves to: the namespace of the module named.
    #[must_use]
    pub fn namespace(&self) -> Option<Bound> {
        let target = self.target.clone().filter(|_| self.dynamic);
        target.map(Bound::Namespace)
    }
}

impl ModuleGraph {
    /// Binds every import and export of the program, with E206 for a named import, a
    /// re-export or a key of a namespace (`ns.b`, `ns["b"]`) the module named does not
    /// export, each where the name is written, and E137 for a namespace read by a key
    /// that is no string literal. A module the grammar rejects (E000, E001, E002) is
    /// left out: its names are a guess, and nothing is said of what it exports.
    #[must_use]
    pub fn link(&self) -> (Links, Diagnostics) {
        let mut links = Links::default();
        let mut uses = Vec::new();
        for module in self.modules() {
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
                let mut reader = Reader {
                    graph: self,
                    module,
                    source,
                    linked: Linked::default(),
                    uses: Vec::new(),
                    keys: BTreeMap::new(),
                };
                reader.read(tree);
                links.modules.insert(module.path.clone(), reader.linked);
                uses.push((module, reader.uses));
            }
        }
        let mut diagnostics = Diagnostics::new();
        for (module, uses) in uses {
            for used in uses {
                let (target, name, span) = match used {
                    Use::Export(target, name, span) => (target, name, span),
                    Use::Key(local, key, span) => {
                        let Some(Bound::Namespace(target)) = links.import(&module.path, &local)
                        else {
                            continue;
                        };
                        let Some(name) = key else {
                            let message = format!(
                                "`{local}` is a namespace, read by the names of its exports only: `{local}.name` or `{local}[\"name\"]` (SPEC.md §4.6)"
                            );
                            let found =
                                Diagnostic::new(Code::E137, module.name.clone(), span, message);
                            diagnostics.push(found);
                            continue;
                        };
                        (target, name, span)
                    }
                };
                let exports = links.modules.get(&target).map(|linked| &linked.exports);
                if exports.is_some_and(|exports| !exports.contains_key(&name)) {
                    let from = self.get(&target).map_or("", |target| target.name.as_str());
                    let message = format!("`{from}` does not export `{name}` (SPEC.md §5.1)");
                    diagnostics.push(Diagnostic::new(
                        Code::E206,
                        module.name.clone(),
                        span,
                        message,
                    ));
                }
            }
        }
        (links, diagnostics)
    }
}

/// A name a module asks another for.
enum Use {
    /// The export of this name by the module at this path: a named import, a re-export.
    Export(PathBuf, String, Span),
    /// A key of what this imported name is bound to: its name (`ns.b`, `ns["b"]`), or
    /// none for a key that is no string literal.
    Key(String, Option<String>, Span),
}

fn node_of(element: Element<'_>) -> Option<NodeRef<'_>> {
    match element {
        Element::Node(node) => Some(node),
        Element::Token(_) => None,
    }
}

/// Where the first and the last token of `node` are.
fn ends(node: NodeRef<'_>) -> Option<(Span, Span)> {
    let mut tokens = node.tokens().iter().map(|token| token.span);
    let first = tokens.next();
    first.map(|first| (first, tokens.next_back().unwrap_or(first)))
}

struct Reader<'a> {
    graph: &'a ModuleGraph,
    module: &'a Module,
    source: &'a SourceFile,
    linked: Linked,
    uses: Vec<Use>,
    /// The key each name is read by, by where the name begins: `ns.b`, `ns[…]`.
    keys: BTreeMap<usize, (Option<String>, Span)>,
}

impl<'a> Reader<'a> {
    /// The first and the last word directly inside each specifier of `list`, which are
    /// one word for `a` and two for `a as b`: (text, where the first is written, text).
    fn specifiers(&self, list: NodeRef<'_>) -> Vec<(String, Span, String)> {
        let specifiers = list.children().filter_map(node_of);
        specifiers
            .filter_map(|specifier| {
                ends(specifier).map(|(name, bound)| (self.text(name), name, self.text(bound)))
            })
            .collect()
    }

    fn text(&self, span: Span) -> String {
        self.source.slice(&span).unwrap_or_default().to_owned()
    }

    /// The module the path of the declaration `node` names, if it names one.
    fn target(&self, node: NodeRef<'_>) -> Option<&'a Module> {
        let path = node.children().find_map(|child| match child {
            Element::Token(token) if matches!(token.token().kind, TokenKind::String(_)) => {
                Some(token.token().span)
            }
            _ => None,
        })?;
        let (graph, module) = (self.graph, self.module);
        let import = module.imports.iter().find(|import| import.span == path)?;
        graph.get(import.target.as_deref()?)
    }

    fn read(&mut self, tree: &SyntaxTree) {
        let root = tree.root();
        let (bindings, _) = bind(self.source, tree);
        let declarations: Vec<NodeRef<'_>> = root.children().filter_map(node_of).collect();
        for node in declarations
            .iter()
            .filter(|node| node.kind() == NodeKind::ImportDecl)
        {
            self.import(*node);
        }
        for declaration in bindings.declarations.iter().filter(|d| d.exported) {
            let local = Source::Local(self.module.path.clone(), declaration.span);
            self.linked.exports.insert(declaration.name.clone(), local);
        }
        for node in declarations
            .iter()
            .filter(|node| node.kind() == NodeKind::ExportDecl)
        {
            let target = self.target(*node).map(|target| target.path.clone());
            let from = node
                .children()
                .any(|child| matches!(child, Element::Token(token) if matches!(token.token().kind, TokenKind::String(_))));
            let lists = node.children().filter_map(node_of);
            let lists = lists.filter(|list| list.kind() == NodeKind::NamedExports);
            let specifiers: Vec<_> = lists.flat_map(|list| self.specifiers(list)).collect();
            for (name, span, exported) in specifiers {
                let source = match &target {
                    Some(target) => {
                        let asked = Use::Export(target.clone(), name.clone(), span);
                        self.uses.push(asked);
                        Source::Named(target.clone(), name.clone())
                    }
                    None if from => Source::Unknown,
                    None => {
                        let declared = bindings
                            .lookup(0, &name)
                            .map(|at| &bindings.declarations[at]);
                        declared.map_or(Source::Unknown, |declared| {
                            let imported = self.linked.imports.get(&name);
                            let imported = imported.filter(|_| declared.kind == DeclKind::Import);
                            imported.map_or(
                                Source::Local(self.module.path.clone(), declared.span),
                                Clone::clone,
                            )
                        })
                    }
                };
                self.linked.exports.insert(exported, source);
            }
        }
        self.keys(root);
        for reference in &bindings.references {
            let declared = reference.declaration.map(|at| &bindings.declarations[at]);
            let key = self.keys.get(&reference.span.byte_start);
            if let (Some(declared), Some((key, span))) = (declared, key) {
                if declared.kind == DeclKind::Import {
                    let read = Use::Key(reference.name.clone(), key.clone(), *span);
                    self.uses.push(read);
                }
            }
        }
    }

    /// What the names of an `import` declaration come from.
    fn import(&mut self, node: NodeRef<'_>) {
        let target = self.target(node);
        for clause in node.children().filter_map(node_of) {
            // The one default import the grammar accepts is of a JSON module.
            let whole = match clause.kind() {
                NodeKind::NamedImports => {
                    for (name, span, local) in self.specifiers(clause) {
                        let source = target.map_or(Source::Unknown, |target| {
                            let asked = Use::Export(target.path.clone(), name.clone(), span);
                            self.uses.push(asked);
                            Source::Named(target.path.clone(), name)
                        });
                        self.linked.imports.insert(local, source);
                    }
                    continue;
                }
                NodeKind::NamespaceImport => Source::Namespace,
                NodeKind::DefaultImport => Source::Json,
                _ => continue,
            };
            let source = target.map_or(Source::Unknown, |target| whole(target.path.clone()));
            let local = ends(clause).map(|(_, last)| self.text(last));
            self.linked
                .imports
                .extend(local.map(|local| (local, source)));
        }
    }

    /// Records the key every name is read by, throughout `node`.
    fn keys(&mut self, node: NodeRef<'_>) {
        let mut pending = vec![node];
        while let Some(node) = pending.pop() {
            let children: Vec<Element<'_>> = node.children().collect();
            pending.extend(children.iter().copied().filter_map(node_of));
            let object = match children.first() {
                Some(Element::Token(token)) => token.token().span.byte_start,
                _ => continue,
            };
            let key = match (node.kind(), children.last(), children.iter().rev().nth(1)) {
                (NodeKind::MemberExpr, Some(Element::Token(name)), _) => {
                    let span = name.token().span;
                    let text = self.source.slice(&span).unwrap_or_default();
                    (Some(text.to_owned()), span)
                }
                (NodeKind::IndexExpr, _, Some(Element::Token(key))) => {
                    let name = match &key.token().kind {
                        TokenKind::String(value) => Some(value.to_string()),
                        _ => None,
                    };
                    (name, key.token().span)
                }
                (NodeKind::IndexExpr, _, Some(Element::Node(key))) => (None, key.span()),
                _ => continue,
            };
            self.keys.insert(object, key);
        }
    }
}
