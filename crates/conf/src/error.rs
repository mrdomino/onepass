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
            Self::Site(err) => write!(f, "site error: {err}"),
            Self::UrlNotFound => f.write_str("url not found"),
            Self::UsernameNotFound => f.write_str("username not found"),
            Self::MultipleChoices(MultipleChoices { first, rest }) => {
                write!(f, "multiple choices: {first}")?;
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
