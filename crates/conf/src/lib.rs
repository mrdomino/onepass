pub mod dirs;

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    fs, io,
    num::NonZero,
    path::{Path, PathBuf},
};

use onepass_seed::{
    expr::Context,
    site::{Error, Site},
    url::normalize,
};
use serde::{Deserialize, Serialize};

use crate::dirs::{config_dir, expand_home};

#[derive(Clone, Debug, Default)]
pub struct Config {
    pub global: Global,
    pub site: BTreeMap<String, RawSite<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Hash)]
pub struct DiskConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<PathBuf>,

    #[serde(default)]
    pub global: Global,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub site: Vec<RawSite<String>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Global {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_path: Option<PathBuf>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_keyring: Option<bool>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aliases: BTreeMap<String, String>,
}

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

impl Config {
    #[cfg(test)]
    pub fn from_str(s: &str) -> Result<Self, io::Error> {
        let ret: DiskConfig = toml::from_str(s).map_err(io::Error::other)?;
        assert!(ret.include.is_empty());
        Config::from_global_site(ret.global, ret.site).map_err(io::Error::other)
    }

    pub fn from_global_site<S>(
        global: Global,
        site: impl IntoIterator<Item = RawSite<S>>,
    ) -> Result<Self, Error>
    where
        S: Into<String>,
    {
        let site = site
            .into_iter()
            .map(|site| -> Result<(String, RawSite<String>), Error> {
                let url = site.url.into();
                let normal = normalize(&url)?;
                Ok((
                    normal,
                    RawSite {
                        url,
                        username: site.username.map(Into::into),
                        schema: site.schema.map(Into::into),
                        increment: site.increment,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Config { global, site })
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
                    "aliases = {\n",
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

    pub fn from_file(base_path: &Path) -> Result<Self, io::Error> {
        let base_path = expand_home(base_path).canonicalize()?;
        let base_config = DiskConfig::from_file(&base_path)?;

        let mut includes: VecDeque<_> = base_config
            .include
            .into_iter()
            .map(|p| Config::resolve_path(&base_path, p))
            .collect();
        let mut visited = HashSet::new();

        let mut global = base_config.global;
        let mut site = base_config.site;

        while let Some(include_path) = includes.pop_front() {
            let path = Config::resolve_path(&base_path, include_path);
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
            visited.insert(path);

            global.merge(config.global);
            site.extend(config.site);
        }

        Config::from_global_site(global, site).map_err(io::Error::other)
    }

    pub fn find_site<'a>(&'a self, url: &str) -> Result<Option<(String, RawSite<&'a str>)>, Error> {
        let url = normalize(url)?;
        let Some(site) = self.site.get(&url) else {
            return Ok(None);
        };
        let mut site = site.as_deref();
        if let Some(pattern) = site.schema.and_then(|name| self.global.aliases.get(name)) {
            site.schema = Some(pattern.as_ref());
        }
        Ok(Some((url, site)))
    }

    pub fn default_schema(&self) -> &str {
        self.global.default_schema.as_deref().unwrap_or("{words}")
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
}

impl DiskConfig {
    pub fn from_file(path: &Path) -> Result<Self, io::Error> {
        let config = fs::read_to_string(path)?;
        toml::from_str(&config).map_err(io::Error::other)
    }
}

impl Global {
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
    pub fn merge(&mut self, other: Global) {
        other
            .default_schema
            .into_iter()
            .for_each(|s| self.default_schema = Some(s));
        // TODO(soon): words_path should be relative to other, not self.
        other
            .words_path
            .into_iter()
            .for_each(|p| self.words_path = Some(p));
        other
            .use_keyring
            .into_iter()
            .for_each(|v| self.use_keyring = Some(v));
        self.aliases.extend(other.aliases);
    }

    pub fn is_empty(&self) -> bool {
        self.default_schema.is_none()
            && self.words_path.is_none()
            && self.use_keyring.is_none()
            && self.aliases.is_empty()
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

    pub fn as_deref(&self) -> RawSite<&str> {
        RawSite {
            url: self.url.as_ref(),
            username: self.get_username(),
            schema: self.schema.as_ref().map(S::as_ref),
            increment: self.increment,
        }
    }

    pub fn to_site(&self, default_schema: &str) -> Result<Site<'_>, Error> {
        Site::new(
            self.url.as_ref(),
            self.get_username(),
            self.get_schema(default_schema),
            self.get_increment(),
        )
    }

    pub fn to_site_with_context<'a>(
        &self,
        default_schema: &str,
        context: &'a Context<'a>,
    ) -> Result<Site<'a>, Error> {
        Site::with_context(
            context,
            self.url.as_ref(),
            self.get_username(),
            self.get_schema(default_schema),
            self.get_increment(),
        )
    }

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

#[cfg(test)]
mod tests {
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
        let site = &config.site["https://google.com/"];
        assert_eq!("google.com", site.url);
        assert_eq!(None, site.username);
        assert_eq!(None, site.schema);
        assert_eq!(None, site.increment);
        assert!(!config.site.contains_key("yahoo.com"));
    }

    // TODO(soon): more tests
}
