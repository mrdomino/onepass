use core::{error, fmt, num::NonZero};

use serde::{Deserialize, Serialize};

use crate::{
    expr::{Context, Expr, ParseError},
    url::{Error as UrlError, normalize},
    write_tsv,
};

/// A fully parsed [`Site`].
#[derive(Debug)]
pub struct Site<'a> {
    pub url: String,
    pub username: Option<String>,
    pub expr: Expr<'a>,
    pub increment: u32,
}

/// Serialized representation of a [`Site`].
///
/// This type is suitable for storing in e.g config files, and may be serialized or deserialized
/// via [`serde`]. It converts to a [`Site`] via [`TryFrom`], or with a custom [`Context`] via
/// `TryFrom<(RawSite, Context)>`. The generic `S` parameter may be any type that implements
/// [`AsRef<str>`].
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct RawSite<S> {
    pub url: S,
    pub schema: S,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<S>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub increment: Option<NonZero<u32>>,
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
        let url = normalize(url)?;
        let username = username.map(str::to_string);
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
        let url = normalize(url)?;
        let username = username.map(str::to_string);
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

impl<S> RawSite<S>
where
    S: AsRef<str>,
{
    pub fn new(url: S, username: Option<S>, schema: S, increment: u32) -> Self {
        RawSite {
            url,
            username,
            schema,
            increment: NonZero::try_from(increment).ok(),
        }
    }
}

impl<S> From<Site<'_>> for RawSite<S>
where
    S: From<String> + AsRef<str>,
{
    fn from(site: Site<'_>) -> Self {
        let url = S::from(site.url);
        let username = site.username.map(S::from);
        let schema = S::from(format!("{}", site.expr));
        let increment = site.increment;
        RawSite::new(url, username, schema, increment)
    }
}

impl<S> TryFrom<&RawSite<S>> for Site<'_>
where
    S: AsRef<str>,
{
    type Error = Error;
    fn try_from(site: &RawSite<S>) -> Result<Self, Self::Error> {
        Site::new(
            site.url.as_ref(),
            site.username.as_ref().map(S::as_ref),
            site.schema.as_ref(),
            site.increment.map_or(0, u32::from),
        )
    }
}

impl<'a, S> TryFrom<(&RawSite<S>, &'a Context<'a>)> for Site<'a>
where
    S: AsRef<str>,
{
    type Error = Error;
    fn try_from(value: (&RawSite<S>, &'a Context<'a>)) -> Result<Self, Self::Error> {
        let (site, context) = value;
        Site::with_context(
            context,
            site.url.as_ref(),
            site.username.as_ref().map(S::as_ref),
            site.schema.as_ref(),
            site.increment.map_or(0, u32::from),
        )
    }
}

impl fmt::Display for Site<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_tsv!(
            f,
            "v3",
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
        use error::Error;

        fmt::Display::fmt(self.source().unwrap(), f)
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
