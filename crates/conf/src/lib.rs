//! On-disk toml configuration format for [onepass].
//!
//! This crate defines a mapping between toml files and onepass password definitions, allowing
//! users to store and manage their password configurations, including e.g. storing URLs and
//! usernames, changing schemas, and rotating passwords.
//!
//! A crucial piece of the design of this crate is that no secret information should need to be
//! persisted, save the seed password to a secure credential store, in order to use onepass. All
//! other configuration may be shared without compromising site passwords; aside from user privacy
//! concerns about sharing URLs or site activity, there shouldn’t be any issue with posting a
//! onepass configuration file on the public internet. Short of that, copying it from one machine
//! to another via an ordinary backup and restore process should be fine.
//!
//! [onepass]: https://github.com/mrdomino/onepass

pub mod dirs;

use core::{error, fmt};
use std::{
    cmp,
    collections::{BTreeMap, HashSet, VecDeque, btree_map::Entry},
    fs, io,
    num::NonZero,
    ops::Bound,
    path::{Path, PathBuf},
};

use onepass_seed::{
    expr::Context,
    site::{Error as SiteError, Site},
    url::normalize,
};
use serde::{Deserialize, Serialize};

use crate::dirs::{config_dir, expand_home};

/// Finalized user configuration for `onepass`.
///
/// Consists of [global settings][Global] and a map of URL to Site.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub global: Global,

    // Sites sorted by (normalize(url), username).
    site: Vec<RawSite<String>>,
    site_by_url: BTreeMap<String, usize>,
    site_by_key: BTreeMap<(String, Option<String>), usize>,
}

/// On-disk representation of a single `onepass` configuration file.
///
/// Compared with [`Config`], this specifies optional include paths and allows any number of sites
/// without any constraints on mapping.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Hash)]
pub struct DiskConfig {
    /// List of files to be included by this file.
    ///
    /// Files are merged, with paths interpreted relative to the file in which they are contained,
    /// to build up a final [`Config`].
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

    /// The word list to use for any sites that generate from dictionaries, instead of the built-in
    /// [`EFF wordlist`][onepass_seed::dict::EFF_WORDLIST].
    // TODO(soon): Make the dictionary configurable per site. Probably we want this to be a list of
    // word files, maybe with optional labels and/or parsing instructions, and then we can refer to
    // dicts by hash or by label in per-site schemas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_path: Option<PathBuf>,

    /// Whether to store the seed password in the OS keyring.
    // TODO(soon): this is poorly named. We probably want a feature to _populate_ generated site
    // passwords _into_ the OS keyring, as well as this one, which reads the _seed_ password _from_
    // the OS keyring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_keyring: Option<bool>,

    /// A lookup of shorthand names to schema definitions. If a site has a schema that matches one
    /// of the keys of this map, then that key’s value will be substituted when that site is
    /// processed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alias: BTreeMap<String, String>,
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
pub struct RawSite<S> {
    pub url: S,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<S>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<S>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub increment: Option<NonZero<u32>>,
}

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

impl Config {
    #[cfg(test)]
    /// Create a `Config` directly from a string, for tests. Panics if `include` is nonempty.
    pub fn from_str(s: &str) -> Result<Self, io::Error> {
        let ret: DiskConfig = toml::from_str(s).map_err(io::Error::other)?;
        assert!(ret.include.is_empty());
        Config::from_global_site(ret.global, ret.site).map_err(io::Error::other)
    }

    /// Create a `Config` from its constituent parts.
    ///
    /// This normalizes all URLs in the [`RawSite`]s and does the conversion from `S` to
    /// [`String`].
    ///
    /// Duplicate sites are merged by (url, username). The merge logic is that the highest
    /// increment wins, and the last seen schema wins. Because sites from included files come after
    /// sites from the files that included them, this means that local includes can override the
    /// schema from a base config.
    pub fn from_global_site<S>(
        global: Global,
        site: impl IntoIterator<Item = RawSite<S>>,
    ) -> Result<Self, SiteError>
    where
        S: Into<String>,
    {
        // Collect records, merging duplicates.
        let mut map = BTreeMap::new();
        for site in site {
            let url = site.url.into();
            let normal = normalize(&url)?;
            let username = site.username.map(S::into);
            let schema = site.schema.map(S::into);
            let increment = site.increment;

            let k = (normal, username);
            match map.entry(k) {
                Entry::Vacant(v) => {
                    v.insert((url, schema, increment));
                }
                Entry::Occupied(mut o) => {
                    let old = o.get_mut();
                    old.0 = url;
                    if schema.is_some() {
                        old.1 = schema;
                    }
                    old.2 = cmp::max(old.2, increment);
                }
            }
        }
        let site = map
            .into_iter()
            .map(|((normal, username), (url, schema, increment))| {
                (
                    normal,
                    RawSite {
                        url,
                        username,
                        schema,
                        increment,
                    },
                )
            })
            .collect::<Vec<_>>();

        let mut site_by_url = site
            .iter()
            .enumerate()
            .map(|(i, (normal, _))| (normal.as_str(), i))
            .collect::<Vec<_>>();
        site_by_url.dedup_by_key(|&mut (normal, _)| normal);
        let site_by_url = site_by_url
            .into_iter()
            .map(|(normal, i)| (normal.to_string(), i))
            .collect();

        let site_by_key = site
            .iter()
            .enumerate()
            .map(|(i, (normal, site))| ((normal.clone(), site.username.clone()), i))
            .collect();

        let site = site.into_iter().map(|entry| entry.1).collect();

        Ok(Config {
            global,
            site,
            site_by_url,
            site_by_key,
        })
    }

    /// Return the config from disk, creating a default one in some cases.
    ///
    /// This reads and returns the config from the passed path, or from the default config path. If
    /// the path was not overridden and the default config path does not exist, it will be
    /// initialized to default contents.
    pub fn from_or_init(config_path: Option<&Path>) -> Result<Self, io::Error> {
        let default_config_path = config_path
            .is_none()
            .then(Config::default_config_path)
            .transpose()?;
        let base_path = config_path.or(default_config_path.as_deref()).unwrap();
        let res = Config::from_file(base_path);
        if let Some(ref config_path) = default_config_path
            && let Err(error) = res
        {
            if error.kind() == io::ErrorKind::NotFound {
                // Sanity check...
                if fs::exists(config_path)? {
                    return Err(io::Error::other(error));
                }
                eprintln!("Configuration not found; creating one");
                let config = concat!(
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
                    "# The OS keyring may be used to store the seed password.\n",
                    "# use_keyring = true\n",
                    "\n",
                    "# Schemas may have named aliases.\n",
                    "alias = {\n",
                    "    apple = \"{word:U}-{words:3:-}\\\\d\",\n",
                    "    login = \"\\\\w{12}\",\n",
                    "}\n",
                    "\n",
                    "# Sites can be configured by URL, username, schema, and increment.\n",
                    "# [[site]]\n",
                    "# url = \"google.com\"\n",
                    "# username = \"gmail@example\"\n",
                    "# schema = \"apple\"\n",
                    "# increment = 1\n",
                );
                fs::write(config_path, config)?;
                return Ok(Config::default());
            }
            return Err(error);
        }
        res
    }

    /// Reads and returns the config pointed to by the base path.
    ///
    /// This traverses includes, producing a single [`Config`] that is the result of merging all
    /// includes together.
    ///
    /// Conflicts in global config are resolved in favor of the last included file. Conflicts in
    /// site entries are resolved by merge using `(url, username)` as the key, taking the highest
    /// increment and last schema defined for any given entry.
    pub fn from_file(base_path: &Path) -> Result<Self, io::Error> {
        let base_path = expand_home(base_path).canonicalize()?;
        let base_config = DiskConfig::from_file(&base_path)?;

        let mut includes: VecDeque<_> = base_config
            .include
            .into_iter()
            .map(|p| Config::resolve_path(&base_path, p))
            .collect();

        let mut global = base_config.global;
        let mut site = base_config.site;

        let mut visited = HashSet::new();
        visited.insert(base_path);
        while let Some(path) = includes.pop_front() {
            let path = path.canonicalize()?;
            if visited.contains(&path) {
                continue;
            }
            let config = DiskConfig::from_file(&path)?;
            includes.extend(
                config
                    .include
                    .into_iter()
                    .map(|p| Config::resolve_path(&path, p)),
            );

            global.merge(config.global, &path);
            site.extend(config.site);

            visited.insert(path);
        }

        Config::from_global_site(global, site).map_err(io::Error::other)
    }

    #[allow(rustdoc::bare_urls)]
    /// Look up a site.
    ///
    /// This does [URL normalization][normalize] on the input URL, so e.g. "google.com" will look
    /// up "https://google.com/" (and vice versa, since URLs are normalized in the site data too.)
    ///
    /// Schema aliases are resolved, so the returned site is directly usable without further
    /// modification.
    ///
    /// Username resolution works as follows:
    /// 1. If there is an exact `(url, username)` match, that value is returned.
    /// 2. If there is an entry for `(url, None)`, that entry’s value is returned.
    /// 3. If the passed username was `None` and only one entry exists for that URL, that entry is
    ///    returned.
    ///
    /// In other cases, a descriptive error is returned. In case no username was specified and
    /// there were multiple sites at the URL with different usernames, all possible usernames are
    /// returned.
    pub fn find_site<'a>(
        &'a self,
        url: &str,
        username: Option<&'a str>,
    ) -> Result<RawSite<&'a str>, Error> {
        let url = normalize(url).map_err(SiteError::from)?;
        let mut site = self.find_site_raw(url, username)?;
        let schema = site
            .schema
            .map(|name| self.resolve_schema(name))
            .unwrap_or_else(|| self.default_schema());
        site.schema = Some(schema);
        Ok(site)
    }

    // Finds a site by normalized URL, without doing schema resolution.
    fn find_site_raw<'a>(
        &'a self,
        url: String,
        username: Option<&'a str>,
    ) -> Result<RawSite<&'a str>, Error> {
        let key = (url, username.map(String::from));
        if let Some(&i) = self.site_by_key.get(&key) {
            return Ok(self.site[i].as_deref());
        }
        let (url, _) = key;
        if username.is_some() {
            if let Some(&i) = self.site_by_key.get(&(url, None)) {
                let mut site = self.site[i].as_deref();
                site.username = username;
                return Ok(site);
            }
            return Err(Error::UsernameNotFound);
        }

        let Some(&i) = self.site_by_url.get(&url) else {
            return Err(Error::UrlNotFound);
        };
        // Since sites is sorted by normalized url, next is the end of the range for this url.
        let next = self
            .site_by_url
            .range::<String, _>((Bound::Excluded(&url), Bound::Unbounded))
            .next()
            .map(|(_, &v)| v);
        let range = i..next.unwrap_or(self.site.len());

        if range.len() == 1 {
            return Ok(self.site[range.start].as_deref());
        }

        let slice = &self.site[range];
        let mut usernames = slice
            .iter()
            .map(|site| match site.username.as_ref() {
                Some(username) => username.clone(),
                None => unreachable!("a None username would have matched earlier"),
            })
            .collect::<VecDeque<_>>();
        let first = usernames.pop_front().unwrap();

        Err(Error::MultipleChoices(MultipleChoices {
            first,
            rest: usernames.into_iter().collect(),
        }))
    }

    /// Returns the configured default schema, or `"{words}"` if none is specified.
    pub fn default_schema(&self) -> &str {
        self.resolve_schema(self.global.default_schema.as_deref().unwrap_or("{words}"))
    }

    fn resolve_path(base_path: &Path, path: PathBuf) -> PathBuf {
        let path = expand_home(&path);
        if path.is_absolute() {
            return path.into_owned();
        }
        let base_dir = base_path.parent().expect("invalid config path");
        base_dir.join(path)
    }

    fn default_config_path() -> Result<PathBuf, io::Error> {
        let mut path = config_dir().map_err(io::Error::other)?;
        path.push("onepass");
        path.push("config.toml");
        Ok(path)
    }

    pub fn resolve_schema<'a>(&'a self, name: &'a str) -> &'a str {
        self.global.alias.get(name).map_or(name, AsRef::as_ref)
    }
}

impl DiskConfig {
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
    pub fn merge(&mut self, other: Global, other_path: &Path) {
        if let Some(s) = other.default_schema {
            self.default_schema = Some(s);
        }
        if let Some(p) = other.words_path {
            self.words_path = Some(Config::resolve_path(other_path, p));
        }
        if let Some(k) = other.use_keyring {
            self.use_keyring = Some(k);
        }
        // NB. this silently clobbers aliases in self.
        self.alias.extend(other.alias);
    }

    /// Returns true if these settings are all unspecified / [`None`].
    pub fn is_empty(&self) -> bool {
        self.default_schema.is_none()
            && self.words_path.is_none()
            && self.use_keyring.is_none()
            && self.alias.is_empty()
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
        }
    }

    /// Dereference this site, returning a `RawSite<&str>`.
    pub fn as_deref(&self) -> RawSite<&str> {
        RawSite {
            url: self.url.as_ref(),
            username: self.get_username(),
            schema: self.schema.as_ref().map(S::as_ref),
            increment: self.increment,
        }
    }

    /// Convert this site to a [`Site`].
    ///
    /// See [`Site::new`].
    pub fn to_site(&self, default_schema: &str) -> Result<Site<'_>, SiteError> {
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
    pub fn to_site_with_context<'a>(
        &self,
        default_schema: &str,
        context: &'a Context<'a>,
    ) -> Result<Site<'a>, SiteError> {
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

impl From<SiteError> for Error {
    fn from(value: SiteError) -> Self {
        Self::Site(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Site(err) => Some(err),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use tempfile::{NamedTempFile, TempDir};

    use super::*;

    #[test]
    fn it_works() {
        let config = Config::from_str(
            r#"
            [[site]]
            url="google.com"
            "#,
        )
        .unwrap();
        eprintln!("{config:?}");
        let site = &config.site[0];
        assert_eq!("google.com", site.url);
        assert_eq!(None, site.username);
        assert_eq!(None, site.schema);
        assert_eq!(None, site.increment);
    }

    #[test]
    fn test_default_schema_alias() {
        let config = Config::from_str(
            r#"
            [global]
            alias={a="b"}
            default_schema="a"
            [[site]]
            url="google.com"
            "#,
        )
        .unwrap();
        let site = config.find_site("google.com", None).unwrap();
        assert_eq!(Some("b"), site.schema);
        assert_eq!("b", config.default_schema());
    }

    #[test]
    fn test_multiple_usernames() {
        let config = Config::from_str(
            r#"
            [[site]]
            url="google.com"
            username="mrdomino"
            [[site]]
            url="google.com"
            username="bobdole"
            "#,
        )
        .unwrap();
        let site = config.find_site("google.com", Some("mrdomino")).unwrap();
        assert_eq!(Some("{words}"), site.schema);
        assert_eq!(Some("mrdomino"), site.username);
        let site = config.find_site("google.com", Some("bobdole")).unwrap();
        assert_eq!(Some("bobdole"), site.username);
        let Error::UsernameNotFound = config.find_site("google.com", Some("nobody")).unwrap_err()
        else {
            panic!();
        };
        let Error::MultipleChoices(choices) = config.find_site("google.com", None).unwrap_err()
        else {
            panic!();
        };
        assert_eq!(
            MultipleChoices {
                first: "bobdole".into(),
                rest: vec!["mrdomino".into()]
            },
            choices
        );
        let Error::UrlNotFound = config.find_site("yahoo.com", None).unwrap_err() else {
            panic!();
        };

        let config = Config::from_str(
            r#"
            [[site]]
            url="google.com"
            schema="a"
            [[site]]
            url="google.com"
            username="bobdole"
            schema="b"
            "#,
        )
        .unwrap();
        let site = config.find_site("google.com", Some("mrdomino")).unwrap();
        assert_eq!(Some("a"), site.schema);
        assert_eq!(Some("mrdomino"), site.username);
        let site = config.find_site("google.com", Some("bobdole")).unwrap();
        assert_eq!(Some("b"), site.schema);
        assert_eq!(Some("bobdole"), site.username);
        let site = config.find_site("google.com", None).unwrap();
        assert_eq!(Some("a"), site.schema);
        assert_eq!(None, site.username);
    }

    #[test]
    fn test_words_path_resolve() {
        let a = TempDir::new().unwrap();
        let b = TempDir::new().unwrap();

        let a_path = a.path().join("config.toml");
        let b_path = b.path().join("config.toml");
        let b_words_path = b.path().join("words");

        let mut a_file = File::create(&a_path).unwrap();
        let mut b_file = File::create(&b_path).unwrap();
        let mut b_words = File::create(&b_words_path).unwrap();
        write!(a_file, "include=[{:?}]", &b_path).unwrap();
        write!(b_file, "[global]\nwords_path=\"words\"").unwrap();
        write!(b_words, "bob").unwrap();

        let config = Config::from_file(&a_path).unwrap();
        assert_eq!(
            "bob",
            fs::read_to_string(config.global.words_path.unwrap()).unwrap()
        );
    }

    #[test]
    fn test_site_merge() {
        let a = NamedTempFile::new().unwrap();
        let b = NamedTempFile::new().unwrap();
        fs::write(
            a.path(),
            format!(
                concat!(
                    "include=[{:?}]\n",
                    "[[site]]\n",
                    r#"url="google.com""#,
                    "\nincrement=2\n",
                    r#"schema="a""#,
                    "\n",
                ),
                b.path(),
            ),
        )
        .unwrap();
        fs::write(
            b.path(),
            concat!(
                "[[site]]\n",
                r#"url="google.com""#,
                "\nincrement=1\n",
                r#"schema="b""#,
                "\n",
            ),
        )
        .unwrap();

        let config = Config::from_file(a.path()).unwrap();
        let site = config.find_site("google.com", None).unwrap();
        assert_eq!(Some("b"), site.schema.as_deref());
        assert_eq!(2, site.increment.unwrap().get());
    }

    // TODO(soon): more tests
}
