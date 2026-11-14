//! The module graph (SPEC.md §5.1, §5.3): the transitive closure of the root's imports,
//! each file read and parsed once and keyed by its canonical path.

use crate::host::Host;
use crate::json::is_json;
use sjs_diag::{Code, Diagnostic, Diagnostics, SourceFile, Span};
use sjs_syntax::{is_project_path, parse, Element, NodeKind, NodeRef, SyntaxTree, TokenKind};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

/// A path a module names: in `import … from`, `export … from` or `import(…)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// The string as written, quotes included.
    pub span: Span,
    /// Its value.
    pub specifier: String,
    /// Whether it is the path of `import(…)`.
    pub dynamic: bool,
    /// The canonical path of the module it names; `None` when it names none (E205, or
    /// a finding of the parser's on the path).
    pub target: Option<PathBuf>,
}

/// What a module holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    /// A `.js` file, parsed.
    Script {
        /// The text.
        source: SourceFile,
        /// Its tree.
        tree: SyntaxTree,
        /// What the parser found.
        parsed: Diagnostics,
    },
    /// A `.js` file that is no text (E000: not UTF-8, or a byte order mark).
    Invalid(Diagnostic),
    /// A `.json` file that is JSON.
    Json,
}

/// One file of the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    /// Its canonical path: its identity.
    pub path: PathBuf,
    /// The path diagnostics print: relative to the base with `/`, or the whole path for
    /// a file outside the base.
    pub name: String,
    /// What it holds.
    pub content: Content,
    /// The paths it names, in source order.
    pub imports: Vec<Import>,
}

/// A root that cannot be loaded: the CLI's exit 70.
#[derive(Debug)]
pub struct LoadError {
    /// The root.
    pub path: PathBuf,
    /// Why.
    pub error: io::Error,
}

/// The program: every module the roots reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleGraph {
    /// The canonical paths of the roots, in the order given, each once.
    pub roots: Vec<PathBuf>,
    modules: BTreeMap<PathBuf, Module>,
}

impl ModuleGraph {
    /// Loads the roots and everything they import, and reports each path that names no
    /// module as E205. Diagnostics name files relative to `base`.
    ///
    /// # Errors
    ///
    /// A root that does not exist or cannot be read.
    pub fn load(
        host: &dyn Host,
        base: &Path,
        roots: &[PathBuf],
    ) -> Result<(Self, Diagnostics), LoadError> {
        let mut loader = Loader {
            host,
            base: host.canonical(base).unwrap_or(base.to_path_buf()),
            listings: BTreeMap::new(),
            fetched: BTreeMap::new(),
            pending: Vec::new(),
            modules: BTreeMap::new(),
            diagnostics: Diagnostics::new(),
        };
        let mut canonical_roots = Vec::new();
        for root in roots {
            let failed = |error| LoadError {
                path: root.clone(),
                error,
            };
            let path = host.canonical(root).map_err(failed)?;
            if !loader.fetched.contains_key(&path) {
                let bytes = host.read(&path).map_err(failed)?;
                loader.fetched.insert(path.clone(), Ok(()));
                loader.pending.push((path.clone(), bytes));
                canonical_roots.push(path);
            }
        }
        while let Some((path, bytes)) = loader.pending.pop() {
            loader.script(path, bytes);
        }
        let graph = Self {
            roots: canonical_roots,
            modules: loader.modules,
        };
        Ok((graph, loader.diagnostics))
    }

    /// The modules, in the order of their canonical paths.
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    /// The module at a canonical path.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<&Module> {
        self.modules.get(path)
    }

    /// Every `*.test.js` file in the directory of each module (§5.3: the tests of a
    /// program given by explicit entries), canonical and sorted.
    #[must_use]
    pub fn test_files(&self, host: &dyn Host) -> Vec<PathBuf> {
        let directories: BTreeSet<&Path> = self.modules.keys().filter_map(|p| p.parent()).collect();
        let mut found = BTreeSet::new();
        for directory in directories {
            for name in host.names(directory).unwrap_or_default() {
                let path = directory.join(&name);
                if name.to_string_lossy().ends_with(".test.js") && host.is_file(&path) {
                    found.insert(host.canonical(&path).unwrap_or(path));
                }
            }
        }
        found.into_iter().collect()
    }
}

struct Loader<'h> {
    host: &'h dyn Host,
    base: PathBuf,
    /// The entries of each directory a path has gone through.
    listings: BTreeMap<PathBuf, BTreeSet<OsString>>,
    /// Each file asked for, with why it is no module if it is none.
    fetched: BTreeMap<PathBuf, Result<(), String>>,
    /// The `.js` files read and not yet parsed.
    pending: Vec<(PathBuf, Vec<u8>)>,
    modules: BTreeMap<PathBuf, Module>,
    diagnostics: Diagnostics,
}

impl Loader<'_> {
    fn name(&self, path: &Path) -> String {
        path.strip_prefix(&self.base).map_or_else(
            |_| path.display().to_string(),
            |inside| {
                let parts: Vec<_> = inside
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy())
                    .collect();
                parts.join("/")
            },
        )
    }

    /// Parses a `.js` file and resolves the paths it names.
    fn script(&mut self, path: PathBuf, bytes: Vec<u8>) {
        let name = self.name(&path);
        let mut imports = Vec::new();
        let content = match SourceFile::new(name.clone(), bytes) {
            Err(diagnostic) => Content::Invalid(*diagnostic),
            Ok(source) => {
                let (tree, parsed) = parse(&source);
                named_paths(tree.root(), &mut imports);
                let directory = path.parent().unwrap_or(&path).to_path_buf();
                for import in &mut imports {
                    match self.resolve(&directory, &import.specifier) {
                        Ok(target) => import.target = Some(target),
                        Err(reason) => self.diagnostics.push(Diagnostic::new(
                            Code::E205,
                            name.clone(),
                            import.span,
                            format!("`{}` {reason} (SPEC.md §5.1)", import.specifier),
                        )),
                    }
                }
                Content::Script {
                    source,
                    tree,
                    parsed,
                }
            }
        };
        let module = Module {
            path: path.clone(),
            name,
            content,
            imports,
        };
        self.modules.insert(path, module);
    }

    /// The canonical path of the module `specifier` names from a file in `directory`,
    /// or why it names none.
    // Extensions are compared exactly: a path names a file as it is on disk.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    fn resolve(&mut self, directory: &Path, specifier: &str) -> Result<PathBuf, String> {
        let missing = || {
            "names no file: a path is spelled as its directory entries are, letter case included"
                .to_owned()
        };
        let mut segments: Vec<&str> = specifier.split('/').collect();
        let last = segments.pop().unwrap_or_default();
        let mut current = directory.to_path_buf();
        for segment in segments {
            current = self.step(&current, segment).ok_or_else(missing)?;
        }
        let directory = || "names a directory, not a file".to_owned();
        if last.is_empty() {
            return Err(directory());
        }
        let target = self.step(&current, last).ok_or_else(missing)?;
        if !self.host.is_file(&target) {
            return Err(directory());
        }
        if target.extension() != Path::new(last).extension() {
            return Err("is a link to a file of another kind".to_owned());
        }
        if let Some(known) = self.fetched.get(&target) {
            return known.clone().map(|()| target);
        }
        let fetched = self.fetch(&target, last.ends_with(".json"));
        self.fetched.insert(target.clone(), fetched.clone());
        fetched.map(|()| target)
    }

    /// One segment further from `current`: an entry spelled exactly `segment`, `.` or
    /// `..`. On a file system that ignores case, a path spelled in another case would
    /// open here and fail elsewhere, so the entry's own spelling decides.
    fn step(&mut self, current: &Path, segment: &str) -> Option<PathBuf> {
        if !matches!(segment, "." | "..") {
            let host = self.host;
            let names = self
                .listings
                .entry(current.to_path_buf())
                .or_insert_with(|| {
                    host.names(current)
                        .unwrap_or_default()
                        .into_iter()
                        .collect()
                });
            if !names.contains(OsStr::new(segment)) {
                return None;
            }
        }
        self.host.canonical(&current.join(segment)).ok()
    }

    /// Reads a file not asked for before: a `.json` file must be JSON and is then a
    /// module; a `.js` file waits to be parsed.
    fn fetch(&mut self, path: &Path, json: bool) -> Result<(), String> {
        let bytes = self
            .host
            .read(path)
            .map_err(|error| format!("cannot be read: {error}"))?;
        if !json {
            self.pending.push((path.to_path_buf(), bytes));
        } else if is_json(&bytes) {
            let module = Module {
                path: path.to_path_buf(),
                name: self.name(path),
                content: Content::Json,
                imports: Vec::new(),
            };
            self.modules.insert(path.to_path_buf(), module);
        } else {
            return Err("is not JSON (RFC 8259)".to_owned());
        }
        Ok(())
    }
}

/// The paths `node` names that resolution is to judge, in source order: those written
/// as project paths (anything else is E201 already) whose kind fits the form (a `.json`
/// path without `with`, or outside an import declaration, is reported by the parser).
// Extensions are compared exactly: a path names a file as it is on disk.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn named_paths(node: NodeRef<'_>, found: &mut Vec<Import>) {
    let kind = node.kind();
    if matches!(
        kind,
        NodeKind::ImportDecl | NodeKind::ExportDecl | NodeKind::ImportCall
    ) {
        let with = node.children().any(
            |child| matches!(child, Element::Node(inner) if inner.kind() == NodeKind::ImportAttributes),
        );
        let string = node.children().find_map(|child| match child {
            Element::Token(token) => match &token.token().kind {
                TokenKind::String(value) => Some((token.token().span, value.to_string())),
                _ => None,
            },
            Element::Node(_) => None,
        });
        if let Some((span, specifier)) = string {
            let json = specifier.ends_with(".json");
            if is_project_path(&specifier) && json == (with && kind == NodeKind::ImportDecl) {
                found.push(Import {
                    span,
                    specifier,
                    dynamic: kind == NodeKind::ImportCall,
                    target: None,
                });
            }
        }
    }
    for child in node.children() {
        if let Element::Node(inner) = child {
            named_paths(inner, found);
        }
    }
}
