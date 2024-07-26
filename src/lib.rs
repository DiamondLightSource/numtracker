use std::fmt::Display;

pub mod numtracker;
pub mod paths;
pub(crate) mod template;

#[derive(Debug)]
pub struct Proposal {
    pub code: String,
    pub number: usize,
}

#[derive(Debug)]
pub struct Visit {
    pub proposal: Proposal,
    pub session: usize,
}

#[derive(Debug)]
pub struct Instrument(String);
impl AsRef<str> for Instrument {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub struct User(String);

pub struct BeamlineContext {
    instrument: Instrument,
}

impl BeamlineContext {
    pub fn new(instrument: impl Into<String>) -> Self {
        Self {
            instrument: Instrument(instrument.into()),
        }
    }
    pub fn instrument(&self) -> &Instrument {
        &self.instrument
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
