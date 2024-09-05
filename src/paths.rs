use std::error::Error;
use std::fmt::{self, Debug, Display};

#[derive(Debug, PartialEq, Eq)]
pub enum BeamlineField {
    Year,
    Visit,
    Proposal,
    Instrument,
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

impl Display for BeamlineField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BeamlineField::Year => f.write_str("year"),
            BeamlineField::Visit => f.write_str("visit"),
            BeamlineField::Proposal => f.write_str("proposal"),
            BeamlineField::Instrument => f.write_str("instrument"),
        }
    }
}

impl Display for ScanField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScanField::Subdirectory => f.write_str("subdirectory"),
            ScanField::ScanNumber => f.write_str("scan_number"),
            ScanField::Beamline(bl) => write!(f, "{bl}"),
        }
    }
}

impl Display for DetectorField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DetectorField::Detector => f.write_str("detector"),
            DetectorField::Scan(sc) => write!(f, "{sc}"),
        }
    }
}

#[derive(Debug)]
pub struct InvalidKey(String);

impl Display for InvalidKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            _ => Err(InvalidKey(value)),
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
