//! The error every genie crate returns ([`GenieError`]).
//!
//! An error names its [`ErrorKind`], its reason (a fixed text: the same failure gives the same
//! reason in every run, so reports group by it), where it was raised (the file and line of the
//! constructor's caller, through `#[track_caller]`) and the context its site attaches (key and
//! value pairs, [`GenieError::with`]). Two errors are equal when their kinds and reasons are:
//! the place and the context tell where and on what it happened, not what it is.
//!
//! Constructing one costs no allocation: the reason and the place are static, and the context
//! takes its box only when a site attaches some. So an error may be built eagerly
//! (`option.ok_or(unsupported("…"))`) on a path that runs every update.
//!
//! The crate depends on no other crate of the workspace: every one may depend on it.

#[cfg(feature = "serde")]
use serde::de::Error as SerdeError;
use std::error::Error;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Error as IoError;
use std::panic::Location;
use std::sync::Arc;

/// A [`Result`] whose error is a [`GenieError`].
pub type GenieResult<T> = Result<T, GenieError>;

/// What kind of failure an error is.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// A part of the game the port does not model yet: a branch, a routine or a case of one.
    Unsupported,
    /// A state the game's own invariants rule out: a defect of the port or of its input.
    Unreachable,
    /// Data the game reads that the input lacks: a master, a sprite or a table the data files do
    /// not hold.
    MissingData,
    /// Input that breaks its format: a recording's command, a save's field, a file's layout.
    InvalidInput,
    /// Reading or writing failed underneath: a file, a stream, an inflate.
    Io,
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unsupported => "not modelled",
            Self::Unreachable => "unreachable",
            Self::MissingData => "missing data",
            Self::InvalidInput => "invalid input",
            Self::Io => "input or output",
        })
    }
}

/// The error of every genie crate: its kind, its reason, where it was raised and its context.
#[derive(Clone)]
pub struct GenieError {
    kind: ErrorKind,
    reason: &'static str,
    at: &'static Location<'static>,
    extra: Option<Box<Extra>>,
}

/// What a site attaches to an error; boxed, so an error without it stays small.
#[derive(Clone, Debug, Default)]
struct Extra {
    context: Vec<(&'static str, String)>,
    source: Option<Arc<dyn Error + Send + Sync>>,
}

impl GenieError {
    /// An error of `kind` for `reason`, raised where this is called.
    #[must_use]
    #[track_caller]
    pub const fn new(kind: ErrorKind, reason: &'static str) -> Self {
        Self {
            kind,
            reason,
            at: Location::caller(),
            extra: None,
        }
    }

    /// A part of the game the port does not model yet ([`ErrorKind::Unsupported`]).
    #[must_use]
    #[track_caller]
    pub const fn unsupported(reason: &'static str) -> Self {
        Self::new(ErrorKind::Unsupported, reason)
    }

    /// A state the game's invariants rule out ([`ErrorKind::Unreachable`]).
    #[must_use]
    #[track_caller]
    pub const fn unreachable(reason: &'static str) -> Self {
        Self::new(ErrorKind::Unreachable, reason)
    }

    /// Data the input lacks ([`ErrorKind::MissingData`]).
    #[must_use]
    #[track_caller]
    pub const fn missing_data(reason: &'static str) -> Self {
        Self::new(ErrorKind::MissingData, reason)
    }

    /// Input that breaks its format ([`ErrorKind::InvalidInput`]).
    #[must_use]
    #[track_caller]
    pub const fn invalid_input(reason: &'static str) -> Self {
        Self::new(ErrorKind::InvalidInput, reason)
    }

    /// Reading or writing failed underneath ([`ErrorKind::Io`]).
    #[must_use]
    #[track_caller]
    pub const fn io(reason: &'static str) -> Self {
        Self::new(ErrorKind::Io, reason)
    }

    /// The error with `key` and `value` added to its context.
    #[must_use]
    pub fn with(mut self, key: &'static str, value: impl fmt::Display) -> Self {
        self.extra
            .get_or_insert_with(Box::default)
            .context
            .push((key, value.to_string()));
        self
    }

    /// The error caused by `source`.
    #[must_use]
    pub fn with_source(mut self, source: impl Error + Send + Sync + 'static) -> Self {
        self.extra.get_or_insert_with(Box::default).source = Some(Arc::new(source));
        self
    }

    /// The error raised at `at` rather than where it was built: a constant error (one reason
    /// the sites share) takes the place of the site that returns it.
    #[must_use]
    pub const fn raised_at(mut self, at: &'static Location<'static>) -> Self {
        self.at = at;
        self
    }

    /// Its kind.
    #[must_use]
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Its reason: a fixed text, the same for the same failure in every run.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    /// Where it was raised: the file, line and column of the constructor's caller.
    #[must_use]
    pub const fn location(&self) -> &'static Location<'static> {
        self.at
    }

    /// Its context, in the order the sites attached it.
    pub fn context(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.extra
            .iter()
            .flat_map(|extra| extra.context.iter().map(|(k, text)| (*k, text.as_str())))
    }
}

/// [`GenieError::unsupported`]: a part of the game the port does not model yet.
#[must_use]
#[track_caller]
pub const fn unsupported(reason: &'static str) -> GenieError {
    GenieError::new(ErrorKind::Unsupported, reason)
}

/// [`GenieError::unreachable`]: a state the game's invariants rule out.
#[must_use]
#[track_caller]
pub const fn unreachable(reason: &'static str) -> GenieError {
    GenieError::new(ErrorKind::Unreachable, reason)
}

/// [`GenieError::missing_data`]: data the input lacks.
#[must_use]
#[track_caller]
pub const fn missing_data(reason: &'static str) -> GenieError {
    GenieError::new(ErrorKind::MissingData, reason)
}

/// [`GenieError::invalid_input`]: input that breaks its format.
#[must_use]
#[track_caller]
pub const fn invalid_input(reason: &'static str) -> GenieError {
    GenieError::new(ErrorKind::InvalidInput, reason)
}

impl From<IoError> for GenieError {
    /// A failed read or write ([`ErrorKind::Io`]) caused by `error`, raised where `?` converts it.
    #[track_caller]
    fn from(error: IoError) -> Self {
        Self::io("a read or write that failed").with_source(error)
    }
}

#[cfg(feature = "serde")]
impl SerdeError for GenieError {
    /// A value that does not deserialize into its field ([`ErrorKind::InvalidInput`]), `why` in
    /// its context.
    #[track_caller]
    fn custom<T: fmt::Display>(why: T) -> Self {
        Self::invalid_input("a value that does not deserialize into its field").with("why", why)
    }
}

impl PartialEq for GenieError {
    /// Equal kinds and reasons: the place and the context do not make another error.
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.reason == other.reason
    }
}

impl Eq for GenieError {}

impl Hash for GenieError {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
        self.reason.hash(state);
    }
}

impl fmt::Debug for GenieError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("GenieError");
        let _ = debug
            .field("kind", &self.kind)
            .field("reason", &self.reason)
            .field("at", &format_args!("{}", self.at));
        if let Some(extra) = &self.extra {
            if !extra.context.is_empty() {
                let _ = debug.field("context", &extra.context);
            }
            if let Some(s) = &extra.source {
                let _ = debug.field("source", s);
            }
        }
        debug.finish()
    }
}

impl fmt::Display for GenieError {
    /// `reason (key: value, …) at file:line:column`, then `: source` when it has one.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)?;
        let mut context = self.context().peekable();
        if context.peek().is_some() {
            formatter.write_str(" (")?;
            for (index, (key, value)) in context.enumerate() {
                if 0 < index {
                    formatter.write_str(", ")?;
                }
                write!(formatter, "{key}: {value}")?;
            }
            formatter.write_str(")")?;
        }
        write!(formatter, " at {}", self.at)?;
        if let Some(source) = self.extra.as_ref().and_then(|extra| extra.source.as_ref()) {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl Error for GenieError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        let source = self.extra.as_ref()?.source.as_deref()?;
        let source: &(dyn Error + 'static) = source;
        Some(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde's custom error is invalid input with its why.
    #[cfg(feature = "serde")]
    #[test]
    fn serde_custom_error_is_invalid_input() {
        let error = GenieError::custom("not a number");
        assert_eq!(
            (error.kind(), error.reason()),
            (
                ErrorKind::InvalidInput,
                "a value that does not deserialize into its field"
            )
        );
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [("why", "not a number")]
        );
    }

    /// A constant error, whose place is its definition's.
    const SHARED: GenieError = unsupported("a shared reason");

    #[test]
    fn an_error_names_where_it_was_raised() {
        let line = line!() + 1;
        let error = unsupported("a branch not modelled");
        assert_eq!(
            (
                error.kind(),
                error.reason(),
                error.location().file(),
                error.location().line()
            ),
            (
                ErrorKind::Unsupported,
                "a branch not modelled",
                file!(),
                line
            )
        );
        assert_eq!(SHARED.location().file(), file!());
        let here = Location::caller();
        assert_eq!(SHARED.raised_at(here).location(), here);
    }

    /// Each kind's name in an error's text.
    #[test]
    fn each_kind_has_its_name() {
        let names = [
            (ErrorKind::Unsupported, "not modelled"),
            (ErrorKind::Unreachable, "unreachable"),
            (ErrorKind::MissingData, "missing data"),
            (ErrorKind::InvalidInput, "invalid input"),
            (ErrorKind::Io, "input or output"),
        ];
        for (kind, name) in names {
            assert_eq!(kind.to_string(), name);
        }
    }

    #[test]
    fn errors_are_equal_by_kind_and_reason_alone() {
        let a = unsupported("a").with("id", 7);
        let error = unsupported("a");
        assert_eq!(a, error);
        assert_ne!(unsupported("a"), unreachable("a"));
        assert_ne!(unsupported("a"), unsupported("b"));
    }

    #[test]
    fn the_context_follows_the_reason_and_the_place_ends_the_text() {
        let error = missing_data("a sprite the data file does not have")
            .with("sprite", 214)
            .with("object", -16_347_136);
        assert_eq!(
            error.context().collect::<Vec<_>>(),
            [("sprite", "214"), ("object", "-16347136")]
        );
        let text = error.to_string();
        assert!(text.starts_with(
            "a sprite the data file does not have (sprite: 214, object: -16347136) at "
        ));
        assert!(text.contains(file!()));
        let bare = unreachable("x");
        assert_eq!(bare.to_string(), format!("x at {}", bare.location()));
    }

    #[test]
    fn source_is_the_cause() {
        let io = IoError::other("short read");
        let error = invalid_input("a header too short").with_source(io);
        assert_eq!(
            Error::source(&error).map(ToString::to_string),
            Some("short read".to_owned())
        );
        assert!(error.to_string().ends_with(": short read"));
    }

    #[test]
    fn an_io_error_converts_where_question_mark_meets_it() {
        fn open() -> GenieResult<()> {
            Err(IoError::other("no such file"))?;
            Ok(())
        }
        let line = line!() - 3;
        let error = open().unwrap_err();
        assert_eq!(
            (error.kind(), error.location().line()),
            (ErrorKind::Io, line)
        );
        assert!(error.to_string().ends_with(": no such file"));
    }

    #[test]
    fn an_error_stays_small_and_is_built_without_box() {
        // the kind, the reason, the place and the context's box
        assert!(size_of::<GenieError>() <= 40);
        assert!(size_of::<GenieResult<()>>() <= 40);
        assert_eq!(unsupported("x").context().count(), 0);
    }
}
