//! A throwaway directory for tests that touch the file system, with nothing but `std`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A fresh directory under the system temp dir, removed on drop.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates `<system temp dir>/sjs-<label>-<pid>-<n>`.
    ///
    /// # Errors
    ///
    /// The I/O error when the directory cannot be created.
    pub fn new(label: &str) -> io::Result<Self> {
        Self::new_in(&std::env::temp_dir(), label)
    }

    /// Creates `<base>/sjs-<label>-<pid>-<n>`.
    ///
    /// # Errors
    ///
    /// The I/O error when the directory cannot be created (for example, `base` is a file).
    pub fn new_in(base: &Path, label: &str) -> io::Result<Self> {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!("sjs-{label}-{}-{n}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    /// The directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `contents` to `relative`, creating parent directories, and returns the path.
    ///
    /// # Errors
    ///
    /// The I/O error when a directory or the file cannot be written.
    pub fn write(&self, relative: &str, contents: &str) -> io::Result<PathBuf> {
        let path = self.path.join(relative);
        // A joined path always has a parent; the fallback only keeps the code branch-free.
        fs::create_dir_all(path.parent().unwrap_or(&self.path))?;
        fs::write(&path, contents)?;
        Ok(path)
    }

    /// Creates the directory `relative` (and its parents) and returns the path.
    ///
    /// # Errors
    ///
    /// The I/O error when it cannot be created.
    pub fn mkdir(&self, relative: &str) -> io::Result<PathBuf> {
        let path = self.path.join(relative);
        fs::create_dir_all(&path)?;
        Ok(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best effort: a leftover temp dir is not a test failure.
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::TempDir;

    #[test]
    fn creates_writes_and_removes_a_directory() {
        let dir = TempDir::new("fs").expect("temp dir");
        let path = dir.path().to_path_buf();
        assert!(path.is_dir());
        let file = dir.write("a/b/c.txt", "hello").expect("write");
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "hello");
        assert_eq!(file, path.join("a/b/c.txt"));
        let sub = dir.mkdir("d/e").expect("mkdir");
        assert!(sub.is_dir());
        assert_eq!(sub, path.join("d/e"));
        drop(dir);
        assert!(!path.exists(), "removed on drop");
    }

    #[test]
    fn a_base_that_is_a_file_is_an_error_not_a_panic() {
        let dir = TempDir::new("fs").expect("temp dir");
        let file = dir.write("base", "").expect("write");
        assert!(TempDir::new_in(&file, "fs").is_err());
        let nested = TempDir::new_in(dir.path(), "nested").expect("a directory base works");
        assert!(nested.path().starts_with(dir.path()));
    }

    #[test]
    fn two_temp_dirs_never_collide() {
        let a = TempDir::new("fs").expect("temp dir");
        let b = TempDir::new("fs").expect("temp dir");
        assert_ne!(a.path(), b.path());
    }

    #[test]
    fn writing_through_an_existing_file_as_a_directory_is_an_error_not_a_panic() {
        let dir = TempDir::new("fs").expect("temp dir");
        dir.write("blocker", "").expect("write");
        assert!(dir.write("blocker/inner.txt", "").is_err());
        assert!(dir.mkdir("blocker/sub").is_err());
        dir.mkdir("adir").expect("mkdir");
        assert!(
            dir.write("adir", "x").is_err(),
            "writing over a directory fails"
        );
    }
}
