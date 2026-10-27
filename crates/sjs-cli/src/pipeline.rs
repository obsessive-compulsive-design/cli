//! The passes of SPEC.md §11, applied to a set of files, producing diagnostics.
//!
//! Each story that lands a pass extends [`check_files`]; the conformance harness
//! (`tests/conformance.rs`) runs every fixture through it, so a fixture can only expect
//! what the pipeline can already find. Today that is US-004 (UTF-8 and BOM validation)
//! and the lexer (US-010 to US-018: E000 for what the lexer rejects, E002 for regex
//! flags, S001 for single-quoted strings).

use sjs_diag::{Diagnostics, SourceFile};

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
                let (_, found) = sjs_syntax::lex(&source);
                diagnostics.extend(found.iter().cloned());
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::{check_files, InputFile};
    use sjs_diag::{Code, Position};

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
            file("ok.js", b"x"),
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
