// Copyright 2024 Diamond Light Source
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::collections::HashSet;
use std::fmt::{Debug, Display};
use std::hash::Hash;

use derive_more::{Display, Error, From};

use crate::template::{PathTemplate, PathTemplateError};

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectoryField {
    #[display("year")]
    Year,
    #[display("visit")]
    Visit,
    #[display("proposal")]
    Proposal,
    #[display("instrument")]
    Instrument,
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanField {
    #[display("subdirectory")]
    Subdirectory,
    #[display("scan_number")]
    ScanNumber,
    #[display("{_0}")]
    Directory(DirectoryField),
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DetectorField {
    #[display("detector")]
    Detector,
    #[display("{_0}")]
    Scan(ScanField),
}

#[derive(Debug, Display, Error)]
#[display("Unrecognised key: {_0:?}")]
pub struct InvalidKey(#[error(ignore)] String);

impl TryFrom<String> for DirectoryField {
    type Error = InvalidKey;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "year" => Ok(DirectoryField::Year),
            "visit" => Ok(DirectoryField::Visit),
            "proposal" => Ok(DirectoryField::Proposal),
            "instrument" => Ok(DirectoryField::Instrument),
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
            _ => Ok(ScanField::Directory(DirectoryField::try_from(value)?)),
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

pub trait PathSpec {
    type Field: TryFrom<String> + Eq + Hash + Display + 'static;
    const REQUIRED: &'static [Self::Field];
    const ABSOLUTE: bool;

    fn new_checked(path: &str) -> Result<PathTemplate<Self::Field>, InvalidPathTemplate> {
        let template = PathTemplate::new(path)?;
        match (Self::ABSOLUTE, template.is_absolute()) {
            (true, false) => Err(InvalidPathTemplate::ShouldBeAbsolute),
            (false, true) => Err(InvalidPathTemplate::ShouldBeRelative),
            _ => Ok(()),
        }?;
        let fields = template.referenced_fields().collect::<HashSet<_>>();
        for f in Self::REQUIRED {
            if !fields.contains(f) {
                return Err(InvalidPathTemplate::MissingField(f.to_string()));
            }
        }
        Ok(template)
    }

    fn describe() -> &'static str;
}

#[derive(Debug, Display, Error, From, PartialEq)]
pub enum InvalidPathTemplate {
    #[display("{_0}")]
    #[from]
    TemplateError(PathTemplateError),
    #[display("Path should be absolute")]
    ShouldBeAbsolute,
    #[display("Path should be relative")]
    ShouldBeRelative,
    #[display("Template should reference missing field: {_0:?}")]
    MissingField(#[error(ignore)] String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryTemplate;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanTemplate;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectorTemplate;

impl PathSpec for DirectoryTemplate {
    type Field = DirectoryField;

    const REQUIRED: &'static [Self::Field] = &[];

    const ABSOLUTE: bool = true;
    fn describe() -> &'static str {
        concat!(
            "A template describing the path to the data directory for a given instrument session. ",
            "It should be an absolute path and contain placeholders for {instrument} and {visit}."
        )
    }
}

impl PathSpec for ScanTemplate {
    type Field = ScanField;

    const REQUIRED: &'static [Self::Field] = &[ScanField::ScanNumber];

    const ABSOLUTE: bool = false;
    fn describe() -> &'static str {
        concat!(
            "A template describing the location within a session data directory where the root scan file should be written. ",
            "It should be a relative path and contain a placeholder for {scan_number} to ensure files are unique."
        )
    }
}

impl PathSpec for DetectorTemplate {
    type Field = DetectorField;

    const REQUIRED: &'static [Self::Field] = &[
        DetectorField::Detector,
        DetectorField::Scan(ScanField::ScanNumber),
    ];

    const ABSOLUTE: bool = false;
    fn describe() -> &'static str {
        concat!(
            "A template describing the location within a session data directory where ",
            "the data for a given detector should be written",
            "\n\n",
            "It should contain placeholders for {detector} and {scan_number} ",
            "to ensure paths are unique between scans and for multiple ",
            "detectors."
        )
    }
}

#[cfg(test)]
mod paths_tests {
    use std::fmt::Debug;

    use super::{
        DetectorTemplate, DirectoryTemplate, InvalidPathTemplate, PathSpec as _, ScanTemplate,
    };
    use crate::template::{ErrorKind, PathTemplateError};

    #[derive(Debug)]
    enum TemplateErrorType {
        Incomplete,
        Nested,
        Empty,
        Unrecognised,
    }

    impl PartialEq<InvalidPathTemplate> for TemplateErrorType {
        fn eq(&self, other: &InvalidPathTemplate) -> bool {
            match other {
                InvalidPathTemplate::TemplateError(PathTemplateError::TemplateError(e)) => {
                    matches!(
                        (self, e.kind()),
                        (Self::Incomplete, ErrorKind::Incomplete)
                            | (Self::Nested, ErrorKind::Nested)
                            | (Self::Unrecognised, ErrorKind::Unrecognised)
                            | (Self::Empty, ErrorKind::Empty)
                    )
                }
                _ => false,
            }
        }
    }

    #[rstest::rstest]
    #[case::relative("relative/visit/path", InvalidPathTemplate::ShouldBeAbsolute)]
    #[case::invalid_path_incomplete("/data/{unclosed", TemplateErrorType::Incomplete)]
    #[case::invalid_path_empty("/data/{}", TemplateErrorType::Empty)]
    #[case::invalid_path_nested("/data/{nes{ted}}", TemplateErrorType::Nested)]
    #[case::invalid_path_unrecognised("/data/{scan_number}", TemplateErrorType::Unrecognised)]
    fn invalid_directory<E: PartialEq<InvalidPathTemplate> + Debug>(
        #[case] template: &str,
        #[case] err: E,
    ) {
        let e = DirectoryTemplate::new_checked(template).unwrap_err();
        assert_eq!(err, e);
    }

    #[rstest::rstest]
    #[case::absolute("/absolute/scan/path", InvalidPathTemplate::ShouldBeRelative)]
    #[case::missing_scan_number("no_scan_number", InvalidPathTemplate::MissingField("scan_number".into()))]
    #[case::invalid_path_incomplete("data/{unclosed", TemplateErrorType::Incomplete)]
    #[case::invalid_path_empty("data/{}", TemplateErrorType::Empty)]
    #[case::invalid_path_nested("data/{nes{ted}}", TemplateErrorType::Nested)]
    #[case::invalid_path_unrecognised("data/{detector}", TemplateErrorType::Unrecognised)]
    fn invalid_scan<E: PartialEq<InvalidPathTemplate> + Debug>(
        #[case] template: &str,
        #[case] err: E,
    ) {
        let e = ScanTemplate::new_checked(template).unwrap_err();
        assert_eq!(err, e);
    }

    #[rstest::rstest]
    #[case::absolute("/absolute/detector/path", InvalidPathTemplate::ShouldBeRelative)]
    #[case::missing_detector("{scan_number}", InvalidPathTemplate::MissingField("detector".into()))]
    #[case::missing_scan_number("{detector}", InvalidPathTemplate::MissingField("scan_number".into()))]
    #[case::invalid_path_incomplete("data/{unclosed", TemplateErrorType::Incomplete)]
    #[case::invalid_path_empty("data/{}", TemplateErrorType::Empty)]
    #[case::invalid_path_nested("data/{nes{ted}}", TemplateErrorType::Nested)]
    #[case::invalid_path_unrecognised("data/{unknown}", TemplateErrorType::Unrecognised)]
    fn invalid_detector<E: PartialEq<InvalidPathTemplate> + Debug>(
        #[case] template: &str,
        #[case] err: E,
    ) {
        let e = DetectorTemplate::new_checked(template).unwrap_err();
        assert_eq!(err, e);
    }
}
