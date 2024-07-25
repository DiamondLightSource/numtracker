use std::fs::{self, OpenOptions};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use fd_lock::RwLock;

use crate::BeamlineContext;

pub trait NumTracker {
    type Err;
    /// Get the next value from this tracker - every call should result in a new number
    ///
    /// If a call fails, the next successful call may or may not reflect that there were
    /// unsuccessful attempts since the last value returned.
    fn increment_and_get(&mut self, ctx: &BeamlineContext) -> Result<usize, Self::Err>;
}

#[derive(Debug)]
pub struct GdaNumTracker {
    directory: PathBuf,
    _lock_file: PathBuf,
}

impl GdaNumTracker {
    /// Create a new num tracker for the given directory and extension
    pub fn new<D: Into<PathBuf>>(directory: D) -> Self {
        let directory = directory.into();
        let _lock_file = directory.join(format!(".numtracker_lock"));
        Self {
            directory,
            _lock_file,
        }
    }

    /// Create a [RwLock] for this tracker's directory lock file
    ///
    /// The returned lock should locked for writing prior to modifying anything in this tracker's
    /// directory.
    /// ```ignore
    /// let _lock = self.file_lock()?;
    /// let _lock = _lock.write()?;
    /// // ... rest of method
    /// // lock will be release when dropped
    /// ```
    /// # Notes
    /// This is an advisory lock only and will only prevent concurrent access by other applications
    /// that are aware of and opt in to respecting this lock. It does not prevent access to the
    /// directory from other uses/processes that don't check.
    fn file_lock(&self) -> Result<RwLock<impl AsFd>, std::io::Error> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self._lock_file)?;
        let _lock = RwLock::new(lock);
        Ok(_lock)
    }

    /// Build the path of the file that would correspond to the given number
    fn file_name(&self, num: usize, ext: &str) -> PathBuf {
        // TODO: protect against extension based path traversal, eg ext='i22/../../somewhere_else'
        self.directory.join(format!("{num}.{ext}"))
    }

    /// Create a file named for the given number and, if present, remove the file for the previous
    /// number.
    fn create_num_file(&self, num: usize, ext: &str) -> Result<(), std::io::Error> {
        let next = self.file_name(num, &ext);
        OpenOptions::new().create_new(true).write(true).open(next)?;
        if let Some(prev) = num.checked_sub(1) {
            let prev = self.file_name(prev, &ext);
            let _ = fs::remove_file(prev);
        }
        Ok(())
    }

    /// Read the number corresponding to the given file if it is a valid file name
    ///
    /// Does not check that the file is a child of the current tracker's directory.
    fn file_num(&self, file: &Path, ext: &str) -> Option<usize> {
        if ext != file.extension()?.to_str()? {
            return None;
        }
        match file.file_stem()?.to_str()?.parse() {
            Ok(val) => Some(val),
            Err(_) => None,
        }
    }

    /// Find the highest number that has a corresponding number file in this tracker's directory
    fn high_file(&self, ext: &str) -> Result<usize, std::io::Error> {
        let mut high = 0;
        for file in self.directory.read_dir()? {
            let file = file?;
            if !file.file_type()?.is_file() {
                continue;
            }
            if let Some(val) = self.file_num(&file.path(), ext) {
                high = high.max(val);
            }
        }
        Ok(high)
    }
}

impl Default for GdaNumTracker {
    /// Create a default GdaNumTracker using `/tmp/` as the directory
    ///
    /// Equivalent to
    /// ```rust
    /// # use numtracker::gda::GdaNumTracker;
    /// GdaNumTracker::new("/tmp/");
    /// ```
    fn default() -> Self {
        Self::new("/tmp/")
    }
}

impl NumTracker for GdaNumTracker {
    type Err = std::io::Error;

    fn increment_and_get(&mut self, ctx: &BeamlineContext) -> Result<usize, Self::Err> {
        let mut _lock = self.file_lock()?;
        let _f = _lock.try_write()?;
        let next = self.high_file(ctx.instrument.as_ref())? + 1;
        self.create_num_file(next, ctx.instrument.as_ref())?;
        Ok(next)
    }
}
