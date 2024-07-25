use std::fmt::Write;
use std::path::PathBuf;

use chrono::{Datelike, Local};

use crate::template::{FieldSource, Template};
use crate::BeamlineContext;

pub trait PathConstructor {
    type Err;
    /// Get the root data directory for the given visit
    fn visit_directory(&self, visit: &BeamlineContext) -> Result<PathBuf, Self::Err>;
}

pub struct TemplatePathConstructor {
    template: Template<BeamlineField>,
}

#[derive(Debug, PartialEq, Eq)]
enum BeamlineField {
    Year,
    Visit,
    Proposal,
    Instrument,
    Custom(String),
}

impl From<String> for BeamlineField {
    fn from(value: String) -> Self {
        match value.as_str() {
            "year" => BeamlineField::Year,
            "visit" => BeamlineField::Visit,
            "proposal" => BeamlineField::Proposal,
            "instrument" => BeamlineField::Instrument,
            _ => BeamlineField::Custom(value),
        }
    }
}

impl FieldSource<BeamlineField> for &BeamlineContext {
    type Err = ();

    fn write_to(&self, buf: &mut String, field: &BeamlineField) -> Result<(), Self::Err> {
        _ = match field {
            // Should be year of visit?
            BeamlineField::Year => buf.write_fmt(format_args!("{}", Local::now().year())),
            BeamlineField::Visit => todo!(),
            BeamlineField::Proposal => todo!(),
            BeamlineField::Instrument => buf.write_str(self.instrument.as_ref()),
            BeamlineField::Custom(_) => todo!(),
        };
        Ok(())
    }
}

impl TemplatePathConstructor {
    pub fn new(template: impl AsRef<str>) -> Result<Self, crate::template::ParseError> {
        Ok(Self {
            template: Template::new(template)?,
        })
    }
}

impl PathConstructor for TemplatePathConstructor {
    type Err = ();

    fn visit_directory(&self, visit: &BeamlineContext) -> Result<PathBuf, Self::Err> {
        Ok(PathBuf::from(&self.template.render(visit)?))
    }
}
