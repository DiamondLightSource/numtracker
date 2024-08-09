use std::borrow::Cow;
use std::error::Error;
use std::fmt::Display;
use std::path::PathBuf;

use chrono::{Datelike, Local};

use crate::template::{FieldSource, PathTemplate, PathTemplateError};
use crate::{BeamlineContext, DetectorContext, ScanContext};

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

impl Display for InvalidKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Unrecognised key: {}", self.0)
    }
}

impl Error for InvalidKey {}

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

impl<'bl> FieldSource<BeamlineField> for BeamlineContext<'bl> {
    type Err = InvalidKey;

    fn resolve(&self, field: &BeamlineField) -> Result<Cow<'_, str>, Self::Err> {
        Ok(match field {
            // Should be year of visit?
            BeamlineField::Year => Local::now().year().to_string().into(),
            BeamlineField::Visit => self.visit().into(),
            BeamlineField::Proposal => self
                .visit
                .split('-')
                .next()
                .expect("There is always one section for a split")
                .into(),
            BeamlineField::Instrument => AsRef::<str>::as_ref(&self.instrument).into(),
            BeamlineField::Custom(key) => return Err(InvalidKey(key.clone())),
        })
    }
}

impl<'bl> FieldSource<ScanField> for ScanContext<'bl> {
    type Err = InvalidKey;

    fn resolve(&self, field: &ScanField) -> Result<Cow<'_, str>, Self::Err> {
        Ok(match field {
            ScanField::Subdirectory => self.subdirectory.as_ref().to_string_lossy(),
            ScanField::ScanNumber => self.scan_number.to_string().into(),
            ScanField::Beamline(bf) => self.beamline.resolve(bf)?,
        })
    }
}

impl<'a> FieldSource<DetectorField> for DetectorContext<'a> {
    type Err = InvalidKey;

    fn resolve(&self, field: &DetectorField) -> Result<Cow<'_, str>, Self::Err> {
        Ok(match field {
            DetectorField::Detector => self.detector.as_ref().into(),
            DetectorField::Scan(sf) => self.scan.resolve(sf)?,
        })
    }
}
