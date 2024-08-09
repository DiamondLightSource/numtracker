use std::fmt::Display;
use std::path::{Component, Path, PathBuf};

use numtracker::NumTracker;

pub mod db_service;
pub mod numtracker;
pub mod paths;
pub(crate) mod template;

pub struct BeamlineContext {
    instrument: String,
    visit: String,
}

pub struct ScanContext<'a> {
    subdirectory: Subdirectory,
    scan_number: usize,
    beamline: &'a BeamlineContext,
}

pub struct DetectorContext<'a> {
    detector: Detector,
    scan: &'a ScanContext<'a>,
}

#[derive(Debug)]
pub struct Detector(String);
impl Detector {
    const INVALID: fn(char) -> bool = |c| !c.is_ascii_alphanumeric();
}

impl From<String> for Detector {
    fn from(value: String) -> Self {
        if value.contains(Self::INVALID) {
            value.as_str().into()
        } else {
            Self(value)
        }
    }
}

impl From<&str> for Detector {
    fn from(value: &str) -> Self {
        Self(value.replace(Self::INVALID, "_"))
    }
}

impl AsRef<str> for Detector {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

// Derived Default is OK without validation as empty path is a valid subdirectory
#[derive(Debug, Default)]
pub struct Subdirectory(PathBuf);

#[derive(Debug)]
pub struct EmptyUsername;

#[derive(Debug)]
pub enum InvalidSubdirectory {
    InvalidComponent(usize),
    AbsolutePath,
}

impl Subdirectory {
    pub fn new(sub: impl Into<PathBuf>) -> Result<Self, InvalidSubdirectory> {
        let sub = sub.into();
        let mut new_sub = PathBuf::new();
        for (i, comp) in sub.components().enumerate() {
            let err = match comp {
                Component::CurDir => continue,
                Component::Normal(seg) => {
                    new_sub.push(seg);
                    continue;
                }
                Component::RootDir => InvalidSubdirectory::AbsolutePath,
                Component::Prefix(_) | Component::ParentDir => {
                    InvalidSubdirectory::InvalidComponent(i)
                }
            };
            return Err(err);
        }
        Ok(Self(new_sub))
    }
}

impl Display for Subdirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display().fmt(f)
    }
}

impl AsRef<Path> for Subdirectory {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl BeamlineContext {
    pub fn new(instrument: impl Into<String>, visit: String) -> Self {
        Self {
            instrument: instrument.into(),
            visit,
        }
    }
    pub fn instrument(&self) -> &str {
        &self.instrument
    }
    pub fn visit(&self) -> &str {
        &self.visit
    }
    pub fn for_scan(&self, scan_number: usize) -> ScanContext<'_> {
        ScanContext {
            subdirectory: Subdirectory::default(),
            scan_number,
            beamline: self,
        }
    }
    pub fn next_scan(&self) -> ScanContext<'_> {
        ScanContext {
            subdirectory: Subdirectory::default(),
            // TODO: source numtracker from somewhere?
            scan_number: numtracker::GdaNumTracker::new("/tmp")
                .increment_and_get(self)
                // TODO: Handle errors
                .unwrap(),
            beamline: self,
        }
    }
}

impl ScanContext<'_> {
    pub fn with_subdirectory(self, sub: Subdirectory) -> Self {
        Self {
            subdirectory: sub,
            ..self
        }
    }

    fn for_detector(&self, det: &str) -> DetectorContext {
        DetectorContext {
            scan: self,
            detector: det.into(),
        }
    }
}

#[cfg(test)]
mod detector_tests {
    use super::Detector;

    #[test]
    fn valid() {
        assert_eq!("valid_detector", Detector::from("valid_detector").as_ref());
    }

    #[test]
    fn invalid() {
        assert_eq!(
            Detector::from("spaced detector").as_ref(),
            "spaced_detector",
        );
        assert_eq!(Detector::from("..").as_ref(), "__");
        assert_eq!(Detector::from("foo.bar").as_ref(), "foo_bar");
        assert_eq!(Detector::from("foo/bar").as_ref(), "foo_bar");
    }
}
