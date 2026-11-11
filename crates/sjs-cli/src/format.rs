//! `sjs format` (US-049, SPEC.md §13.4): the unified diff of every root against its
//! canonical layout on stdout, and the count of files that would change as the exit
//! code; with `--write`, the canonical text written in place of each such file, through
//! a temporary file beside it that is renamed over it, its permissions kept. A file the
//! parser reports E000 or E001 for cannot be laid out safely: it is reported, left as
//! it is, and counted. Nothing but the layout is ever applied: S020, S021 and S030 are
//! findings for a person, not edits.

use std::fmt::Write as _;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use sjs_diag::{Code, Diagnostic, SourceFile};
use sjs_format::{unified_diff, Formatter};

use crate::exit::Outcome;
use crate::options::{CommandOptions, Options};

/// A file whose canonical text differs from what it holds.
struct Change {
    path: PathBuf,
    display: String,
    canonical: String,
    permissions: fs::Permissions,
}

/// Runs `sjs format` over the roots of `options`, paths printed relative to `cwd`.
pub(crate) fn run(
    options: &Options,
    cwd: &Path,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<Outcome> {
    let write = options.specific == CommandOptions::Format { write: true };
    let base = fs::canonicalize(cwd).unwrap_or(cwd.to_path_buf());
    let mut diffs = String::new();
    let mut reports = String::new();
    let mut changes = Vec::new();
    let mut skipped = 0;
    for path in &options.entries {
        let display = path
            .strip_prefix(&base)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let (bytes, permissions) = fs::metadata(path)
            .and_then(|metadata| fs::read(path).map(|bytes| (bytes, metadata.permissions())))
            .map_err(|error| with_path(&display, &error))?;
        match canonical_of(&display, bytes) {
            Err(found) => {
                skipped += 1;
                for d in &found {
                    let at = d.span.start;
                    let _ = writeln!(
                        reports,
                        "{display}:{}:{}: {} {}",
                        at.line,
                        at.column,
                        d.code.as_str(),
                        d.message
                    );
                }
            }
            Ok((text, canonical)) => {
                if text != canonical {
                    diffs.push_str(&unified_diff(&display, &text, &canonical));
                    changes.push(Change {
                        path: path.clone(),
                        display,
                        canonical,
                        permissions,
                    });
                }
            }
        }
    }
    let mut summary = format!("sjs format: {} ", files(changes.len()));
    summary.push_str(if write { "written" } else { "would change" });
    if skipped > 0 {
        let _ = write!(summary, ", {} skipped", files(skipped));
    }
    summary.push('\n');
    let count = if write {
        // Every file is checked before any is written: one that cannot be replaced
        // leaves the tree as it was.
        if let Some(locked) = changes.iter().find(|c| c.permissions.readonly()) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{} is read-only: nothing was written", locked.display),
            ));
        }
        for change in &changes {
            write_atomically(&change.path, &change.canonical, &change.permissions)
                .map_err(|error| with_path(&change.display, &error))?;
        }
        skipped
    } else {
        changes.len() + skipped
    };
    if !(write || options.quiet || options.json) {
        stdout.write_all(diffs.as_bytes())?;
    }
    if !options.quiet {
        summary.insert_str(0, &reports);
    }
    stderr.write_all(summary.as_bytes())?;
    Ok(Outcome::FilesWouldChange(count))
}

/// The text of a file and its canonical layout, or the E000 and E001 findings that
/// keep it from being laid out.
fn canonical_of(display: &str, bytes: Vec<u8>) -> Result<(String, String), Vec<Diagnostic>> {
    let source = SourceFile::new(display, bytes).map_err(|d| vec![*d])?;
    let (tree, found) = sjs_syntax::parse(&source);
    let unsafe_to_format: Vec<Diagnostic> = found
        .iter()
        .filter(|d| matches!(d.code, Code::E000 | Code::E001))
        .cloned()
        .collect();
    if !unsafe_to_format.is_empty() {
        return Err(unsafe_to_format);
    }
    let canonical = Formatter::new(&source, &tree).module();
    Ok((source.text().to_owned(), canonical))
}

/// Replaces `path` with `text`: written to a temporary file beside it, given the
/// permissions `path` had, then renamed over it, so no reader sees half a file.
fn write_atomically(path: &Path, text: &str, permissions: &fs::Permissions) -> io::Result<()> {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let temp = path.with_file_name(format!(".{name}.sjs-{}.tmp", std::process::id()));
    let result = fs::write(&temp, text)
        .and_then(|()| fs::set_permissions(&temp, permissions.clone()))
        .and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn with_path(display: &str, error: &io::Error) -> io::Error {
    io::Error::new(error.kind(), format!("{display}: {error}"))
}

fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

#[cfg(test)]
mod tests {
    use crate::{run_in, EXIT_INTERNAL};
    use sjs_testkit::fs::TempDir;
    use std::ffi::OsString;
    use std::fs;
    use std::io::{self, Write};
    use std::path::Path;

    const DIFF: &str =
        "--- src/index.js\n+++ src/index.js\n@@ -1 +1 @@\n-const a=1\n+const a = 1\n";

    fn project(index: &str) -> TempDir {
        let dir = TempDir::new("format").expect("temp dir");
        dir.write("src/index.js", index).expect("write");
        dir
    }

    fn sjs_at(cwd: &Path, list: &[&str]) -> (u8, String, String) {
        let args: Vec<OsString> = list.iter().map(OsString::from).collect();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_in(args, cwd, &mut out, &mut err);
        let text = |bytes: Vec<u8>| String::from_utf8(bytes).expect("utf-8");
        (code, text(out), text(err))
    }

    fn sjs(dir: &TempDir, list: &[&str]) -> (u8, String, String) {
        sjs_at(dir.path(), list)
    }

    fn read(dir: &TempDir, relative: &str) -> String {
        fs::read_to_string(dir.path().join(relative)).expect("read")
    }

    fn set_readonly(path: &Path, readonly: bool) {
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_readonly(readonly);
        fs::set_permissions(path, permissions).expect("set permissions");
    }

    #[test]
    fn the_default_prints_the_diff_and_exits_with_the_count() {
        let dir = project("const a=1\n");
        let expected = (
            1,
            DIFF.to_owned(),
            "sjs format: 1 file would change\n".to_owned(),
        );
        assert_eq!(sjs(&dir, &["format"]), expected);
        assert_eq!(
            sjs(&dir, &["format", "--check"]),
            expected,
            "the default spelled out"
        );
        assert_eq!(
            read(&dir, "src/index.js"),
            "const a=1\n",
            "nothing is written"
        );
        let quiet = (
            1,
            String::new(),
            "sjs format: 1 file would change\n".to_owned(),
        );
        assert_eq!(sjs(&dir, &["format", "--quiet"]), quiet);
        assert_eq!(
            sjs(&dir, &["format", "--json"]),
            quiet,
            "stdout is the document's"
        );
        let clean = project("const a = 1\n");
        assert_eq!(
            sjs(&clean, &["format"]),
            (
                0,
                String::new(),
                "sjs format: 0 files would change\n".to_owned()
            )
        );
    }

    #[test]
    fn the_count_is_of_files_once_each_and_capped() {
        let dir = project("const a=1\n");
        dir.write("b.js", "f( )\n").expect("write");
        let (code, out, err) = sjs(&dir, &["format", "src/index.js", "b.js", "./src/index.js"]);
        assert_eq!(code, 2, "a file given twice counts once");
        assert_eq!(out.matches("--- ").count(), 2, "{out}");
        assert!(out.starts_with(DIFF), "in the order given: {out}");
        assert_eq!(err, "sjs format: 2 files would change\n");
        let mut args = vec!["format".to_owned()];
        for n in 0..256 {
            let name = format!("f{n}.js");
            dir.write(&name, "f( )\n").expect("write");
            args.push(name);
        }
        let list: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, _, err) = sjs(&dir, &list);
        assert_eq!(code, 255, "capped");
        assert_eq!(err, "sjs format: 256 files would change\n");
    }

    #[test]
    fn a_path_outside_the_working_directory_is_printed_whole() {
        let dir = project("const a=1\n");
        dir.write("sub/x.js", "").expect("write");
        let (code, out, _) = sjs_at(&dir.path().join("sub"), &["format", "../src/index.js"]);
        assert_eq!(code, 1);
        let line = out.lines().next().unwrap_or("");
        assert!(
            line.starts_with("--- /") && line.ends_with("/src/index.js"),
            "{out}"
        );
    }

    #[test]
    fn write_replaces_each_file_and_exits_zero() {
        let dir = project("const a=1\n");
        dir.write("src/other.js", "f( )\n").expect("write");
        assert_eq!(
            sjs(&dir, &["format", "--write"]),
            (0, String::new(), "sjs format: 1 file written\n".to_owned())
        );
        assert_eq!(read(&dir, "src/index.js"), "const a = 1\n");
        assert_eq!(
            read(&dir, "src/other.js"),
            "f( )\n",
            "a file that is no root is not touched"
        );
        let names: Vec<String> = fs::read_dir(dir.path().join("src"))
            .expect("read dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names.len(), 2, "no temporary file is left: {names:?}");
        assert_eq!(
            sjs(&dir, &["format", "--write"]),
            (0, String::new(), "sjs format: 0 files written\n".to_owned()),
            "a second run has nothing to do"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_keeps_the_mode_of_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project("const a=1\n");
        let path = dir.path().join("src/index.js");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(sjs(&dir, &["format", "--write"]).0, 0);
        assert_eq!(read(&dir, "src/index.js"), "const a = 1\n");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn a_file_that_cannot_be_parsed_safely_is_reported_skipped_and_counted() {
        let dir = project("const a=1\n");
        fs::write(dir.path().join("bytes.js"), b"a\xFF\n").expect("write");
        dir.write("e000.js", "let x = ;\n").expect("write");
        dir.write("e001.js", "x = a\n(b).c\n").expect("write");
        dir.write("s002.js", "f();\n").expect("write");
        let list = ["src/index.js", "bytes.js", "e000.js", "e001.js", "s002.js"];
        let check: Vec<&str> = ["format"].iter().chain(&list).copied().collect();
        let (code, out, err) = sjs(&dir, &check);
        assert_eq!(code, 5, "two would change, three are skipped");
        assert_eq!(out.matches("--- ").count(), 2, "{out}");
        for (name, code) in [
            ("bytes.js:1:2", "E000"),
            ("e000.js:1:", "E000"),
            ("e001.js:2:1", "E001"),
        ] {
            assert!(
                err.lines()
                    .any(|line| line.starts_with(name) && line.contains(&format!(": {code} "))),
                "{name} {code}: {err}"
            );
        }
        assert!(
            !err.contains("s002.js"),
            "a style finding skips nothing: {err}"
        );
        assert!(
            err.ends_with("sjs format: 2 files would change, 3 files skipped\n"),
            "{err}"
        );
        let write: Vec<&str> = ["format", "--write", "--quiet"]
            .iter()
            .chain(&list)
            .copied()
            .collect();
        assert_eq!(
            sjs(&dir, &write),
            (
                3,
                String::new(),
                "sjs format: 2 files written, 3 files skipped\n".to_owned()
            ),
            "quiet: the summary alone; the exit code counts the files skipped"
        );
        assert_eq!(read(&dir, "src/index.js"), "const a = 1\n");
        assert_eq!(read(&dir, "s002.js"), "f()\n");
        assert_eq!(read(&dir, "e000.js"), "let x = ;\n");
        assert_eq!(read(&dir, "e001.js"), "x = a\n(b).c\n");
        assert_eq!(
            fs::read(dir.path().join("bytes.js")).expect("read"),
            b"a\xFF\n"
        );
        dir.write("one.js", "let x = ;\n").expect("write");
        let (code, _, err) = sjs(&dir, &["format", "one.js"]);
        assert_eq!(code, 1);
        assert!(
            err.ends_with("sjs format: 0 files would change, 1 file skipped\n"),
            "{err}"
        );
    }

    #[test]
    fn a_read_only_file_stops_write_before_anything_is_written() {
        let dir = project("const a=1\n");
        let locked = dir.write("locked.js", "f( )\n").expect("write");
        set_readonly(&locked, true);
        let (code, out, err) = sjs(&dir, &["format", "--write", "src/index.js", "locked.js"]);
        assert_eq!((code, out.as_str()), (EXIT_INTERNAL, ""));
        assert_eq!(
            err,
            "sjs: internal failure: locked.js is read-only: nothing was written\n"
        );
        assert_eq!(
            read(&dir, "src/index.js"),
            "const a=1\n",
            "nothing was written"
        );
        assert_eq!(read(&dir, "locked.js"), "f( )\n");
        set_readonly(&locked, false);
        fs::write(&locked, "f()\n").expect("write");
        set_readonly(&locked, true);
        let (code, _, _) = sjs(&dir, &["format", "--write", "src/index.js", "locked.js"]);
        assert_eq!(code, 0, "a read-only file already canonical is no obstacle");
        assert_eq!(read(&dir, "src/index.js"), "const a = 1\n");
        set_readonly(&locked, false);
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_read_or_replaced_is_an_internal_failure() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project("const a=1\n");
        let src = dir.path().join("src");
        fs::set_permissions(&src, fs::Permissions::from_mode(0o555)).expect("chmod");
        let (code, _, err) = sjs(&dir, &["format", "--write"]);
        fs::set_permissions(&src, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(code, EXIT_INTERNAL);
        assert!(
            err.starts_with("sjs: internal failure: src/index.js: "),
            "{err}"
        );
        assert_eq!(read(&dir, "src/index.js"), "const a=1\n");
        let index = src.join("index.js");
        fs::set_permissions(&index, fs::Permissions::from_mode(0o000)).expect("chmod");
        let (code, _, err) = sjs(&dir, &["format"]);
        fs::set_permissions(&index, fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(code, EXIT_INTERNAL);
        assert!(
            err.starts_with("sjs: internal failure: src/index.js: "),
            "{err}"
        );
    }

    #[test]
    fn only_the_layout_is_applied_never_s020_s021_or_s030() {
        let source =
            "const o = {a: 1}\n/** @deprecated */\nexport function old(x) {return o}\nold(1)\n";
        let dir = project(source);
        assert_eq!(sjs(&dir, &["format", "--write"]).0, 0);
        assert_eq!(
            read(&dir, "src/index.js"),
            "const o = { a: 1 }\n\n/** @deprecated */\nexport function old(x) {\n  return o\n}\n\nold(1)\n",
            "no freeze, no narrowed signature, no call rewritten"
        );
    }

    /// A stream that fails every write.
    struct Broken;

    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("closed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_stream_that_fails_is_an_internal_failure() {
        let dir = project("const a=1\n");
        let args = || vec![OsString::from("format")];
        let mut sink = Vec::new();
        assert_eq!(
            run_in(args(), dir.path(), &mut Broken, &mut sink),
            EXIT_INTERNAL
        );
        assert_eq!(
            run_in(args(), dir.path(), &mut sink, &mut Broken),
            EXIT_INTERNAL
        );
        assert_eq!(Broken.flush().ok(), Some(()));
    }
}
