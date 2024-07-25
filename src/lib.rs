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
impl Visit {
    fn display(&self) -> String {
        format!(
            "{}{}-{}",
            self.proposal.code, self.proposal.number, self.session
        )
    }

    fn proposal(&self) -> String {
        format!("{}{}", self.proposal.code, self.proposal.number)
    }
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
