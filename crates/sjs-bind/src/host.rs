//! What the loader asks of a file system, and the real one.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The file system as the module graph sees it. A trait so that a test can refuse a
/// read or count the calls.
pub trait Host {
    /// The one identity of `path`: absolute, without `.`, `..` or symbolic links.
    ///
    /// # Errors
    ///
    /// The I/O error when `path` names nothing.
    fn canonical(&self, path: &Path) -> io::Result<PathBuf>;

    /// The bytes of the file at `path`.
    ///
    /// # Errors
    ///
    /// The I/O error when it cannot be read.
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// The names of the entries of the directory `dir`, exactly as it spells them.
    ///
    /// # Errors
    ///
    /// The I/O error when it cannot be listed.
    fn names(&self, dir: &Path) -> io::Result<Vec<OsString>>;

    /// Whether `path` is a file (and not a directory).
    fn is_file(&self, path: &Path) -> bool;
}

/// The file system of the machine.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Disk;

impl Host for Disk {
    fn canonical(&self, path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn names(&self, dir: &Path) -> io::Result<Vec<OsString>> {
        // An entry that fails to read mid-iteration is skipped: `flatten` drops it.
        Ok(fs::read_dir(dir)?
            .flatten()
            .map(|entry| entry.file_name())
            .collect())
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}
