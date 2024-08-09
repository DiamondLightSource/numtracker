use std::borrow::Cow;
use std::error::Error;
use std::fmt::Display;
use std::path::{Component, PathBuf};

pub trait FieldSource<F> {
    type Err;
    fn resolve(&self, field: &F) -> Result<Cow<'_, str>, Self::Err>;
}

#[derive(Debug, PartialEq, Eq)]
enum Part<Field> {
    Literal(String),
    Field(Field),
}
#[derive(Debug)]
pub struct Template<Field> {
    parts: Vec<Part<Field>>,
}

#[derive(Debug)]
pub struct PathTemplate<Field> {
    parts: Vec<Template<Field>>,
    kind: PathType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathType {
    Absolute,
    Relative,
}
impl PathType {
    fn init(&self) -> PathBuf {
        match self {
            PathType::Absolute => PathBuf::from("/"),
            PathType::Relative => PathBuf::new(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PathTemplateError<F> {
    InvalidPath,
    TemplateError(TemplateError<F>),
}

impl<F: Display> Display for PathTemplateError<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathTemplateError::InvalidPath => f.write_str("Path is not valid"),
            PathTemplateError::TemplateError(e) => write!(f, "{e}"),
        }
    }
}

impl<F: Error + 'static> Error for PathTemplateError<F> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            PathTemplateError::InvalidPath => None,
            PathTemplateError::TemplateError(e) => Some(e),
        }
    }
}

#[derive(Debug)]
enum ParseState {
    /// We haven't started parsing anything yet
    Init,
    /// We are parsing a field key
    PartialKey(String),
    /// We are parsing a literal section of the template
    Literal(String),
    /// We are reading a literal section of the template but have encountered a (potentially
    /// escaped) opening brace.
    PendingLiteral(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct TemplateError<E> {
    position: usize,
    kind: ErrorKind<E>,
}

impl<F: Display> Display for TemplateError<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error parsing template: {} at {}",
            self.kind, self.position
        )
    }
}

impl<F: Error + 'static> Error for TemplateError<F> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            ErrorKind::Unrecognised(e) => Some(e),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ErrorKind<E> {
    Nested,
    Empty,
    Incomplete,
    Unrecognised(E),
}

impl<E: Display> Display for ErrorKind<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorKind::Nested => f.write_str("Nested placeholder"),
            ErrorKind::Empty => f.write_str("Empty placeholder"),
            ErrorKind::Incomplete => f.write_str("Unclosed placeholder"),
            ErrorKind::Unrecognised(e) => write!(f, "Invalid placeholder: {e}"),
        }
    }
}

impl<E> TemplateError<E> {
    fn new(position: usize, kind: ErrorKind<E>) -> Self {
        Self { position, kind }
    }
    fn nested(position: usize) -> Self {
        Self::new(position, ErrorKind::Nested)
    }
    fn incomplete(position: usize) -> Self {
        Self::new(position, ErrorKind::Incomplete)
    }
    fn empty(position: usize) -> Self {
        Self::new(position, ErrorKind::Empty)
    }
    fn unknown(position: usize, err: E) -> Self {
        Self::new(position, ErrorKind::Unrecognised(err))
    }
}

impl<E> From<TemplateError<E>> for PathTemplateError<E> {
    fn from(value: TemplateError<E>) -> Self {
        Self::TemplateError(value)
    }
}

impl<F: TryFrom<String>> Template<F> {
    pub fn new<S: AsRef<str>>(template: S) -> Result<Self, TemplateError<F::Error>> {
        let mut parts = vec![];
        let mut state = ParseState::Init;
        for (i, c) in template.as_ref().chars().enumerate() {
            match c {
                '{' => match state {
                    ParseState::Init => state = ParseState::PartialKey(String::new()),
                    ParseState::PartialKey(_) => return Err(TemplateError::nested(i)),
                    ParseState::Literal(val) => state = ParseState::PendingLiteral(val),
                    ParseState::PendingLiteral(val) => state = ParseState::Literal(val + "{"),
                },
                '}' => match state {
                    ParseState::Init => state = ParseState::Literal("}".into()),
                    ParseState::PartialKey(key) if key.trim().is_empty() => {
                        return Err(TemplateError::empty(i))
                    }
                    ParseState::PartialKey(key) => {
                        match F::try_from(key) {
                            Ok(field) => parts.push(Part::Field(field)),
                            Err(e) => return Err(TemplateError::unknown(i, e)),
                        }
                        // parts.push(Part::Field(F::try_from(key)));
                        state = ParseState::Init;
                    }
                    ParseState::PendingLiteral(_) => return Err(TemplateError::empty(i)),
                    ParseState::Literal(val) => state = ParseState::Literal(val + "}"),
                },
                c => match state {
                    ParseState::Init => state = ParseState::Literal(c.into()),
                    ParseState::PartialKey(mut key) => {
                        key.push(c);
                        state = ParseState::PartialKey(key);
                    }
                    ParseState::Literal(mut text) => {
                        text.push(c);
                        state = ParseState::Literal(text);
                    }
                    ParseState::PendingLiteral(text) => {
                        parts.push(Part::Literal(text));
                        state = ParseState::PartialKey(c.into());
                    }
                },
            }
        }
        match state {
            ParseState::Init => {}
            ParseState::PendingLiteral(_) | ParseState::PartialKey(_) => {
                return Err(TemplateError::incomplete(template.as_ref().len()))
            }
            ParseState::Literal(text) => parts.push(Part::Literal(text)),
        }
        Ok(Self { parts })
    }

    pub fn render<Src: FieldSource<F>>(&self, src: &Src) -> Result<String, Src::Err> {
        let mut buf = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => buf.push_str(text),
                Part::Field(f) => buf.push_str(&src.resolve(f)?),
            }
        }
        Ok(buf)
    }
}

impl<F: TryFrom<String>> PathTemplate<F> {
    pub fn new<S: AsRef<str>>(
        template: S,
    ) -> Result<Self, PathTemplateError<<F as TryFrom<String>>::Error>> {
        let path = PathBuf::from(template.as_ref());
        let mut parts = Vec::new();
        let mut kind = PathType::Relative;
        for comp in path.components() {
            match comp {
                Component::Normal(seg) => match seg.to_str() {
                    Some(seg) => parts.push(Template::new(seg)?),
                    None => return Err(PathTemplateError::InvalidPath),
                },
                Component::RootDir => kind = PathType::Absolute,
                Component::CurDir => continue,
                _ => return Err(PathTemplateError::InvalidPath),
            }
        }
        Ok(Self { parts, kind })
    }

    pub fn render<'a, Src, E>(&self, src: &'a Src) -> Result<PathBuf, E>
    where
        Src: FieldSource<F, Err = E>,
    {
        let mut path = self.kind.init();
        for part in &self.parts {
            path.push(part.render(src)?);
        }
        Ok(path)
    }

    pub fn is_absolute(&self) -> bool {
        self.kind == PathType::Absolute
    }
}

#[cfg(test)]
mod parser_tests {
    use super::Part::*;
    use super::*;

    type StrTemplate = Template<String>;

    fn literal(lit: &'static str) -> Part<String> {
        Part::Literal(lit.into())
    }
    fn field(f: &'static str) -> Part<String> {
        Part::Field(f.into())
    }

    #[test]
    fn only_literal() {
        let temp = Template::new("this is all literal").unwrap();
        assert_eq!(temp.parts, vec![literal("this is all literal")])
    }

    #[test]
    fn only_single_field() {
        let temp = StrTemplate::new("{year}").unwrap();
        assert_eq!(temp.parts, vec![Field("year".into())]);
    }

    #[test]
    fn only_fields() {
        let temp = StrTemplate::new("{year}{visit}{proposal}").unwrap();
        assert_eq!(
            temp.parts,
            vec![field("year"), field("visit"), field("proposal")]
        );
    }

    #[test]
    fn mixed_literal_and_fields() {
        // Start/end with literal
        let temp = StrTemplate::new("start{visit}middle{year}end").unwrap();
        assert_eq!(
            temp.parts,
            vec![
                literal("start"),
                field("visit"),
                literal("middle"),
                field("year"),
                literal("end")
            ]
        );

        // Start/end with field
        let temp = StrTemplate::new("{year}first{visit}second{proposal}").unwrap();
        assert_eq!(
            temp.parts,
            vec![
                field("year"),
                literal("first"),
                field("visit"),
                literal("second"),
                field("proposal")
            ]
        )
    }

    #[test]
    fn escaped_open() {
        let temp = StrTemplate::new("all {{ literal").unwrap();
        assert_eq!(temp.parts, vec![literal("all { literal")])
    }

    macro_rules! error {
        ($pos:literal, $kind:ident) => {
            TemplateError {
                position: $pos,
                kind: ErrorKind::$kind,
            }
        };
    }

    #[test]
    fn empty_key() {
        let temp = StrTemplate::new("missing {} key").unwrap_err();
        assert_eq!(temp, error!(9, Empty));

        let temp = StrTemplate::new("whitespace {  } key").unwrap_err();
        assert_eq!(temp, error!(14, Empty));
    }

    #[test]
    fn unmatched_close() {
        let temp = StrTemplate::new("closing } only").unwrap();
        assert_eq!(temp.parts, vec![literal("closing } only")]);

        let temp = StrTemplate::new("} closing start").unwrap();
        assert_eq!(temp.parts, vec![literal("} closing start")]);

        let temp = StrTemplate::new("double {close}}").unwrap();
        assert_eq!(
            temp.parts,
            vec![literal("double "), field("close"), literal("}")]
        )
    }

    #[test]
    fn nested_keys() {
        let temp = StrTemplate::new("{nested{keys}}").unwrap_err();
        assert_eq!(temp, error!(7, Nested))
    }

    #[test]
    fn incomplete_key() {
        let temp = StrTemplate::new("incomplete {key").unwrap_err();
        assert_eq!(temp, error!(15, Incomplete));

        let temp = StrTemplate::new("incomplete {").unwrap_err();
        assert_eq!(temp, error!(12, Incomplete));
    }
}

#[cfg(test)]
mod string_templates {
    use std::convert::Infallible;
    use std::fmt::Error;

    use super::*;
    pub type StrTemplate = Template<String>;

    /// FieldSource that replaces every key with the uppercase version of itself
    pub struct EchoSource;

    impl FieldSource<String> for EchoSource {
        type Err = Error;

        fn resolve(&self, field: &String) -> Result<Cow<'_, str>, Self::Err> {
            Ok(field.to_uppercase().into())
        }
    }

    /// Field Source that replaces every key with the empty string
    pub struct NullSource;
    impl FieldSource<String> for NullSource {
        type Err = Infallible;

        fn resolve(&self, _: &String) -> Result<Cow<'_, str>, Self::Err> {
            Ok(Cow::Owned(String::new()))
        }
    }

    /// FieldSource that returns an error for all keys
    pub struct ErrorSource;
    #[derive(Debug, PartialEq, Eq)]
    pub struct RenderFailed(pub String);
    impl FieldSource<String> for ErrorSource {
        type Err = RenderFailed;

        fn resolve(&self, key: &String) -> Result<Cow<'_, str>, Self::Err> {
            Err(RenderFailed(key.to_owned()))
        }
    }
}

#[cfg(test)]
mod template_tests {
    use super::string_templates::*;

    #[test]
    fn literal_template() {
        let text = StrTemplate::new("all literal")
            .unwrap()
            .render(&EchoSource)
            .unwrap();
        assert_eq!(text, "all literal");
    }

    #[test]
    fn mixed() {
        let text = StrTemplate::new("/tmp/{instrument}/data/{year}/{visit}/")
            .unwrap()
            .render(&EchoSource)
            .unwrap();
        assert_eq!(text, "/tmp/INSTRUMENT/data/YEAR/VISIT/");
    }
}

#[cfg(test)]
mod path_template_tests {
    use super::string_templates::*;
    use super::*;

    fn from_template<'a, E, Src>(fmt: &'static str, src: &'a Src) -> PathBuf
    where
        Src: FieldSource<String, Err = E>,
        E: std::fmt::Debug,
    {
        PathTemplate::new(fmt).unwrap().render(src).unwrap()
    }

    #[test]
    fn literal_absolute_path() {
        let path = from_template("/absolute/literal/path", &EchoSource);
        assert_eq!(path, PathBuf::from("/absolute/literal/path"))
    }

    #[test]
    fn templated_absolute_path() {
        let path = from_template("/path/{with}/mixed_{fields}/", &EchoSource);
        assert_eq!(path, PathBuf::from("/path/WITH/mixed_FIELDS/"))
    }

    #[test]
    fn absolute_empty_segment() {
        let path = from_template("/with/{optional}/parts", &NullSource);
        assert_eq!(path, PathBuf::from("/with/parts"))
    }

    #[test]
    fn literal_relative() {
        let path = from_template("relative/literal/path", &EchoSource);
        assert_eq!(path, PathBuf::from("relative/literal/path"));

        let path = from_template("./relative/literal/path", &EchoSource);
        assert_eq!(path, PathBuf::from("relative/literal/path"));
    }

    #[test]
    fn dynamic_relative() {
        let path = from_template("relative/{literal}/path", &EchoSource);
        assert_eq!(path, PathBuf::from("relative/LITERAL/path"));

        let path = from_template("{opening}_dynamic/path", &EchoSource);
        assert_eq!(path, PathBuf::from("OPENING_dynamic/path"));
    }

    #[test]
    fn missing_relative_opener() {
        let path = from_template("{optional}/relative/opener", &NullSource);
        assert_eq!(path, PathBuf::from("relative/opener"))
    }

    #[test]
    fn partial_opener() {
        let path = from_template("{optional}_part/of/relative", &NullSource);
        assert_eq!(path, PathBuf::from("_part/of/relative"))
    }

    #[test]
    fn invalid_path() {
        fn assert_invalid(fmt: &'static str) {
            assert_eq!(
                PathTemplate::<String>::new(fmt).unwrap_err(),
                PathTemplateError::InvalidPath
            )
        }
        assert_invalid("../empty/{segment}");
        assert_invalid("/../empty/{segment}");
    }

    #[test]
    fn failed_rendering() {
        let path = PathTemplate::new("/fail/to/{render}").unwrap();
        assert_eq!(
            path.render(&ErrorSource),
            Err(RenderFailed("render".into()))
        )
    }
}
