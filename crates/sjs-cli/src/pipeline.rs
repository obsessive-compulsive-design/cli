//! The passes of SPEC.md §11, applied to a set of files, producing diagnostics.
//!
//! Each story that lands a pass extends [`check_files`]; the conformance harness
//! (`tests/conformance.rs`) runs every fixture through it, so a fixture can only expect
//! what the pipeline can already find. Today that is US-004 (UTF-8 and BOM validation)
//! the parser (US-010 to US-033: E000 for what the grammar rejects, E001 and E002, S001,
//! S002 and S012, E201, E202 and E205) and the formatter's S005 (US-041), S007 and S008
//! (US-042), S004 and S011 (US-043), S010 (US-044), S003 (US-045), S006 (US-046), S009
//! (US-047), and the S006, S007 and S008 of every other change the layout makes
//! (US-048). [`check_graph`] is the module graph's pass over a program on disk
//! (US-060: E205 for a path that names no module; US-061: E203 for an import cycle; US-062: E207 for an unreachable module;
//! US-067: E206 for a name a module does not export, E137 for a namespace's computed key).

use sjs_bind::{Host, LoadError, ModuleGraph};
use sjs_diag::{Code, Diagnostics, SourceFile};
use std::path::{Path, PathBuf};

/// A file of the program as the harness or a command hands it over: the path the
/// reporter prints and the raw bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFile {
    /// Path as printed in diagnostics (relative to the root's directory).
    pub path: String,
    /// Raw bytes; validation happens inside.
    pub bytes: Vec<u8>,
}

/// Runs every pass that exists over `files` and collects their findings.
#[must_use]
pub fn check_files(files: &[InputFile]) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    for file in files {
        match SourceFile::new(file.path.clone(), file.bytes.clone()) {
            Err(diagnostic) => diagnostics.push(*diagnostic),
            Ok(source) => {
                let (tree, found) = sjs_syntax::parse(&source);
                diagnostics.extend(found.iter().cloned());
                // The formatter's S-codes (US-041 to US-048): its passes' findings and
                // one for every other change its layout makes.
                let formatter = sjs_format::Formatter::new(&source, &tree);
                diagnostics.extend(formatter.report(&found).findings);
                // The binder's findings (US-063: E000 for a name declared twice, E135
                // for a use before its declaration), for a file the grammar accepts:
                // the names of a tree that was recovered are a guess.
                let rejected = found
                    .iter()
                    .any(|d| matches!(d.code, Code::E000 | Code::E001 | Code::E002));
                if !rejected {
                    let (bindings, bound) = sjs_bind::bind(&source, &tree);
                    diagnostics.extend(bound.iter().cloned());
                    // What the expressions say of themselves (US-083): E100 for a
                    // `null` the program writes, E125 for a template filled with what
                    // is no text.
                    let facts = sjs_infer::Facts::default();
                    let mut types = sjs_types::Types::new();
                    let found_typed =
                        sjs_infer::check_expressions(&source, &tree, &bindings, &facts, &mut types);
                    diagnostics.extend(found_typed.iter().cloned());
                }
            }
        }
    }
    diagnostics
}

/// What the module graph of the program at `roots` finds: E205 for each path that names
/// no module (US-060), E203 for each import cycle (US-061) and E207 for each `.js` file
/// beneath `<base>/src` the roots do not reach (US-062), E206 for each name asked of a
/// module that does not export it and E137 for a namespace read by a computed key
/// (US-067). Files are named relative to `base`, the project's directory.
///
/// # Errors
///
/// A root that cannot be loaded.
pub fn check_graph(
    host: &dyn Host,
    base: &Path,
    roots: &[PathBuf],
) -> Result<Diagnostics, LoadError> {
    ModuleGraph::load(host, base, roots).map(|(graph, mut diagnostics)| {
        diagnostics.extend(graph.import_cycles().iter().cloned());
        diagnostics.extend(graph.unreachable(host).iter().cloned());
        diagnostics.extend(graph.link().1.iter().cloned());
        diagnostics
    })
}

#[cfg(test)]
mod tests {
    use super::Code;
    use super::{check_files, check_graph, InputFile};
    use sjs_bind::Disk;
    use sjs_diag::Position;
    use sjs_testkit::fs::TempDir;

    #[test]
    fn the_graph_of_a_program_on_disk_reports_its_unresolved_imports() {
        let dir = TempDir::new("pipeline").expect("temp dir");
        let entry = dir
            .write("entry.js", "import { a } from \"./a.js\"\n")
            .expect("write");
        let found = check_graph(&Disk, dir.path(), std::slice::from_ref(&entry)).expect("loads");
        let found: Vec<_> = found
            .iter()
            .map(|d| (d.path.as_str(), d.code, d.span.start))
            .collect();
        assert_eq!(found, [("entry.js", Code::E205, Position::new(1, 19))]);
        dir.write("a.js", "export const a = 1\n").expect("write");
        assert!(check_graph(&Disk, dir.path(), &[entry])
            .expect("loads")
            .is_empty());
        dir.write("a.js", "export { a } from \"./entry.js\"\n")
            .expect("write");
        let root = [dir.path().join("entry.js")];
        let found = check_graph(&Disk, dir.path(), &root).expect("loads");
        let found: Vec<_> = found
            .iter()
            .map(|d| (d.path.as_str(), d.code, d.span.start))
            .collect();
        assert_eq!(
            found,
            [
                ("a.js", Code::E206, Position::new(1, 10)),
                ("entry.js", Code::E203, Position::new(1, 19))
            ]
        );
        dir.write("src/orphan.js", "").expect("write");
        let found = check_graph(&Disk, dir.path(), &root).expect("loads");
        let found: Vec<_> = found
            .iter()
            .map(|d| (d.path.as_str(), d.code, d.span.start))
            .collect();
        assert_eq!(
            found,
            [
                ("a.js", Code::E206, Position::new(1, 10)),
                ("entry.js", Code::E203, Position::new(1, 19)),
                ("src/orphan.js", Code::E207, Position::new(1, 1))
            ]
        );
        let gone = dir.path().join("gone.js");
        let error =
            check_graph(&Disk, dir.path(), std::slice::from_ref(&gone)).expect_err("no root");
        assert_eq!(error.path, gone);
    }

    fn file(path: &str, bytes: &[u8]) -> InputFile {
        InputFile {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn valid_files_produce_nothing() {
        let diagnostics = check_files(&[
            file("a.js", b"export const a = 1\n"),
            file("b.js", "// \u{e9}\n".as_bytes()),
        ]);
        assert!(diagnostics.is_empty());
        assert!(check_files(&[]).is_empty());
    }

    #[test]
    fn each_invalid_file_reports_its_own_e000() {
        let diagnostics = check_files(&[
            file("bom.js", b"\xEF\xBB\xBFx"),
            file("ok.js", b"undefined\n"),
            file("bad.js", b"a\xFF"),
        ]);
        let found: Vec<(&str, Code, Position)> = diagnostics
            .iter()
            .map(|d| (d.path.as_str(), d.code, d.span.start))
            .collect();
        assert_eq!(
            found,
            [
                ("bad.js", Code::E000, Position::new(1, 2)),
                ("bom.js", Code::E000, Position::new(1, 1)),
            ]
        );
    }
}
