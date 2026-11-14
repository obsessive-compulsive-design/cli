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
//! (US-060: E205 for a path that names no module).

use sjs_bind::{Host, LoadError, ModuleGraph};
use sjs_diag::{Diagnostics, SourceFile};
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
            }
        }
    }
    diagnostics
}

/// What the module graph of the program at `roots` finds (US-060): E205 for each path
/// that names no module. Files are named relative to `base`.
///
/// # Errors
///
/// A root that cannot be loaded.
pub fn check_graph(
    host: &dyn Host,
    base: &Path,
    roots: &[PathBuf],
) -> Result<Diagnostics, LoadError> {
    ModuleGraph::load(host, base, roots).map(|(_, diagnostics)| diagnostics)
}

#[cfg(test)]
mod tests {
    use super::{check_files, check_graph, InputFile};
    use sjs_bind::Disk;
    use sjs_diag::{Code, Position};
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
            file("a.js", b"const a = 1\n"),
            file("b.js", "// \u{e9}\n".as_bytes()),
        ]);
        assert!(diagnostics.is_empty());
        assert!(check_files(&[]).is_empty());
    }

    #[test]
    fn each_invalid_file_reports_its_own_e000() {
        let diagnostics = check_files(&[
            file("bom.js", b"\xEF\xBB\xBFx"),
            file("ok.js", b"x\n"),
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
