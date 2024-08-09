use std::borrow::Cow;
use std::error::Error;
use std::fmt::Display;

use chrono::{Datelike, Local};

use crate::template::FieldSource;
use crate::{BeamlineContext, DetectorContext, ScanContext};

#[derive(Debug, PartialEq, Eq)]
pub enum BeamlineField {
    Year,
    Visit,
    Proposal,
    Instrument,
    Custom(String),
}

#[derive(Debug)]
pub enum ScanField {
    Subdirectory,
    ScanNumber,
    Beamline(BeamlineField),
}

#[derive(Debug)]
pub enum DetectorField {
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
