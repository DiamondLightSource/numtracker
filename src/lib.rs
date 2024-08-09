use std::fmt::Display;
use std::path::{Component, Path, PathBuf};
use std::str::FromStr;

use numtracker::NumTracker;

pub mod db_service;
pub mod numtracker;
pub mod paths;
pub(crate) mod template;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Proposal {
    code: String,
    number: usize,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Visit {
    proposal: Proposal,
    session: usize,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Instrument(String);
impl AsRef<str> for Instrument {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl TryFrom<&str> for Instrument {
    type Error = InvalidInstrument;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value.is_empty() || value.contains(char::is_whitespace) {
            return Err(InvalidInstrument);
        }
        Ok(Self(value.into()))
    }
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

pub struct BeamlineContext {
    instrument: Instrument,
    visit: Visit,
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

#[derive(Debug, PartialEq, Eq)]
pub enum InvalidVisit {
    NonAsciiCode,
    InvalidFormat,
    InvalidSession,
    InvalidProposal,
}

#[derive(Debug)]
pub struct EmptyUsername;

#[derive(Debug)]
pub enum InvalidSubdirectory {
    InvalidComponent(usize),
    AbsolutePath,
}

#[derive(Debug)]
pub struct InvalidInstrument;

impl Visit {
    pub fn new<C: Into<String>>(
        code: C,
        proposal: usize,
        session: usize,
    ) -> Result<Self, InvalidVisit> {
        let code = code.into();
        if !code.is_empty() && code.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self {
                proposal: Proposal {
                    code,
                    number: proposal,
                },
                session,
            })
        } else {
            Err(InvalidVisit::NonAsciiCode)
        }
    }
}

impl FromStr for Visit {
    type Err = InvalidVisit;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((proposal, session)) = value.split_once('-') else {
            return Err(InvalidVisit::InvalidFormat);
        };
        let session = session.parse().map_err(|_| InvalidVisit::InvalidSession)?;
        let Some(split) = proposal.find(|c: char| !c.is_alphabetic()) else {
            return Err(InvalidVisit::InvalidFormat);
        };
        let (code, proposal) = proposal.split_at(split);
        let proposal = proposal
            .parse()
            .map_err(|_| InvalidVisit::InvalidProposal)?;
        Self::new(code, proposal, session)
    }
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
    pub fn new(instrument: impl Into<String>, visit: Visit) -> Self {
        Self {
            instrument: Instrument(instrument.into()),
            visit,
        }
    }
    pub fn instrument(&self) -> &Instrument {
        &self.instrument
    }
    pub fn visit(&self) -> &Visit {
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

impl Display for Proposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.code, self.number)
    }
}

impl Display for Visit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.proposal, self.session)
    }
}

#[cfg(test)]
mod visit_tests {
    use crate::{InvalidVisit, Visit};

    #[test]
    fn visit_from_valid_str() {
        assert_eq!(Visit::new("cm", 12345, 3), "cm12345-3".parse())
    }
    #[test]
    fn missing_code() {
        assert_eq!(
            "123-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::NonAsciiCode
        )
    }
    #[test]
    fn missing_session() {
        assert_eq!(
            "cm12345".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidFormat
        )
    }
    #[test]
    fn missing_proposal() {
        assert_eq!(
            "cm-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidFormat
        );
    }
    #[test]
    fn invalid_proposal() {
        assert_eq!(
            "cm12fede-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidProposal
        )
    }
    #[test]
    fn invalid_session() {
        assert_eq!(
            "cm12345-abc".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidSession
        )
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
