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
pub mod disk;
pub mod error;

use std::{
    cmp,
    collections::{BTreeMap, HashSet, VecDeque, btree_map::Entry},
    fs, io,
    ops::Bound,
    path::{Path, PathBuf},
};

use onepass_seed::{site::Error as SiteError, url::normalize};

use crate::{
    dirs::{config_dir, expand_home},
    disk::resolve_path,
};
// TODO(major): remove some of these public re-exports
pub use crate::{
    disk::{Config as DiskConfig, EXAMPLE_CONFIG, Global, Keyring, KeyringSeed, RawSite},
    error::{Error, MultipleChoices},
};

/// Finalized user configuration for `onepass`.
///
/// Consists of [global settings][Global] and a map of URL to Site.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub global: Global,

    // Sites sorted by (normalize(url), username).
    site: Vec<RawSite<String>>,
    site_by_url: BTreeMap<String, usize>,
    site_by_key: BTreeMap<(String, Option<String>), usize>,
}

impl Config {
    #[cfg(test)]
    /// Create a `Config` directly from a string, for tests. Panics if `include` is nonempty.
    pub fn from_str(s: &str) -> Result<Self, io::Error> {
        let ret: disk::Config = toml::from_str(s).map_err(io::Error::other)?;
        assert!(ret.include.is_empty());
        Config::from_global_site(ret.global, ret.site).map_err(io::Error::other)
    }

    pub fn example() -> Self {
        let mut ret = Config::default();
        ret.global.alias.extend(
            [("apple", "{words:4:-:U}\\d"), ("login", "[[:print:]]{12}")]
                .map(|(a, b)| (a.to_string(), b.to_string())),
        );
        ret
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

            let comment = site.comment.map(S::into);
            let data = site.data.map(S::into);

            let k = (normal, username);
            match map.entry(k) {
                Entry::Vacant(v) => {
                    v.insert((url, schema, increment, comment, data));
                }
                Entry::Occupied(mut o) => {
                    let old = o.get_mut();
                    old.0 = url;
                    if schema.is_some() {
                        old.1 = schema;
                    }
                    old.2 = cmp::max(old.2, increment);
                    if comment.is_some() {
                        old.3 = comment;
                    }
                    match (&old.4, &data) {
                        (_, None) => (),
                        (None, Some(_)) => old.4 = data,
                        (Some(d1), Some(d2)) if d1 == d2 => (),
                        _ => {
                            // TODO(soon): return error here.
                            panic!("Cannot merge data fields {:?} and {:?}", old.4, data);
                        }
                    }
                }
            }
        }
        let site = map
            .into_iter()
            .map(
                |((normal, username), (url, schema, increment, comment, data))| {
                    (
                        normal,
                        RawSite {
                            url,
                            username,
                            schema,
                            increment,
                            comment,
                            data,
                        },
                    )
                },
            )
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
        let config_path = config_path.or(default_config_path.as_deref()).unwrap();
        let res = Config::from_file(config_path);
        if default_config_path.is_some()
            && let Err(error) = res
        {
            if error.kind() == io::ErrorKind::NotFound {
                // Sanity check...
                if fs::exists(config_path)? {
                    return Err(io::Error::other(error));
                }
                eprintln!("Configuration not found; creating one");
                if let Some(config_dir) = config_path.parent() {
                    // E.g. `~/.config/onepass/config.toml`.
                    // We will create `onepass` but not `.config` or above.
                    //
                    // TODO(soon): warn and proceed without config if `.config` does not exist.
                    let _ = fs::create_dir(config_dir);
                }
                fs::write(config_path, EXAMPLE_CONFIG).map_err(|e| {
                    io::Error::new(e.kind(), format!("failed writing {config_path:?}: {e}"))
                })?;
                return Ok(Config::example());
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
        let base_path = expand_home(base_path)
            .map_err(io::Error::other)?
            .canonicalize()
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("failed reading config path {base_path:?}: {e}"),
                )
            })?;
        let disk::Config {
            include,
            mut global,
            mut site,
        } = disk::Config::from_file(&base_path)?;

        let mut includes = include
            .into_iter()
            .map(|p| resolve_path(&base_path, p))
            .collect::<Result<VecDeque<_>, _>>()?;

        let mut visited = HashSet::new();
        visited.insert(base_path);
        while let Some(path) = includes.pop_front() {
            let path = path.canonicalize().map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("failed reading include path {path:?}: {e}"),
                )
            })?;
            if visited.contains(&path) {
                continue;
            }
            let config = disk::Config::from_file(&path)?;

            includes.reserve(config.include.len());
            for p in config.include {
                includes.push_back(resolve_path(&path, p)?);
            }

            global.merge(config.global, &path)?;
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
        let Some(&i) = self.site_by_url.get(&key.0) else {
            return Err(Error::UrlNotFound);
        };
        if username.is_some() {
            let mut site = self.site[i].as_deref();
            if site.username.is_none() {
                site.username = username;
                return Ok(site);
            }
            return Err(Error::UsernameNotFound);
        }

        // Since sites is sorted by normalized url, next is the end of the range for this url.
        let next = self
            .site_by_url
            .range::<String, _>((Bound::Excluded(&key.0), Bound::Unbounded))
            .next()
            .map(|(_, &v)| v);
        let range = i..next.unwrap_or(self.site.len());

        if range.len() == 1 {
            return Ok(self.site[range.start].as_deref());
        }

        let slice = &self.site[range];
        let usernames = slice.iter().map(|site| match site.username.as_ref() {
            Some(username) => username.clone(),
            None => unreachable!("a None username would have matched earlier"),
        });

        Err(Error::MultipleChoices(MultipleChoices::new(usernames)))
    }

    /// Returns the configured default schema, or `"{words}"` if none is specified.
    pub fn default_schema(&self) -> &str {
        self.resolve_schema(self.global.default_schema.as_deref().unwrap_or("{words}"))
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

    pub fn sites(&self) -> &[RawSite<String>] {
        &self.site
    }
}

#[cfg(test)]
mod tests {
    use std::{assert_matches, fs::File, io::Write};

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
        assert_matches!(
            config.find_site("google.com", Some("nobody")),
            Err(Error::UsernameNotFound)
        );
        let Err(Error::MultipleChoices(choices)) = config.find_site("google.com", None) else {
            panic!();
        };
        assert_eq!(
            MultipleChoices::new(vec!["bobdole".into(), "mrdomino".into()]),
            choices
        );
        assert_matches!(config.find_site("yahoo.com", None), Err(Error::UrlNotFound));
        assert_matches!(
            config.find_site("yahoo.com", Some("nobody")),
            Err(Error::UrlNotFound)
        );

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

    #[test]
    fn test_example_config_is_consistent() {
        let config = Config::from_str(EXAMPLE_CONFIG).unwrap();
        assert_eq!(config, Config::example());
    }

    // XXX mostly redundant with `test_multiple_usernames` above
    #[test]
    fn test_find_site() {
        let config = Config::from_global_site(
            Global::default(),
            [RawSite::new("example.com", None, Some("a"), 0)],
        )
        .unwrap();
        // Exact match matches
        let site = config.find_site("example.com", None).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(None, site.username);
        assert_eq!(Some("a"), site.schema);

        // Matching url replaces username
        let site = config.find_site("https://example.com/", Some("b")).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(Some("b"), site.username);
        assert_eq!(Some("a"), site.schema);

        // Absent URL returns error
        let err = config.find_site("google.com", None).unwrap_err();
        assert_matches!(err, Error::UrlNotFound);

        // UrlNotFound takes priority over UsernameNotFound
        let err = config.find_site("google.com", Some("x")).unwrap_err();
        assert_matches!(err, Error::UrlNotFound);

        let config = Config::from_global_site(
            Global::default(),
            [
                RawSite::new("example.com", None, Some("a"), 0),
                RawSite::new("example.com", Some("x"), Some("b"), 0),
            ],
        )
        .unwrap();

        // Exact match matches
        let site = config.find_site("example.com", None).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(None, site.username);
        assert_eq!(Some("a"), site.schema);
        let site = config.find_site("example.com", Some("x")).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(Some("x"), site.username);
        assert_eq!(Some("b"), site.schema);

        // Matching url replaces None entry's username
        let site = config.find_site("example.com", Some("y")).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(Some("y"), site.username);
        assert_eq!(Some("a"), site.schema);

        let config = Config::from_global_site(
            Global::default(),
            [
                RawSite::new("example.com", Some("y"), Some("a"), 0),
                RawSite::new("example.com", Some("x"), Some("b"), 0),
            ],
        )
        .unwrap();
        // Exact match matches
        let site = config.find_site("example.com", Some("x")).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(Some("x"), site.username);
        assert_eq!(Some("b"), site.schema);
        let site = config.find_site("example.com", Some("y")).unwrap();
        assert_eq!("example.com", site.url);
        assert_eq!(Some("y"), site.username);
        assert_eq!(Some("a"), site.schema);

        // Matching url with missing username returns UsernameNotFound
        let err = config.find_site("example.com", Some("z")).unwrap_err();
        assert_matches!(err, Error::UsernameNotFound);

        // Matching url with no username requested returns MultipleChoices
        let err = config.find_site("example.com", None).unwrap_err();
        assert_matches!(err, Error::MultipleChoices(_));
    }

    // TODO(soon): more tests
}
