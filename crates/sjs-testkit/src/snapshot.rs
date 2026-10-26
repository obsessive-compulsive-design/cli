//! Snapshot assertions (STORIES.md §0.2 item 8): a text is compared byte for byte with
//! `tests/snapshots/<name>.snap`; `SJS_UPDATE_SNAPSHOTS=1` writes the file instead. A
//! mismatch fails with a unified diff.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Compare with the stored file, or overwrite it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fail on any difference (the default).
    Compare,
    /// Write the actual text as the new snapshot.
    Update,
}

impl Mode {
    /// `Update` when `SJS_UPDATE_SNAPSHOTS` is `1`, otherwise `Compare`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_var(std::env::var("SJS_UPDATE_SNAPSHOTS").ok().as_deref())
    }

    /// The mode for a variable's value.
    #[must_use]
    pub fn from_var(value: Option<&str>) -> Self {
        if value.map(str::trim) == Some("1") {
            Mode::Update
        } else {
            Mode::Compare
        }
    }
}

/// What happened to the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The stored text equals the actual text.
    Matched,
    /// The file was written (`Mode::Update`).
    Written(PathBuf),
}

/// Why the snapshot did not match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// The name would escape the snapshot directory.
    BadName(String),
    /// There is no stored snapshot yet.
    Missing(PathBuf),
    /// The stored text differs; the diff is expected → actual.
    Differs {
        /// The snapshot file.
        path: PathBuf,
        /// A unified diff from stored to actual.
        diff: String,
    },
    /// The file could not be read or written.
    Io(PathBuf, String),
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mismatch::BadName(name) => write!(f, "snapshot name {name:?} must be a plain file name"),
            Mismatch::Missing(path) => write!(
                f,
                "no snapshot at {}; run with SJS_UPDATE_SNAPSHOTS=1 to create it, then review the file",
                path.display()
            ),
            Mismatch::Differs { path, diff } => write!(f, "snapshot {} differs from the actual text:\n{diff}", path.display()),
            Mismatch::Io(path, error) => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for Mismatch {}

/// Compares `actual` with `<dir>/<name>.snap` (or writes it under `Mode::Update`).
///
/// # Errors
///
/// A bad name, a missing file in `Compare` mode, a difference, or an I/O failure.
pub fn check_in(dir: &Path, name: &str, actual: &str, mode: Mode) -> Result<Outcome, Mismatch> {
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(Mismatch::BadName(name.to_owned()));
    }
    let path = dir.join(format!("{name}.snap"));
    match mode {
        Mode::Update => {
            fs::create_dir_all(dir).map_err(|e| Mismatch::Io(dir.to_path_buf(), e.to_string()))?;
            fs::write(&path, actual).map_err(|e| Mismatch::Io(path.clone(), e.to_string()))?;
            Ok(Outcome::Written(path))
        }
        Mode::Compare => {
            let stored = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err(Mismatch::Missing(path))
                }
                Err(e) => return Err(Mismatch::Io(path, e.to_string())),
            };
            if stored == actual.as_bytes() {
                return Ok(Outcome::Matched);
            }
            let expected = String::from_utf8_lossy(&stored);
            let diff = unified_diff(&expected, actual, "stored", "actual");
            Err(Mismatch::Differs { path, diff })
        }
    }
}

/// [`check_in`] with the environment's mode, panicking on a mismatch.
///
/// # Panics
///
/// On any [`Mismatch`].
pub fn assert_in(dir: &Path, name: &str, actual: &str) {
    if let Err(mismatch) = check_in(dir, name, actual, Mode::from_env()) {
        panic!("{mismatch}");
    }
}

/// Asserts `$text` against `tests/snapshots/$name.snap` of the calling crate.
#[macro_export]
macro_rules! snapshot {
    ($name:expr, $text:expr) => {
        $crate::snapshot::assert_in(
            &::std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("snapshots"),
            $name,
            $text,
        )
    };
}

/// A unified diff over lines, with every line shown (no hunk headers: snapshots are
/// small and a reviewer wants the whole picture). A carriage return at the end of a line
/// is shown as `<CR>` so CRLF differences are visible.
#[must_use]
pub fn unified_diff(
    expected: &str,
    actual: &str,
    expected_label: &str,
    actual_label: &str,
) -> String {
    let a: Vec<&str> = expected.split('\n').collect();
    let b: Vec<&str> = actual.split('\n').collect();
    let table = lcs_table(&a, &b);
    let mut out = format!("--- {expected_label}\n+++ {actual_label}\n");
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            push_line(&mut out, ' ', a[i]);
            i += 1;
            j += 1;
        } else if i < a.len() && (j == b.len() || table[i + 1][j] >= table[i][j + 1]) {
            push_line(&mut out, '-', a[i]);
            i += 1;
        } else {
            push_line(&mut out, '+', b[j]);
            j += 1;
        }
    }
    out
}

/// `table[i][j]` is the length of the longest common subsequence of `a[i..]` and `b[j..]`.
fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let mut table = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i][j] = if a[i] == b[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    table
}

fn push_line(out: &mut String, sign: char, line: &str) {
    out.push(sign);
    match line.strip_suffix('\r') {
        Some(body) => {
            out.push_str(body);
            out.push_str("<CR>");
        }
        None => out.push_str(line),
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::{assert_in, check_in, unified_diff, Mismatch, Mode, Outcome};
    use crate::fs::TempDir;

    #[test]
    fn mode_comes_from_the_variable() {
        assert_eq!(Mode::from_var(None), Mode::Compare);
        assert_eq!(Mode::from_var(Some("0")), Mode::Compare);
        assert_eq!(Mode::from_var(Some("true")), Mode::Compare);
        assert_eq!(Mode::from_var(Some("1")), Mode::Update);
        assert_eq!(Mode::from_var(Some(" 1 ")), Mode::Update);
        let _ = Mode::from_env();
    }

    #[test]
    fn update_writes_then_compare_matches_byte_for_byte() {
        let dir = TempDir::new("snapshot").expect("temp dir");
        let snapshots = dir.path().join("deep").join("snapshots");
        let written = check_in(&snapshots, "hello", "a\r\nb\n", Mode::Update).expect("written");
        assert_eq!(written, Outcome::Written(snapshots.join("hello.snap")));
        assert_eq!(
            std::fs::read(snapshots.join("hello.snap")).expect("file"),
            b"a\r\nb\n"
        );
        assert_eq!(
            check_in(&snapshots, "hello", "a\r\nb\n", Mode::Compare),
            Ok(Outcome::Matched)
        );
        let crlf_vs_lf = check_in(&snapshots, "hello", "a\nb\n", Mode::Compare)
            .expect_err("line endings differ");
        assert!(
            matches!(&crlf_vs_lf, Mismatch::Differs { diff, .. } if diff.contains("-a<CR>\n+a\n")),
            "{crlf_vs_lf}"
        );
        assert_in(&snapshots, "hello", "a\r\nb\n");
    }

    #[test]
    fn a_missing_snapshot_is_reported_with_the_remedy() {
        let dir = TempDir::new("snapshot").expect("temp dir");
        let error = check_in(dir.path(), "nope", "x", Mode::Compare).expect_err("missing");
        assert_eq!(error, Mismatch::Missing(dir.path().join("nope.snap")));
        assert!(error.to_string().contains("SJS_UPDATE_SNAPSHOTS=1"));
    }

    #[test]
    fn bad_names_cannot_escape_the_directory() {
        let dir = TempDir::new("snapshot").expect("temp dir");
        for bad in ["", "a/b", "a\\b", ".", "..", "../x"] {
            assert_eq!(
                check_in(dir.path(), bad, "x", Mode::Update),
                Err(Mismatch::BadName(bad.to_owned())),
                "{bad:?}"
            );
        }
        assert_eq!(
            Mismatch::BadName("a/b".to_owned()).to_string(),
            "snapshot name \"a/b\" must be a plain file name"
        );
    }

    #[test]
    fn io_failures_are_reported_not_panicked() {
        let dir = TempDir::new("snapshot").expect("temp dir");
        let blocker = dir.write("blocker", "").expect("write");
        let error =
            check_in(&blocker, "x", "y", Mode::Update).expect_err("a file is not a directory");
        assert!(
            matches!(&error, Mismatch::Io(path, _) if *path == blocker),
            "{error}"
        );
        let under_file = blocker.join("sub");
        let error =
            check_in(&under_file, "x", "y", Mode::Compare).expect_err("cannot read under a file");
        assert!(format!("{error:?}").starts_with("Io("), "{error}");
        assert_eq!(
            Mismatch::Io("/p".into(), "boom".to_owned()).to_string(),
            "/p: boom"
        );
        let dir_as_file = check_in(dir.path(), "blocker", "y", Mode::Compare);
        assert!(dir_as_file.is_err());
        dir.mkdir("adir.snap").expect("mkdir");
        let error = check_in(dir.path(), "adir", "y", Mode::Compare)
            .expect_err("reading a directory fails");
        assert!(format!("{error:?}").starts_with("Io("), "{error}");
        let error = check_in(dir.path(), "adir", "y", Mode::Update)
            .expect_err("writing over a directory fails");
        assert!(format!("{error:?}").starts_with("Io("), "{error}");
    }

    #[test]
    fn the_diff_shows_context_removals_and_additions() {
        let diff = unified_diff("a\nb\nc\n", "a\nx\nc\nd\n", "stored", "actual");
        assert_eq!(diff, "--- stored\n+++ actual\n a\n-b\n+x\n c\n+d\n \n");
        assert_eq!(unified_diff("", "", "l", "r"), "--- l\n+++ r\n \n");
        assert_eq!(
            unified_diff("only", "", "l", "r"),
            "--- l\n+++ r\n-only\n+\n"
        );
        assert_eq!(
            unified_diff("", "only", "l", "r"),
            "--- l\n+++ r\n-\n+only\n"
        );
        assert_eq!(
            unified_diff("same", "same", "l", "r"),
            "--- l\n+++ r\n same\n"
        );
        assert_eq!(
            unified_diff("a\r\n", "a\n", "l", "r"),
            "--- l\n+++ r\n-a<CR>\n+a\n \n"
        );
        assert_eq!(
            unified_diff(
                "x
y", "x", "l", "r"
            ),
            "--- l
+++ r
 x
-y
",
            "trailing deletions"
        );
        assert_eq!(
            unified_diff(
                "x", "x
y", "l", "r"
            ),
            "--- l
+++ r
 x
+y
",
            "trailing additions"
        );
    }

    #[test]
    fn the_lcs_table_holds_suffix_lengths() {
        let table = super::lcs_table(&["a", "b", "a"], &["a", "a"]);
        assert_eq!(
            table,
            vec![vec![2, 1, 0], vec![1, 1, 0], vec![1, 1, 0], vec![0, 0, 0]]
        );
        assert_eq!(super::lcs_table(&[], &[]), vec![vec![0]]);
        assert_eq!(
            super::lcs_table(&["a"], &["b"]),
            vec![vec![0, 0], vec![0, 0]]
        );
    }

    #[test]
    fn the_diff_output_is_itself_snapshotted() {
        let stored = "const a = 1\nconst b = 2\nconst c = 3\n";
        let actual = "const a = 1\nconst b = 20\nconst c = 3\nconst d = 4\n";
        let diff = unified_diff(stored, actual, "stored", "actual");
        crate::snapshot!("unified-diff", &diff);
        let dir = TempDir::new("snapshot").expect("temp dir");
        check_in(dir.path(), "x", stored, Mode::Update).expect("written");
        let error = check_in(dir.path(), "x", actual, Mode::Compare).expect_err("differs");
        assert_eq!(
            error.to_string(),
            format!(
                "snapshot {} differs from the actual text:\n{diff}",
                dir.path().join("x.snap").display()
            )
        );
    }

    #[test]
    fn assert_in_panics_with_the_mismatch_text() {
        let dir = TempDir::new("snapshot").expect("temp dir");
        let caught = std::panic::catch_unwind(|| assert_in(dir.path(), "missing", "x"));
        let payload = caught.expect_err("panics");
        assert!(payload
            .downcast_ref::<String>()
            .expect("String")
            .contains("no snapshot at"));
    }
}
