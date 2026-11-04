//! Error types

use abscissa_core::error::{BoxError, Context};
use derive_more::Display;
use std::error::Error as StdError;
use std::{io, ops::Deref};
use thiserror::Error;

/// Kinds of errors
#[derive(Copy, Clone, Debug, Eq, Error, PartialEq)]
pub enum ErrorKind {
    /// Error in configuration file
    #[error("config error")]
    Config,

    /// Input/output error
    #[error("I/O error")]
    Io,
}

impl ErrorKind {
    /// Create an error context from this error
    pub fn context(self, source: impl Into<BoxError>) -> Context<Self> {
        Context::new(self, Some(source.into()))
    }
}

/// Error type
#[derive(Debug, Display)]
pub struct Error(Box<Context<ErrorKind>>);

impl Deref for Error {
    type Target = Context<ErrorKind>;

    fn deref(&self) -> &Context<ErrorKind> {
        &self.0
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.0.source()
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Context::new(kind, None).into()
    }
}

impl From<Context<ErrorKind>> for Error {
    fn from(context: Context<ErrorKind>) -> Self {
        Self(Box::new(context))
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        ErrorKind::Io.context(err).into()
    }
}
