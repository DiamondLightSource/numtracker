pub mod numtracker;
pub mod paths;
pub(crate) mod template;

#[derive(Debug)]
pub struct Code(pub String);
#[derive(Debug)]
pub struct Proposal(pub usize);
#[derive(Debug)]
pub struct Session(pub usize);

#[derive(Debug)]
pub struct Visit(pub Code, pub Proposal, pub Session);
impl Visit {
    fn display(&self) -> String {
        format!("{}{}-{}", self.0 .0, self.1 .0, self.2 .0)
    }

    fn proposal(&self) -> String {
        format!("{}{}", self.0 .0, self.1 .0)
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
