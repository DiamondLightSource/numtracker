use std::error::Error;
use std::fmt::Display;
use std::future::Future;
use std::path::{Component, Path, PathBuf};

use paths::{DetectorTemplate, ScanTemplate, VisitTemplate};

pub mod db_service;
pub mod fallback;
pub mod numtracker;
pub mod paths;
pub(crate) mod template;

pub trait ScanNumberBackend: Clone + Sync + Send {
    type NumberError: Error + Send + Sync;
    fn next_scan_number(
        &self,
        beamline: &str,
    ) -> impl Future<Output = Result<usize, Self::NumberError>> + Send;
}
pub trait PathTemplateBackend: Clone + Sync + Send {
    type TemplateErr: Error + Send + Sync;
    fn visit_directory_template(
        &self,
        beamline: &str,
    ) -> impl Future<Output = Result<VisitTemplate, Self::TemplateErr>> + Send;
    fn scan_file_template(
        &self,
        beamline: &str,
    ) -> impl Future<Output = Result<ScanTemplate, Self::TemplateErr>> + Send;
    fn detector_file_template(
        &self,
        bl: &str,
    ) -> impl Future<Output = Result<DetectorTemplate, Self::TemplateErr>> + Send;
}

pub struct VisitService<Backend> {
    db: Backend,
    ctx: BeamlineContext,
}

pub struct ScanService<Backend> {
    db: Backend,
    ctx: ScanContext,
}

#[derive(Clone)]
pub struct BeamlineContext {
    instrument: String,
    visit: String,
}

pub struct ScanContext {
    subdirectory: Subdirectory,
    scan_number: usize,
    beamline: BeamlineContext,
}

pub struct DetectorContext<'bl> {
    detector: Detector,
    scan: &'bl ScanContext,
}

impl<'bl, Backend> VisitService<Backend> {
    pub fn new(backend: Backend, ctx: BeamlineContext) -> Self {
        Self { db: backend, ctx }
    }
    pub fn beamline(&self) -> &str {
        &self.ctx.instrument
    }
    pub fn visit(&self) -> &str {
        &self.ctx.visit
    }
}

impl<'bl, Backend> VisitService<Backend>
where
    Backend: ScanNumberBackend,
{
    pub async fn new_scan(
        &self,
        subdirectory: Subdirectory,
    ) -> Result<ScanService<Backend>, Backend::NumberError> {
        let number = self.db.next_scan_number(&self.ctx.instrument).await?;
        Ok(ScanService {
            db: self.db.clone(),
            ctx: self.ctx.for_scan(number, subdirectory),
        })
    }
}

impl<'bl, Backend> VisitService<Backend>
where
    Backend: PathTemplateBackend,
{
    pub async fn visit_directory(&self) -> Result<PathBuf, Backend::TemplateErr> {
        Ok(self
            .db
            .visit_directory_template(&self.ctx.instrument)
            .await?
            .render(&self.ctx))
    }
}

impl<Backend> ScanService<Backend>
where
    Backend: PathTemplateBackend,
{
    pub fn scan_number(&self) -> usize {
        self.ctx.scan_number
    }
    pub fn beamline(&self) -> &str {
        &self.ctx.beamline.instrument
    }
    pub fn visit(&self) -> &str {
        &self.ctx.beamline.visit
    }

    pub async fn visit_directory(&self) -> Result<PathBuf, Backend::TemplateErr> {
        Ok(self
            .db
            .visit_directory_template(&self.beamline())
            .await?
            .render(&self.ctx.beamline))
    }

    pub async fn scan_file(&self) -> Result<PathBuf, Backend::TemplateErr> {
        Ok(self
            .db
            .scan_file_template(&self.ctx.beamline.instrument)
            .await?
            .render(&self.ctx))
    }

    pub async fn detector_files<'det>(
        &self,
        detectors: &'det [String],
    ) -> Result<Vec<(&'det String, PathBuf)>, Backend::TemplateErr> {
        if detectors.is_empty() {
            return Ok(vec![]);
        }
        let template = self
            .db
            .detector_file_template(&self.ctx.beamline.instrument)
            .await?;
        Ok(detectors
            .iter()
            .map(|det| {
                let path = template.render(&self.ctx.for_detector(det));
                (det, path)
            })
            .collect())
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

#[derive(Debug)]
pub struct EmptyUsername;

#[derive(Debug)]
pub enum InvalidSubdirectory {
    InvalidComponent(usize),
    AbsolutePath,
}

impl Display for InvalidSubdirectory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidSubdirectory::InvalidComponent(s) => {
                write!(f, "Segment {s} of path is not valid for a subdirectory")
            }
            InvalidSubdirectory::AbsolutePath => f.write_str("Subdirectory cannot be absolute"),
        }
    }
}

impl Error for InvalidSubdirectory {}

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
    pub fn new(instrument: String, visit: String) -> Self {
        Self { instrument, visit }
    }
    pub fn instrument(&self) -> &str {
        &self.instrument
    }
    pub fn visit(&self) -> &str {
        &self.visit
    }
    pub fn for_scan(&self, scan_number: usize, subdirectory: Subdirectory) -> ScanContext {
        ScanContext {
            subdirectory,
            scan_number,
            beamline: self.clone(),
        }
    }
}

impl ScanContext {
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
