use core::{
    error,
    fmt::{Display, Formatter, Result},
};

/// Error is a superficially compatible type with keyring::Error. In particular, it must expose a
/// NoEntry option, so this can be checked by the functions that call keyring.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum Error {
    NoEntry,
    Other(anyhow::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Error::NoEntry => write!(f, "entry not found"),
            Error::Other(err) => err.fmt(f),
        }
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Error::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}
