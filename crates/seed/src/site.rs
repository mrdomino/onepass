use core::{error, fmt};

use crate::{
    expr::{Context, Expr, ParseError},
    url::{Error as UrlError, normalize},
    write_tsv,
};

/// A fully parsed [`Site`].
#[derive(Debug)]
pub struct Site<'a> {
    pub url: Box<str>,
    pub username: Option<Box<str>>,
    pub expr: Expr<'a>,
    pub increment: u32,
}

/// Represents an error deserializing a [`Site`].
#[derive(Clone, Debug)]
pub enum Error {
    Parse(ParseError),
    Url(UrlError),
}

impl Site<'_> {
    pub fn new(
        url: &str,
        username: Option<&str>,
        schema: &str,
        increment: u32,
    ) -> Result<Self, Error> {
        let url = normalize(url)?.into_boxed_str();
        let username = username.map(Box::from);
        let expr = Expr::new(schema.parse()?);
        Ok(Site {
            url,
            username,
            expr,
            increment,
        })
    }
}

impl<'a> Site<'a> {
    pub fn with_expr(
        url: &str,
        username: Option<&str>,
        expr: Expr<'a>,
        increment: u32,
    ) -> Result<Self, Error> {
        let url = normalize(url)?.into_boxed_str();
        let username = username.map(Box::from);
        Ok(Site {
            url,
            username,
            expr,
            increment,
        })
    }

    pub fn with_context(
        ctx: &'a Context<'a>,
        url: &str,
        username: Option<&str>,
        schema: &str,
        increment: u32,
    ) -> Result<Self, Error> {
        let expr = Expr::with_context(schema.parse()?, ctx);
        Self::with_expr(url, username, expr, increment)
    }
}

impl fmt::Display for Site<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_tsv!(
            f,
            "v3/priv",
            &self.url,
            &self.username.as_deref().unwrap_or(""),
            &self.expr,
            self.increment
        )
    }
}

impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        use Error::*;

        Some(match self {
            Parse(e) => e,
            Url(e) => e,
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (t, e): (&str, &dyn error::Error) = match self {
            Error::Parse(e) => ("parse", e),
            Error::Url(e) => ("url", e),
        };
        write!(f, "{t}: {e}")
    }
}

impl From<ParseError> for Error {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<UrlError> for Error {
    fn from(e: UrlError) -> Self {
        Self::Url(e)
    }
}
