use std::fmt::Write;
use std::path::PathBuf;

use chrono::{Datelike, Local};

use crate::template::{FieldSource, Template, TemplateError};
use crate::BeamlineContext;

pub trait PathConstructor {
    type Err;
    /// Get the root data directory for the given visit
    fn visit_directory(&self, visit: &BeamlineContext) -> Result<PathBuf, Self::Err>;
}

pub struct TemplatePathConstructor {
    visit_directory: Template<BeamlineField>,
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

impl FieldSource<BeamlineField> for &BeamlineContext {
    type Err = InvalidKey;

    fn write_to(&self, buf: &mut String, field: &BeamlineField) -> Result<(), Self::Err> {
        _ = match field {
            // Should be year of visit?
            BeamlineField::Year => buf.write_fmt(format_args!("{}", Local::now().year())),
            BeamlineField::Visit => write!(buf, "{}", self.visit()),
            BeamlineField::Proposal => write!(buf, "{}", self.visit.proposal),
            BeamlineField::Instrument => buf.write_str(self.instrument.as_ref()),
            BeamlineField::Custom(_) => todo!(),
        };
        Ok(())
    }
}

impl TemplatePathConstructor {
    pub fn new(template: impl AsRef<str>) -> Result<Self, TemplateError<InvalidKey>> {
        Ok(Self {
            visit_directory: Template::new(template)?,
        })
    }
}

impl PathConstructor for TemplatePathConstructor {
    type Err = InvalidKey;

    fn visit_directory(&self, ctx: &BeamlineContext) -> Result<PathBuf, Self::Err> {
        Ok(PathBuf::from(&self.visit_directory.render(ctx)?).join(&ctx.subdirectory))
    }
}
