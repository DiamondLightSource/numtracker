use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use fd_lock::{RwLock, RwLockWriteGuard};

use crate::ScanNumberBackend;

#[derive(Debug, Clone)]
pub struct GdaNumTracker {
    directory: PathBuf,
}

/// Wrapper around file lock that creates, locks and then deletes the file
struct TempFileLock(PathBuf, RwLock<File>);

impl TempFileLock {
    fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        let lock = OpenOptions::new()
            .create_new(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        Ok(Self(path, RwLock::new(lock)))
    }
    fn lock(&mut self) -> Result<RwLockWriteGuard<'_, File>, std::io::Error> {
        self.1.try_write()
    }
}

impl Drop for TempFileLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl GdaNumTracker {
    const LOCK_FILE_NAME: &'static str = ".numtracker_lock";
    /// Create a new num tracker for the given directory and extension
    pub fn new<D: Into<PathBuf>>(directory: D) -> Self {
        let directory = directory.into();
        Self { directory }
    }

    /// Create a [TempFileLock] for the given extension in this tracker's directory
    ///
    /// The returned lock should locked prior to reading or modifying anything in this tracker's
    /// directory related to the given extension.
    /// ```ignore
    /// let _lock = self.file_lock()?;
    /// let _lock = _lock.lock()?;
    /// // ... rest of method
    /// // lock will be released and file deleted when dropped
    /// ```
    /// # Notes
    /// This is an advisory lock only and will only prevent concurrent access by other applications
    /// that are aware of and opt in to respecting this lock. It does not prevent access to the
    /// directory from other uses/processes that don't check.
    fn file_lock(&self, ext: &str) -> Result<TempFileLock, std::io::Error> {
        TempFileLock::new(
            self.directory
                .join(Self::LOCK_FILE_NAME)
                .with_extension(ext),
        )
    }

    /// Build the path of the file that would correspond to the given number
    fn file_name(&self, num: usize, ext: &str) -> PathBuf {
        // TODO: protect against extension based path traversal, eg ext='i22/../../somewhere_else'
        self.directory.join(format!("{num}.{ext}"))
    }

    /// Create a file named for the given number and, if present, remove the file for the previous
    /// number.
    fn create_num_file(&self, num: usize, ext: &str) -> Result<(), std::io::Error> {
        let next = self.file_name(num, ext);
        OpenOptions::new().create_new(true).write(true).open(next)?;
        if let Some(prev) = num.checked_sub(1) {
            let prev = self.file_name(prev, ext);
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
    /// # use numtracker::numtracker::GdaNumTracker;
    /// GdaNumTracker::new("/tmp/");
    /// ```
    fn default() -> Self {
        Self::new("/tmp/")
    }
}

impl ScanNumberBackend for GdaNumTracker {
    type NumberError = std::io::Error;

    async fn next_scan_number(&self, ext: &str) -> Result<usize, Self::NumberError> {
        // Nothing here is async but the trait expects an async method
        let mut _lock = self.file_lock(ext)?;
        let _f = _lock.lock()?;
        let next = self.high_file(ext)? + 1;
        self.create_num_file(next, ext)?;
        Ok(next)
    }
}
