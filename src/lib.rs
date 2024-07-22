use std::path::PathBuf;

pub mod gda;

pub enum Code {
    Cm,
    Sm,
    Other(String),
}

#[allow(unused)]
pub struct Proposal(usize);
#[allow(unused)]
pub struct Session(usize);

#[allow(unused)]
pub struct Visit(Code, Proposal, Session);

#[allow(unused)]
pub struct User(String);

pub trait NumTracker {
    type Err;
    /// Get the next value from this tracker - every call should result in a new number
    ///
    /// If a call fails, the next successful call may or may not reflect that there were
    /// unsuccessful attempts since the last value returned.
    fn increment_and_get(&mut self) -> Result<usize, Self::Err>;

    // GDA NumTracker interface
    // * getCurrentFileNumber -> int
    // * incrementNumber -> int
    // * setFileNumber(long)
    // * resetFileNumber
}

pub trait PathConstructor {
    type Err;
    /// Get the root data directory for the given visit
    fn visit_directory(&self, visit: &Visit) -> Result<PathBuf, Self::Err>;
    /// The path to the scan file for the given visit, scan number and optionally a
    /// detector/process name.
    fn scan_file(&self, visit: &Visit, number: usize) -> Result<PathBuf, Self::Err>;

    // GDA PathConstructor interface
    // * createFromDefaultProperty -> String
    // * createFromRCPProperties -> String (deprecated)
    // * createFromProperty(String) -> String
    // * createFromProperty(String, Map<String, String>) -> String
    // * createFromTemplate(String) -> String
    // * createFromTemplate(String, Map<String, String>) -> String
    // * getVisitDirectory -> String
    // * getVisitSubdirectory -> String
    // * getClientVisitDirectory -> String
    // * getClientVisitSubdirectory -> String
    // * getDefaultDataDir -> String
    // * getFromTemplate(String) -> String
}
