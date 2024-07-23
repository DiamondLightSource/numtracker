//! Implementations of services designed to reproduce the behaviour of the equivalent classes used
//! in GDA.

use std::fs::{self, OpenOptions};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use fd_lock::RwLock;

use crate::NumTracker;

#[derive(Debug)]
pub struct GdaNumTracker {
    directory: PathBuf,
    extension: String,
    _lock_file: PathBuf,
}

#[derive(Debug)]
pub struct InvalidExtension;

impl GdaNumTracker {
    /// Create a new num tracker for the given directory and extension
    pub fn new<D: Into<PathBuf>>(
        directory: D,
        extension: String,
    ) -> Result<Self, InvalidExtension> {
        if extension
            .find(|c: char| !c.is_ascii_alphanumeric())
            .is_some()
        {
            return Err(InvalidExtension);
        }
        let directory = directory.into();
        let _lock_file = directory.join(format!(".numtracker_lock.{extension}"));
        Ok(Self {
            directory,
            extension,
            _lock_file,
        })
    }

    /// Create a [RwLock] for this tracker's directory lock file
    ///
    /// The returned lock should locked for writing prior to modifying anything in this tracker's
    /// directory.
    /// ```no_run
    /// let _lock = self.file_lock()?;
    /// let _lock = _lock.write()?;
    /// // ... rest of method
    /// // lock will be released when dropped
    /// ```
    /// # Notes
    /// This is an advisory lock only and will only prevent concurrent access by other applications
    /// that are aware of and opt in to respecting this lock. It does not prevent access to the
    /// directory from other uses/processes that don't check.
    fn file_lock(&self) -> Result<RwLock<impl AsFd>, std::io::Error> {
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self._lock_file)?;
        let _lock = RwLock::new(lock);
        Ok(_lock)
    }

    /// Build the path of the file that would correspond to the given number
    fn file_name(&self, num: usize) -> PathBuf {
        self.directory.join(format!("{num}.{}", self.extension))
    }

    /// Create a file named for the given number and, if present, remove the file for the previous
    /// number.
    fn create_num_file(&self, num: usize) -> Result<(), std::io::Error> {
        let next = self.file_name(num);
        OpenOptions::new().create_new(true).write(true).open(next)?;
        if let Some(prev) = num.checked_sub(1) {
            let prev = self.file_name(prev);
            let _ = fs::remove_file(prev);
        }
        Ok(())
    }

    /// Read the number corresponding to the given file if it is a valid file name
    ///
    /// Does not check that the file is a child of the current tracker's directory.
    fn file_num(&self, file: &Path) -> Option<usize> {
        if file.extension()?.to_str()? != self.extension {
            return None;
        }
        match file.file_stem()?.to_str()?.parse() {
            Ok(val) => Some(val),
            Err(_) => None,
        }
    }

    /// Find the highest number that has a corresponding number file in this tracker's directory
    fn high_file(&self) -> Result<usize, std::io::Error> {
        let mut high = 0;
        for file in self.directory.read_dir()? {
            let file = file?;
            if !file.file_type()?.is_file() {
                continue;
            }
            if let Some(val) = self.file_num(&file.path()) {
                high = high.max(val);
            }
        }
        Ok(high)
    }
}

impl Default for GdaNumTracker {
    /// Create a default GdaNumTracker using `/tmp/` as the directory and `tmp` as the extension
    ///
    /// Equivalent to
    /// ```rust
    /// GdaNumTracker::new("/tmp/", "tmp").unwrap();
    /// ```
    fn default() -> Self {
        Self::new("/tmp/", "tmp".into()).expect("tmp is valid extension")
    }
}

impl NumTracker for GdaNumTracker {
    type Err = std::io::Error;

    fn increment_and_get(&mut self) -> Result<usize, Self::Err> {
        let mut _lock = self.file_lock()?;
        let _f = _lock.try_write()?;
        let next = self.high_file()? + 1;
        self.create_num_file(next)?;
        Ok(next)
    }
}
