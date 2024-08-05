use std::fmt::Write;
use std::path::PathBuf;

use chrono::{Datelike, Local};

use crate::template::{FieldSource, PathTemplate, PathTemplateError, TemplateError};
use crate::{BeamlineContext, DetectorContext, ScanContext};

pub trait PathConstructor {
    type Err;
    /// Get the root data directory for the given visit
    fn visit_directory(&self, ctx: &BeamlineContext) -> Result<PathBuf, Self::Err>;
    /// Get the file path for a given scan
    fn scan_file(&self, ctx: &ScanContext) -> Result<PathBuf, Self::Err>;
    /// Get the path for the file of a specific detector
    fn detector_file(&self, ctx: &DetectorContext) -> Result<PathBuf, Self::Err>;
}

pub struct TemplatePathConstructor {
    visit_directory: PathTemplate<BeamlineField>,
    scan_file: PathTemplate<ScanField>,
    detector_file: PathTemplate<DetectorField>,
}

pub struct VisitPathTemplate(PathTemplate<BeamlineField>);
pub struct ScanPathTemplate(PathTemplate<ScanField>);
pub struct DetectorPathTemplate(PathTemplate<DetectorField>);

pub fn visit_path(template: &str) -> Result<VisitPathTemplate, PathTemplateError<InvalidKey>> {
    Ok(VisitPathTemplate(PathTemplate::new(template)?))
}

pub fn scan_path(template: &str) -> Result<ScanPathTemplate, PathTemplateError<InvalidKey>> {
    Ok(ScanPathTemplate(PathTemplate::new(template)?))
}

pub fn detector_path(
    template: &str,
) -> Result<DetectorPathTemplate, PathTemplateError<InvalidKey>> {
    Ok(DetectorPathTemplate(PathTemplate::new(template)?))
}

impl VisitPathTemplate {
    pub fn render(&self, ctx: &BeamlineContext) -> PathBuf {
        self.0.render(ctx).unwrap()
    }
}
impl ScanPathTemplate {
    pub fn render(&self, ctx: &ScanContext) -> PathBuf {
        self.0.render(ctx).unwrap()
    }
}
impl DetectorPathTemplate {
    pub fn render(&self, ctx: &DetectorContext) -> PathBuf {
        self.0.render(ctx).unwrap()
    }
}

#[derive(Debug, PartialEq, Eq)]
enum BeamlineField {
    Year,
    Visit,
    Proposal,
    Instrument,
    Custom(String),
}

#[derive(Debug)]
enum ScanField {
    Subdirectory,
    ScanNumber,
    Beamline(BeamlineField),
}

#[derive(Debug)]
enum DetectorField {
    Detector,
    Scan(ScanField),
}

#[derive(Debug)]
pub struct InvalidKey(String);

impl TryFrom<String> for BeamlineField {
    type Error = InvalidKey;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "year" => Ok(BeamlineField::Year),
            "visit" => Ok(BeamlineField::Visit),
            "proposal" => Ok(BeamlineField::Proposal),
            "instrument" => Ok(BeamlineField::Instrument),
            _ => Ok(BeamlineField::Custom(value)),
        }
    }
}

impl TryFrom<String> for ScanField {
    type Error = InvalidKey;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "scan_number" => Ok(ScanField::ScanNumber),
            "subdirectory" => Ok(ScanField::Subdirectory),
            _ => Ok(ScanField::Beamline(BeamlineField::try_from(value)?)),
        }
    }
}

impl TryFrom<String> for DetectorField {
    type Error = InvalidKey;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "detector" => Ok(DetectorField::Detector),
            _ => Ok(DetectorField::Scan(ScanField::try_from(value)?)),
        }
    }
}

impl FieldSource<BeamlineField> for &BeamlineContext {
    type Err = InvalidKey;

    fn write_to(&self, buf: &mut String, field: &BeamlineField) -> Result<(), Self::Err> {
        _ = match field {
            // Should be year of visit?
            BeamlineField::Year => buf.write_fmt(format_args!("{}", Local::now().year())),
            BeamlineField::Visit => write!(buf, "{}", self.visit()),
            BeamlineField::Proposal => write!(buf, "{}", self.visit.proposal),
            BeamlineField::Instrument => buf.write_str(self.instrument.as_ref()),
            BeamlineField::Custom(key) => return Err(InvalidKey(key.clone())),
        };
        Ok(())
    }
}

impl<'a> FieldSource<ScanField> for &ScanContext<'a> {
    type Err = InvalidKey;

    fn write_to(&self, buf: &mut String, field: &ScanField) -> Result<(), Self::Err> {
        _ = match field {
            ScanField::Subdirectory => write!(buf, "{}", self.subdirectory),
            ScanField::ScanNumber => write!(buf, "{}", self.scan_number),
            ScanField::Beamline(bf) => Ok(self.beamline.write_to(buf, bf)?),
        };
        Ok(())
    }
}

impl<'a> FieldSource<DetectorField> for &DetectorContext<'a> {
    type Err = InvalidKey;

    fn write_to(&self, buf: &mut String, field: &DetectorField) -> Result<(), Self::Err> {
        _ = match field {
            DetectorField::Detector => buf.write_str(self.detector.as_ref()),
            DetectorField::Scan(sf) => Ok(self.scan.write_to(buf, sf)?),
        };
        Ok(())
    }
}

impl TemplatePathConstructor {
    pub fn new(template: impl AsRef<str>) -> Result<Self, PathTemplateError<InvalidKey>> {
        Ok(Self {
            visit_directory: PathTemplate::new(template)?,
            scan_file: PathTemplate::new("{subdirectory}/{instrument}-{scan_number}")?,
            detector_file: PathTemplate::new(
                "{subdirectory}/{instrument}-{scan_number}-{detector}",
            )?,
        })
    }
}

impl PathConstructor for TemplatePathConstructor {
    type Err = InvalidKey;

    fn visit_directory(&self, ctx: &BeamlineContext) -> Result<PathBuf, Self::Err> {
        Ok(PathBuf::from(&self.visit_directory.render(ctx)?))
    }

    fn scan_file(&self, ctx: &ScanContext) -> Result<PathBuf, Self::Err> {
        Ok(self
            .visit_directory(ctx.beamline)?
            .join(self.scan_file.render(ctx)?))
    }

    fn detector_file(&self, ctx: &DetectorContext) -> Result<PathBuf, Self::Err> {
        Ok(self
            .visit_directory(ctx.scan.beamline)?
            .join(self.detector_file.render(ctx)?))
    }
}
