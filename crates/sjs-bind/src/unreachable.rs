//! Unreachable modules (SPEC.md §5.3): what is on disk and what is the program never
//! differ, so a `.js` file beneath `./src` that the root does not reach is E207.

use crate::graph::{name_of, ModuleGraph};
use crate::host::Host;
use sjs_diag::{Code, Diagnostic, Diagnostics, Position, Span};
use std::collections::BTreeSet;

impl ModuleGraph {
    /// E207 at 1:1 of each `.js` file beneath `<base>/src` that is no module of the
    /// graph. `*.test.js` files are exempt (`sjs test` adds them as roots) and so is
    /// everything that is not `.js`. Nothing is ignored: a dotfile and a directory named
    /// `node_modules` are scanned like any other. Without `<base>/src` nothing is.
    #[must_use]
    // Extensions are compared exactly: the rule names files as they are on disk.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    pub fn unreachable(&self, host: &dyn Host) -> Diagnostics {
        let start = Position::new(1, 1);
        let mut diagnostics = Diagnostics::new();
        // Directories and files by canonical path, so a link adds nothing and a loop
        // of links ends.
        let mut seen = BTreeSet::new();
        let mut directories = Vec::from_iter(host.canonical(&self.base.join("src")));
        while let Some(directory) = directories.pop() {
            for name in host.names(&directory).unwrap_or_default() {
                let Ok(path) = host.canonical(&directory.join(name)) else {
                    continue;
                };
                if !seen.insert(path.clone()) {
                    continue;
                }
                if !host.is_file(&path) {
                    directories.push(path);
                    continue;
                }
                let file = path.to_string_lossy();
                if file.ends_with(".js") && !file.ends_with(".test.js") && self.get(&path).is_none()
                {
                    let name = name_of(&self.base, &path);
                    let message = format!(
                        "`{name}` is not reached from the root: import it or delete it (SPEC.md §5.3)"
                    );
                    let span = Span::new(start, start, 0, 0);
                    diagnostics.push(Diagnostic::new(Code::E207, name, span, message));
                }
            }
        }
        diagnostics
    }
}
