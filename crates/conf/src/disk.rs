use std::{
    collections::BTreeMap,
    fs, io,
    num::NonZero,
    path::{Path, PathBuf},
};

use onepass_seed::{
    expr::Context,
    site::{Error as SiteError, Site},
};
use serde::{Deserialize, Serialize};

use crate::dirs::expand_home;

pub const EXAMPLE_CONFIG: &str = concat!(
    "# Other files may be included.\n",
    "# include = [\"local.toml\"]\n",
    "\n",
    "# These settings affect all sites.\n",
    "[global]\n",
    "# The default schema can be overridden.\n",
    "# default_schema = \"{words:5:-}\"\n",
    "\n",
    "# A custom word list may be specified.\n",
    "# words_path = \"/usr/share/dict/words\"\n",
    "\n",
    "[global.keyring]\n",
    "# The OS keyring may be used to store the seed password.\n",
    "# seed = \"cache\"  # or \"off\"\n",
    "\n",
    "# Schemas may have named aliases.\n",
    "[global.alias]\n",
    "apple = '{words:4:-:U}\\d'\n",
    "login = '[[:print:]]{12}'\n",
    "\n",
    "# Sites can be configured by URL, username, schema, and increment.\n",
    "# [[site]]\n",
    "# url = \"google.com\"\n",
    "# username = \"gmail@example\"\n",
    "# schema = \"apple\"\n",
    "# increment = 1\n",
);

/// On-disk representation of a single `onepass` configuration file.
///
/// Compared with [`crate::Config`], this specifies optional include paths and allows any number of
/// sites without any constraints on mapping.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Hash)]
// TODO(someday): better handling for unknown fields. We want an error in some cases, a warning in
// others.
#[serde(deny_unknown_fields)]
pub struct Config {
    /// List of files to be included by this file.
    ///
    /// Files are merged, with paths interpreted relative to the file in which they are contained,
    /// to build up a final [`crate::Config`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<PathBuf>,

    #[serde(default)]
    pub global: Global,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub site: Vec<RawSite<String>>,
}

/// Global settings for `onepass`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Global {
    /// The default schema for any sites that don’t have one of their own. If not specified,
    /// defaults to `{words}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_schema: Option<String>,

    /// Keyring settings (whether to cache/require seed in keyring or populate site passwords in
    /// keyring)
    #[serde(default, skip_serializing_if = "Keyring::is_default")]
    pub keyring: Keyring,

    /// The word list to use for any sites that generate from dictionaries, instead of the built-in
    /// [`EFF wordlist`][onepass_seed::dict::EFF_WORDLIST].
    // TODO(soon): Make the dictionary configurable per site. Probably we want this to be a list of
    // word files, maybe with optional labels and/or parsing instructions, and then we can refer to
    // dicts by hash or by label in per-site schemas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_path: Option<PathBuf>,

    /// A lookup of shorthand names to schema definitions. If a site has a schema that matches one
    /// of the keys of this map, then that key’s value will be substituted when that site is
    /// processed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Keyring {
    #[serde(default, skip_serializing_if = "KeyringSeed::is_default")]
    pub seed: KeyringSeed,
    // TODO(someday): auto-sync site passwords to OS keyring
    // #[serde(default, skip_serializing_if = "KeyringSite::is_default")]
    // pub site: KeyringSite,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum KeyringSeed {
    /// KeyringSeed was not set.
    ///
    /// The behavior is implementation-defined. In `onepass`, the keyring is used if support is
    /// available (like [`Cache`].)
    ///
    /// This value does not override previously specified values on merge.
    ///
    /// [`Cache`]: KeyringSeed::Cache
    #[default]
    Unspecified,

    /// Don’t try to store or load the seed in the OS keyring.
    Off,

    /// Store the seed in the OS keyring, fallback to readpassphrase on error.
    Cache,
    // TODO(soon): require the OS keyring, no readpassphrase fallback
    // Require,
}

/// A pseudo-[`Site`] that is easier to represent on disk.
///
/// Compared with [`Site`], this allows using any [`AsRef<str>`] type, and does not enforce correct
/// URLs or schemas. Incorrect or missing data will result in errors converting from `RawSite` to
/// `Site`.
///
/// Morally speaking, there is a `impl From<Site> for RawSite`, but only
/// `impl TryFrom<RawSite> for Site`. But neither of these quite exist, because there needs to be
/// an optional dictionary passed along as well, and since the current
/// [`Dict`][onepass_seed::dict::Dict] takes a lifetime parameter, the dictionary cannot be easily
/// subbed in here without some changes at a higher level.
#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct RawSite<S> {
    pub url: S,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<S>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<S>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub increment: Option<NonZero<u32>>,

    /// Internal data, reserved for future use by generators. Does not affect derivation paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<S>,

    /// User-facing comment/description. Does not affect generated passwords at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<S>,
}

impl Config {
    /// Read a config from a file, returning it.
    ///
    /// This just does simple deserialization without any traversal of includes; see
    /// [`Config::from_file`].
    pub fn from_file(path: &Path) -> Result<Self, io::Error> {
        let config = fs::read_to_string(path)?;
        toml::from_str(&config).map_err(io::Error::other)
    }
}

impl Global {
    /// Returns the word list from disk as a single string suitable for passing to
    /// [`BoxDict::from_lines`][onepass_seed::dict::BoxDict::from_lines].
    pub fn get_words_string(&self) -> Result<Option<Box<str>>, io::Error> {
        let Some(ref path) = self.words_path else {
            return Ok(None);
        };
        let Ok(ret) = fs::read_to_string(path) else {
            return Ok(None);
        };
        Ok(Some(ret.into_boxed_str()))
    }

    /// Merge `other` into `self`, preferring `other` (i.e. `other` overrides base.)
    pub fn merge(&mut self, other: Global, other_path: &Path) -> Result<(), io::Error> {
        if let Some(s) = other.default_schema {
            self.default_schema = Some(s);
        }
        if let Some(p) = other.words_path {
            self.words_path = Some(resolve_path(other_path, p)?);
        }
        self.keyring.merge(&other.keyring);
        // NB. this silently clobbers aliases in self.
        self.alias.extend(other.alias);
        Ok(())
    }

    /// Returns true if these settings are all unspecified / [`None`].
    pub fn is_empty(&self) -> bool {
        self.default_schema.is_none()
            && self.words_path.is_none()
            && self.keyring.is_default()
            && self.alias.is_empty()
    }
}

impl Keyring {
    pub fn is_default(&self) -> bool {
        self.seed.is_default()
    }

    pub fn merge(&mut self, other: &Self) {
        if !other.seed.is_default() {
            self.seed = other.seed;
        }
    }
}

impl KeyringSeed {
    pub fn is_default(&self) -> bool {
        *self == Default::default()
    }
}

impl<S> RawSite<S>
where
    S: AsRef<str>,
{
    pub fn new(url: S, username: Option<S>, schema: Option<S>, increment: u32) -> Self {
        RawSite {
            url,
            username,
            schema,
            increment: NonZero::new(increment),

            // TODO(someday): fix public API.
            comment: None,
            data: None,
        }
    }

    /// Dereference this site, returning a `RawSite<&str>`.
    pub fn as_deref(&self) -> RawSite<&str> {
        RawSite {
            url: self.url.as_ref(),
            username: self.get_username(),
            schema: self.schema.as_ref().map(S::as_ref),
            increment: self.increment,
            comment: self.comment.as_ref().map(S::as_ref),
            data: self.data.as_ref().map(S::as_ref),
        }
    }

    /// Convert this site to a [`Site`].
    ///
    /// See [`Site::new`].
    pub fn to_site(&self, default_schema: &str) -> Result<Site, SiteError> {
        Site::new(
            self.url.as_ref(),
            self.get_username(),
            self.get_schema(default_schema),
            self.get_increment(),
        )
    }

    /// Convert this site to a [`Site`] with a specific context.
    ///
    /// See [`Site::with_context`].
    pub fn to_site_with_context(
        &self,
        default_schema: &str,
        context: &Context,
    ) -> Result<Site, SiteError> {
        Site::with_context(
            context,
            self.url.as_ref(),
            self.get_username(),
            self.get_schema(default_schema),
            self.get_increment(),
        )
    }

    /// Return the increment for this site as a u32.
    ///
    /// This trivial helper method exists because we use `Option<NonZero<u32>>` to skip serializing
    /// zero values.
    fn get_increment(&self) -> u32 {
        self.increment.map_or(0, NonZero::get)
    }

    fn get_username(&self) -> Option<&str> {
        self.username.as_ref().map(S::as_ref)
    }

    fn get_schema<'a>(&'a self, default: &'a str) -> &'a str {
        self.schema.as_ref().map_or(default, S::as_ref)
    }
}

pub(crate) fn resolve_path(base_path: &Path, path: PathBuf) -> Result<PathBuf, io::Error> {
    let path = expand_home(&path).map_err(io::Error::other)?;
    if path.is_absolute() {
        return Ok(path.into_owned());
    }
    let base_dir = base_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid filename"))?;
    Ok(base_dir.join(path))
}
