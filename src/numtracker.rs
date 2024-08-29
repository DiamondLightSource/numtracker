use std::fs as std_fs;
use std::io::Error;
use std::path::{Path, PathBuf};

use fd_lock::{RwLock, RwLockWriteGuard};
use tokio::fs as async_fs;
use tracing::{instrument, trace};

#[derive(Debug, Clone)]
pub struct GdaNumTracker {
    directory: PathBuf,
}

pub async fn increment_and_get<P: AsRef<Path>>(dir: P, ext: &str) -> Result<usize, std::io::Error> {
    GdaNumTracker::new(dir.as_ref()).next_scan_number(ext).await
}

/// Wrapper around file lock that creates, locks and then deletes the file
struct TempFileLock(PathBuf, RwLock<std_fs::File>);

impl TempFileLock {
    fn new(path: PathBuf) -> Result<Self, std::io::Error> {
        let lock = std_fs::OpenOptions::new()
            .create_new(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        Ok(Self(path, RwLock::new(lock)))
    }
    fn lock(&mut self) -> Result<RwLockWriteGuard<'_, std_fs::File>, std::io::Error> {
        self.1.try_write()
    }
}

impl Drop for TempFileLock {
    fn drop(&mut self) {
        trace!("Removing temporary lock file: {:?}", self.0);
        let _ = std_fs::remove_file(&self.0);
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
    // #[instrument]
    fn file_lock(&self, ext: &str) -> Result<TempFileLock, std::io::Error> {
        trace!("Creating new file lock for ext: {ext:?}");
        TempFileLock::new(
            self.directory
                .join(Self::LOCK_FILE_NAME)
                .with_extension(ext),
        )
    }

    /// Build the path of the file that would correspond to the given number
    fn file_name(&self, num: usize, ext: &str) -> PathBuf {
        // TODO: protect against extension based path traversal, eg ext='i22/../../somewhere_else'
        self.directory.join(num.to_string()).with_extension(ext)
    }

    /// Create a file named for the given number and, if present, remove the file for the previous
    /// number.
    // #[instrument]
    async fn create_num_file(&self, num: usize, ext: &str) -> Result<(), std::io::Error> {
        trace!("Creating new scan number file: {num}.{ext}");
        let next = self.file_name(num, ext);
        async_fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(next)
            .await?;
        if let Some(prev) = num.checked_sub(1) {
            let prev = self.file_name(prev, ext);
            let _ = async_fs::remove_file(prev).await;
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
    pub async fn latest_scan_number(&self, ext: &str) -> Result<usize, std::io::Error> {
        let mut high = 0;
        let mut dir = async_fs::read_dir(&self.directory).await?;
        while let Some(file) = dir.next_entry().await? {
            if !file.file_type().await?.is_file() {
                continue;
            }
            if let Some(val) = self.file_num(&file.path(), ext) {
                high = high.max(val);
            }
        }
        Ok(high)
    }
    #[instrument]
    pub async fn next_scan_number(&self, ext: &str) -> Result<usize, std::io::Error> {
        let mut _lock = self.file_lock(ext)?;
        let _f = _lock.lock()?;
        let next = self.latest_scan_number(ext).await? + 1;
        self.create_num_file(next, ext).await?;
        Ok(next)
    }

    /// Create a new scan number file for the given number and extension, and ensure that there are
    /// no other matching files with higher numbers.
    pub async fn set_scan_number(&self, ext: &str, num: usize) -> Result<(), std::io::Error> {
        let next = self.file_name(num, ext);
        async_fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(next)
            .await?;
        let mut dir = async_fs::read_dir(&self.directory).await?;
        while let Some(file) = dir.next_entry().await? {
            if !file.file_type().await?.is_file() {
                continue;
            }
            if let Some(val) = self.file_num(&file.path(), ext) {
                if val > num {
                    async_fs::remove_file(&file.path()).await?;
                }
            }
        }
        Ok(())
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
