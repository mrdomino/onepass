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

use std::{
    collections::HashMap,
    fs::{create_dir_all, read_to_string, write},
    path::Path,
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::url::canonicalize;

pub(crate) struct Config {
    pub default_schema: String,
    pub aliases: HashMap<String, String>,
    pub sites: HashMap<String, SiteConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SiteConfig {
    pub schema: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub increment: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

impl Config {
    pub fn from_file(p: &Path) -> Result<Self> {
        if !p.exists() {
            create_dir_all(p.parent().context("invalid config path")?)?;
            write(p, serde_yaml::to_string(&SerConfig::example())?)?;
        }
        Self::from_str(&read_to_string(p)?)
    }

    pub fn from_str(s: &str) -> Result<Self> {
        let config: SerConfig = serde_yaml::from_str(s)?;
        Ok(Self::from_ser_config(config))
    }

    pub fn find_site(&self, url: &str) -> Result<Option<(String, &SiteConfig)>> {
        let url = canonicalize(url, None)?;
        let Some(site) = self.sites.get(&url) else {
            return Ok(None);
        };
        let url = canonicalize(&url, site.username.as_deref())?;
        Ok(Some((url, site)))
    }

    fn from_ser_config(config: SerConfig) -> Self {
        let default_schema = config.default_schema;
        let aliases = config.aliases;
        let sites = config
            .sites
            .into_iter()
            .map(|(mut site, mut config)| {
                if let Some(schema) = aliases.get(&config.schema) {
                    config.schema = schema.clone();
                }
                // TODO: print warnings on parse errors here
                if let Ok(url) = canonicalize(&site, None) {
                    site = url;
                }
                (site, config)
            })
            .collect();
        Config {
            default_schema,
            aliases,
            sites,
        }
    }
}

#[derive(Debug, Deserialize)]
struct SerConfig {
    #[serde(default = "default_schema")]
    pub default_schema: String,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    #[serde(deserialize_with = "deserialize_sites")]
    pub sites: HashMap<String, SiteConfig>,
}

impl SerConfig {
    fn example() -> Self {
        let aliases: HashMap<String, String> = [
            ("alnum", "[A-Za-z0-9]{18}"),
            ("apple", "[:Word:](-[:word:]){3}[0-9!-/]"),
            ("login", "[!-~]{12}"),
            ("mobile", "[a-z0-9]{24}"),
            ("phrase", "[:word:](-[:word:]){4}"),
            ("pin", "0-9]{8}"),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
        let sites: HashMap<String, SiteConfig> = [
            ("apple.com", ("apple", 0)),
            ("google.com", ("mobile", 0)),
            ("iphone.local", ("pin", 1)),
        ]
        .into_iter()
        .map(|(k, (schema, increment))| {
            (
                k.into(),
                SiteConfig {
                    schema: schema.into(),
                    increment,
                    username: None,
                },
            )
        })
        .collect();
        let default_schema = "login".to_string();
        SerConfig {
            default_schema,
            aliases,
            sites,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum SchemaOrSiteConfig {
    Schema(String),
    Config(SiteConfig),
}

impl From<SchemaOrSiteConfig> for SiteConfig {
    fn from(value: SchemaOrSiteConfig) -> Self {
        match value {
            SchemaOrSiteConfig::Schema(schema) => SiteConfig {
                schema,
                increment: 0,
                username: None,
            },
            SchemaOrSiteConfig::Config(config) => config,
        }
    }
}

impl From<&SiteConfig> for SchemaOrSiteConfig {
    fn from(config: &SiteConfig) -> Self {
        if is_zero(&config.increment) && config.username.is_none() {
            SchemaOrSiteConfig::Schema(config.schema.clone())
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
        state.serialize_field("default_schema", &self.default_schema)?;
        state.serialize_field("aliases", &self.aliases)?;

        let sites_for_serialization: HashMap<String, SchemaOrSiteConfig> = self
            .sites
            .iter()
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();

        state.serialize_field("sites", &sites_for_serialization)?;
        state.end()
    }
}

fn deserialize_sites<'de, D>(deserializer: D) -> Result<HashMap<String, SiteConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let sites: HashMap<String, SchemaOrSiteConfig> = HashMap::deserialize(deserializer)?;
    Ok(sites.into_iter().map(|(k, v)| (k, v.into())).collect())
}

fn default_schema() -> String {
    "[A-Za-z0-9]{16}".into()
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    #[test]
    fn basic() -> Result<()> {
        let config: SerConfig = serde_yaml::from_str("sites:\n google.com: \"[A-Z]\"")?;
        let goog = &config.sites["google.com"];
        assert_eq!("[A-Z]", goog.schema);
        Ok(())
    }

    #[test]
    fn object() -> Result<()> {
        let config: SerConfig =
            serde_yaml::from_str("sites:\n abcd:\n  schema: \"A\"\n  increment: 1\n")?;
        let abcd = &config.sites["abcd"];
        assert_eq!(1, abcd.increment);
        assert_eq!("A", abcd.schema);
        Ok(())
    }

    #[test]
    fn find_site_ok() -> Result<()> {
        let config = Config::from_str(
            r#"
            sites:
                google.com:
                    schema: A
                    username: "test@gmail.com"
                apple.com:
                    schema: B
                "http://localhost":
                    schema: C
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
        ];
        for (want, input) in tests {
            let got = config.find_site(input)?;
            match (want, got) {
                (Some((want_url, want_schema)), Some((got_url, got_config))) => {
                    assert_eq!(want_url, &got_url);
                    assert_eq!(want_schema, got_config.schema);
                }
                (None, None) => (),
                (want, got) => panic!("mismatch: {want:?} / {got:?}"),
            };
        }
        Ok(())
    }

    // TODO: temp config file
}
