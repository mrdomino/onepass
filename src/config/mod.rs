// Copyright 2025 Steven Dee
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod dirs;

use std::{
    collections::{BTreeMap, HashSet},
    fs::{create_dir_all, read_to_string, write},
    mem,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use dirs::{config_dir, expand_home};
use serde::{Deserialize, Serialize};

use onepass_seed::url::normalize;

pub(crate) struct Config {
    pub words_path: Option<Box<Path>>,
    default_schema: Option<String>,
    pub use_keyring: Option<bool>,
    pub aliases: BTreeMap<String, String>,
    pub sites: BTreeMap<String, SiteConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SiteConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub increment: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

const DEFAULT_SCHEMA: &str = "[A-Za-z0-9]{16}";

impl Config {
    pub fn from_file(path: Option<&Path>) -> Result<Self> {
        let path = path
            .map_or_else(Self::default_path, |p| Some(p.into()))
            .context("failed finding config dir")?;
        if !path.exists() {
            create_dir_all(path.parent().context("invalid config path")?)?;
            write(&path, serde_yaml::to_string(&SerConfig::example())?)?;
        }
        let mut loader = ConfigLoader::new();
        loader.load(&path)
    }

    #[cfg(test)]
    pub fn from_str(s: &str) -> Result<Self> {
        let config: SerConfig = serde_yaml::from_str(s)?;
        Self::from_ser_config(config, &PathBuf::from("/a"))
    }

    pub fn find_site(&self, url: &str) -> Result<Option<(String, &SiteConfig)>> {
        let url = normalize(url, None)?;
        let Some(site) = self.sites.get(&url) else {
            return Ok(None);
        };
        let url = normalize(&url, site.username.as_deref())?;
        Ok(Some((url, site)))
    }

    pub fn default_schema(&'_ self) -> &'_ str {
        self.default_schema.as_deref().unwrap_or(DEFAULT_SCHEMA)
    }

    pub fn site_schema<'a>(&'a self, config: &'a SiteConfig) -> &'a str {
        config
            .schema
            .as_deref()
            .unwrap_or_else(|| self.default_schema())
    }

    /// Merge other into self, preferring other over self.
    fn extend(&mut self, mut other: Config) {
        other
            .words_path
            .into_iter()
            .for_each(|p| self.words_path = Some(p));
        other
            .default_schema
            .into_iter()
            .for_each(|s| self.default_schema = Some(s));
        other
            .use_keyring
            .into_iter()
            .for_each(|v| self.use_keyring = Some(v));
        self.aliases.extend(mem::take(&mut other.aliases));
        // TODO(someday): merge and apply schema aliases in a more principled way.
        // This only applies base schemas to included sites; it does not apply included schemas
        // to base sites. To do the latter seems like it would require a more substantial rework
        // of the schema alias code.
        for (k, mut config) in other.sites {
            if let Some(schema) = config
                .schema
                .as_deref()
                .and_then(|schema| self.aliases.get(schema))
            {
                config.schema = Some(schema.clone());
            }
            self.sites.insert(k, config);
        }
    }

    fn from_ser_config(config: SerConfig, config_path: &Path) -> Result<Self> {
        let words_path = config
            .words_path
            .map(|p| -> Result<Box<Path>> {
                let mut path = expand_home(p).context("expand_home failed")?;
                if path.is_relative() {
                    path = config_path.join(path);
                }
                Ok(path.into())
            })
            .transpose()?;
        let aliases = config.aliases;
        let default_schema = config
            .default_schema
            .map(|schema| aliases.get(&schema).map_or(schema, Clone::clone));
        let use_keyring = config.use_keyring;
        let sites = config
            .sites
            .into_iter()
            .map(|(mut site, mut config)| {
                if let Some(schema) = config
                    .schema
                    .as_deref()
                    .and_then(|schema| aliases.get(schema))
                {
                    config.schema = Some(schema.clone());
                }
                // TODO: print warnings on parse errors here
                if let Ok(url) = normalize(&site, None) {
                    site = url;
                }
                (site, config)
            })
            .collect();
        Ok(Config {
            words_path,
            default_schema,
            use_keyring,
            aliases,
            sites,
        })
    }

    fn default_path() -> Option<Box<Path>> {
        let mut config_dir = config_dir()?;
        config_dir.push("onepass");
        config_dir.push("config.yaml");
        Some(config_dir.into_boxed_path())
    }
}

pub struct ConfigLoader {
    visited_files: HashSet<PathBuf>,
}

impl ConfigLoader {
    pub fn new() -> Self {
        Self {
            visited_files: HashSet::new(),
        }
    }

    pub fn load<P: AsRef<Path>>(&mut self, path: P) -> Result<Config> {
        let path = path.as_ref();
        let canonical_path = path.canonicalize()?;
        if self.visited_files.contains(&canonical_path) {
            anyhow::bail!("circular dependency");
        }
        self.visited_files.insert(canonical_path.clone());
        let contents = read_to_string(&canonical_path)?;
        let mut ser_config: SerConfig = serde_yaml::from_str(&contents)?;
        let include = mem::take(&mut ser_config.include);
        let mut config = Config::from_ser_config(
            ser_config,
            canonical_path.parent().context("failed getting parent")?,
        )?;
        if !include.is_empty() {
            let base_dir = canonical_path
                .parent()
                .context("failed to get parent dir")?;
            for include_path in &include {
                let resolved_path = self.resolve_include_path(include_path, base_dir)?;
                match self.load(&resolved_path) {
                    Ok(included_config) => config.extend(included_config),
                    Err(e) => eprintln!("Loading {}: {}", resolved_path.display(), e),
                }
            }
        }
        self.visited_files.remove(&canonical_path);
        Ok(config)
    }

    fn resolve_include_path(&self, include_path: &Path, base_dir: &Path) -> Result<PathBuf> {
        let mut path = expand_home(include_path).context("failed home expansion")?;
        if path.is_relative() {
            path = base_dir.join(path);
        }
        Ok(path)
    }
}

#[derive(Debug, Deserialize)]
struct SerConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub words_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_None")]
    pub default_schema: Option<String>,
    #[serde(default)]
    pub use_keyring: Option<bool>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(deserialize_with = "deserialize_sites")]
    pub sites: BTreeMap<String, SiteConfig>,
}

impl SerConfig {
    fn example() -> Self {
        let aliases: BTreeMap<String, String> = [
            ("alnum", "[A-Za-z0-9]{18}"),
            ("apple", "[:Word:](-[:word:]){3}[0-9!-/]"),
            ("login", "[!-~]{12}"),
            ("mobile", "[a-z0-9]{24}"),
            ("phrase", "[:word:](-[:word:]){4}"),
            ("pin", "[0-9]{8}"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let sites: BTreeMap<String, SiteConfig> = [
            ("apple.com", ("apple", 0)),
            ("google.com", ("mobile", 0)),
            ("iphone.local", ("pin", 1)),
        ]
        .into_iter()
        .map(|(k, (schema, increment))| {
            (
                k.into(),
                SiteConfig {
                    schema: Some(schema.into()),
                    increment,
                    username: None,
                },
            )
        })
        .collect();
        let default_schema = Some("login".to_string());
        let use_keyring = Some(cfg!(not(target_os = "linux")));
        SerConfig {
            include: Vec::new(),
            words_path: None,
            default_schema,
            use_keyring,
            aliases,
            sites,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum SchemaOrSiteConfig {
    Empty,
    Schema(String),
    Config(SiteConfig),
}

impl From<SchemaOrSiteConfig> for SiteConfig {
    fn from(value: SchemaOrSiteConfig) -> Self {
        match value {
            SchemaOrSiteConfig::Empty => SiteConfig {
                schema: None,
                increment: 0,
                username: None,
            },
            SchemaOrSiteConfig::Schema(schema) => SiteConfig {
                schema: Some(schema),
                increment: 0,
                username: None,
            },
            SchemaOrSiteConfig::Config(config) => config,
        }
    }
}

impl From<&SiteConfig> for SchemaOrSiteConfig {
    fn from(config: &SiteConfig) -> Self {
        if let Some(schema) = config.schema.as_deref()
            && is_zero(&config.increment)
            && config.username.is_none()
        {
            SchemaOrSiteConfig::Schema(schema.to_string())
        } else {
            SchemaOrSiteConfig::Config(SiteConfig {
                schema: config.schema.clone(),
                increment: config.increment,
                username: config.username.clone(),
            })
        }
    }
}

impl Serialize for SerConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("SerConfig", 3)?;
        if let Some(path) = &self.words_path {
            state.serialize_field("words_path", path)?;
        }
        state.serialize_field("default_schema", &self.default_schema)?;
        if let Some(use_keyring) = &self.use_keyring {
            state.serialize_field("use_keyring", use_keyring)?;
        }
        state.serialize_field("aliases", &self.aliases)?;

        let sites_for_serialization: BTreeMap<String, SchemaOrSiteConfig> = self
            .sites
            .iter()
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();

        state.serialize_field("sites", &sites_for_serialization)?;
        state.end()
    }
}

fn deserialize_sites<'de, D>(deserializer: D) -> Result<BTreeMap<String, SiteConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let sites: BTreeMap<String, SchemaOrSiteConfig> = BTreeMap::deserialize(deserializer)?;
    Ok(sites.into_iter().map(|(k, v)| (k, v.into())).collect())
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write};

    use anyhow::Result;
    use tempfile::{NamedTempFile, TempDir};

    use super::*;

    #[test]
    fn basic() -> Result<()> {
        let config: SerConfig = serde_yaml::from_str("sites:\n google.com: \"[A-Z]\"")?;
        let goog = &config.sites["google.com"];
        assert_eq!(Some("[A-Z]"), goog.schema.as_deref());
        Ok(())
    }

    #[test]
    fn object() -> Result<()> {
        let config: SerConfig =
            serde_yaml::from_str("sites:\n abcd:\n  schema: \"A\"\n  increment: 1\n")?;
        let abcd = &config.sites["abcd"];
        assert_eq!(1, abcd.increment);
        assert_eq!(Some("A"), abcd.schema.as_deref());
        Ok(())
    }

    #[test]
    fn find_site_ok() -> Result<()> {
        let config = Config::from_str(
            r#"
            default_schema: DEF
            sites:
                google.com:
                    schema: A
                    username: "test@gmail.com"
                apple.com:
                    schema: B
                "http://localhost":
                    schema: C
                example.com:
            "#,
        )?;
        let tests = [
            (
                Some(("https://test%40gmail.com@google.com/", "A")),
                "google.com",
            ),
            (Some(("https://apple.com/", "B")), "https://apple.com"),
            (Some(("http://localhost/", "C")), "http://localhost/"),
            (None, "localhost"),
            (Some(("https://example.com/", "DEF")), "https://example.com"),
        ];
        for (want, input) in tests {
            let got = config.find_site(input)?;
            match (want, got) {
                (Some((want_url, want_schema)), Some((got_url, got_config))) => {
                    assert_eq!(want_url, &got_url);
                    assert_eq!(want_schema, config.site_schema(got_config));
                }
                (None, None) => (),
                (want, got) => panic!("mismatch: {want:?} / {got:?}"),
            };
        }
        Ok(())
    }

    #[test]
    fn find_incr() -> Result<()> {
        let config = Config::from_str(
            r#"
            sites:
                example.com:
                    increment: 1
            "#,
        )?;
        let (_, site) = config.find_site("example.com")?.context("fail")?;
        assert_eq!(1, site.increment);
        assert_eq!(DEFAULT_SCHEMA, config.site_schema(site));
        Ok(())
    }

    #[test]
    fn temp_config_file() -> Result<()> {
        let mut config_file = NamedTempFile::new()?;
        write!(
            config_file,
            r#"
            sites:
                google.com:
                    schema: '[A-Z]{{0}}'
            "#,
        )?;
        let config = Config::from_file(Some(config_file.path()))?;
        let (u, site) = config.find_site("google.com")?.context("fail")?;
        assert_eq!(Some("[A-Z]{0}"), site.schema.as_deref());
        assert_eq!("https://google.com/", &u);
        Ok(())
    }

    #[test]
    fn test_include() -> Result<()> {
        let mut config_file = NamedTempFile::new()?;
        let dir = TempDir::new()?;
        let a_path = dir.path().join("a.yaml");
        let b_dir = dir.path().join("b");
        create_dir_all(&b_dir)?;
        let b_path = b_dir.join("b.yaml");
        let b_words_path = b_dir.join("words");
        writeln!(config_file, "include:")?;
        writeln!(config_file, "- {}", a_path.display())?;
        writeln!(config_file, "aliases:")?;
        writeln!(config_file, " a: '[A-Z]{{4}}'")?;
        writeln!(config_file, "sites:")?;
        let mut a_file = File::create(&a_path)?;
        writeln!(a_file, "include:")?;
        writeln!(a_file, "- b/b.yaml")?;
        writeln!(a_file, "sites:")?;
        let mut b_file = File::create(&b_path)?;
        writeln!(b_file, "words_path: words")?;
        writeln!(b_file, "sites:")?;
        writeln!(b_file, " google.com:")?;
        writeln!(b_file, "  schema: a")?;
        let mut b_words_file = File::create(&b_words_path)?;
        writeln!(b_words_file, "aAa")?;
        writeln!(b_words_file, "bB")?;
        let config = Config::from_file(Some(config_file.path()))?;
        let (u, site) = config.find_site("google.com")?.context("fail")?;
        assert_eq!("https://google.com/", &u);
        assert_eq!(Some("[A-Z]{4}"), site.schema.as_deref());
        assert_eq!(Some(b_words_path.canonicalize()?.into()), config.words_path);
        Ok(())
    }
}
