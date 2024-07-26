use std::fmt::Display;
use std::str::FromStr;

pub mod numtracker;
pub mod paths;
pub(crate) mod template;

#[derive(Debug, PartialEq, Eq)]
pub struct Proposal {
    pub code: String,
    pub number: usize,
}

#[derive(Debug, PartialEq, Eq)]
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
    visit: Visit,
    user: User,
}

#[derive(Debug, PartialEq, Eq)]
pub enum InvalidVisit {
    NonAsciiCode,
    InvalidFormat,
    InvalidSession,
    InvalidProposal,
}

#[derive(Debug)]
pub struct EmptyUsername;

impl Visit {
    pub fn new<C: Into<String>>(
        code: C,
        proposal: usize,
        session: usize,
    ) -> Result<Self, InvalidVisit> {
        let code = code.into();
        if !code.is_empty() && code.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self {
                proposal: Proposal {
                    code,
                    number: proposal,
                },
                session,
            })
        } else {
            Err(InvalidVisit::NonAsciiCode)
        }
    }
}

impl FromStr for Visit {
    type Err = InvalidVisit;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((proposal, session)) = value.split_once('-') else {
            return Err(InvalidVisit::InvalidFormat);
        };
        let session = session.parse().map_err(|_| InvalidVisit::InvalidSession)?;
        let Some(split) = proposal.find(|c: char| !c.is_alphabetic()) else {
            return Err(InvalidVisit::InvalidFormat);
        };
        let (code, proposal) = proposal.split_at(split);
        let proposal = proposal
            .parse()
            .map_err(|_| InvalidVisit::InvalidProposal)?;
        Self::new(code, proposal, session)
    }
}

impl User {
    pub fn new(user: impl Into<String>) -> Result<Self, EmptyUsername> {
        let user = user.into();
        if user.is_empty() {
            Err(EmptyUsername)
        } else {
            Ok(Self(user))
        }
    }
}

impl BeamlineContext {
    pub fn new(instrument: impl Into<String>, visit: Visit, user: User) -> Self {
        Self {
            instrument: Instrument(instrument.into()),
            visit,
            user,
        }
    }
    pub fn instrument(&self) -> &Instrument {
        &self.instrument
    }
    pub fn visit(&self) -> &Visit {
        &self.visit
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

#[cfg(test)]
mod visit_tests {
    use crate::{InvalidVisit, Visit};

    #[test]
    fn visit_from_valid_str() {
        assert_eq!(Visit::new("cm", 12345, 3), "cm12345-3".parse())
    }
    #[test]
    fn missing_code() {
        assert_eq!(
            "123-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::NonAsciiCode
        )
    }
    #[test]
    fn missing_session() {
        assert_eq!(
            "cm12345".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidFormat
        )
    }
    #[test]
    fn missing_proposal() {
        assert_eq!(
            "cm-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidFormat
        );
    }
    #[test]
    fn invalid_proposal() {
        assert_eq!(
            "cm12fede-3".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidProposal
        )
    }
    #[test]
    fn invalid_session() {
        assert_eq!(
            "cm12345-abc".parse::<Visit>().unwrap_err(),
            InvalidVisit::InvalidSession
        )
    }
}
