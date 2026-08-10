use core::{error, fmt};

use onepass_seed::site::Error as SiteError;

#[derive(Clone, Debug)]
pub enum Error {
    Site(SiteError),
    UrlNotFound,
    UsernameNotFound,
    MultipleChoices(MultipleChoices),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultipleChoices {
    first: String,
    rest: Vec<String>,
}

/// Error returned if `$HOME` is not set in the environment.
#[derive(Clone, Copy, Debug)]
pub struct HomeNotSet;

impl MultipleChoices {
    pub fn new(usernames: impl IntoIterator<Item = String>) -> Self {
        let mut iter = usernames.into_iter();
        let first = iter.next().unwrap();
        let rest = iter.collect();
        MultipleChoices { first, rest }
    }
}

impl From<SiteError> for Error {
    fn from(value: SiteError) -> Self {
        Self::Site(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Site(_) => f.write_str("site deserialization error"),
            Self::UrlNotFound => f.write_str("url not found"),
            Self::UsernameNotFound => f.write_str("username not found"),
            Self::MultipleChoices(MultipleChoices { first, rest }) => {
                write!(f, "multiple username choices: {first}")?;
                for s in rest {
                    write!(f, ", {s}")?;
                }
                Ok(())
            }
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Site(err) => Some(err),
            _ => None,
        }
    }
}

impl error::Error for HomeNotSet {}
impl fmt::Display for HomeNotSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed reading $HOME")
    }
}
