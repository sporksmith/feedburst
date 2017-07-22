use std::{io, fmt};
use reqwest;
#[cfg(unix)]
use xdg;
#[cfg(windows)]
use app_dirs;

#[derive(Debug)]
pub enum Error {
    Msg(String),
    Io(io::Error),
    Request(reqwest::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::Io(ref err) => write!(fmt, "Error performing IO: {}", err),
            Error::Msg(ref err) => write!(fmt, "{}", err),
            Error::Request(ref err) => write!(fmt, "Error making request: {}", err),
        }
    }
}

pub type Span = Option<(usize, usize)>;

#[derive(Clone, Debug, PartialEq)]
pub enum ParseError {
    Expected { character: char, row: usize, span: Span },
    ExpectedMsg { msg: String, row: usize, span: Span },
}

impl ParseError {
    pub fn expected<St: Into<String>, Sp: IntoSpan>(msg: St, row: usize, span: Sp) -> Self {
        ParseError::ExpectedMsg { msg: msg.into(), row, span: span.into_span() }
    }

    pub fn expected_char<Sp: IntoSpan>(character: char, row: usize, span: Sp) -> Self {
        ParseError::Expected { character, row, span: span.into_span() }
    }
}

pub trait IntoSpan {
    fn into_span(self) -> Span;
}

impl IntoSpan for usize {
    fn into_span(self) -> Span {
        Some((self, self))
    }
}

impl IntoSpan for (usize, usize) {
    fn into_span(self) -> Span {
        Some(self)
    }
}

impl IntoSpan for Option<()> {
    fn into_span(self) -> Span {
        None
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Error {
        Error::Request(err)
    }
}

#[cfg(unix)]
impl From<xdg::BaseDirectoriesError> for Error {
    fn from(err: xdg::BaseDirectoriesError) -> Error {
        Error::Msg(format!("{}", err))
    }
}

#[cfg(windows)]
impl From<app_dirs::AppDirsError> for Error {
    fn from(err: app_dirs::AppDirsError) -> Error {
        Error::Msg(format!("{}", err))
    }
}