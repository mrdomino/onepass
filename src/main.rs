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

mod randexp;

use std::{
    collections::HashMap,
    env::{self},
    fs::{self, File},
    io::{BufRead, BufReader, IsTerminal, Write, stdout},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use argon2::Argon2;
use blake3::OutputReader;
use clap::Parser;
use crypto_bigint::{NonZero, RandomMod, U256};
use rand_core::RngCore;
use randexp::{
    Expr, WordList,
    quantifiable::{Enumerable, Quantifiable},
};
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

fn default_schema() -> String {
    "[A-Za-z0-9]{16}".into()
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default = "default_schema")]
    pub default_schema: String,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    pub sites: Vec<Site>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Site {
    pub name: String,
    pub schema: String,
    #[serde(default)]
    pub increment: u32,
}

impl Default for Config {
    fn default() -> Self {
        let mut aliases = HashMap::new();
        aliases.insert("strong".to_string(), "[A-Za-z0-9]{18}".to_string());
        aliases.insert(
            "apple".to_string(),
            "[:Word:](-[:word:]){3}[0-9!-/]".to_string(),
        );
        aliases.insert("mobile".to_string(), "[a-z0-9]{16}".to_string());
        aliases.insert("phrase".to_string(), "[:word:](-[:word:]){4}".to_string());
        aliases.insert("pin".to_string(), "[0-9]{8}".to_string());
        let sites = vec![
            Site {
                name: "apple.com".to_string(),
                schema: "apple".to_string(),
                increment: 0,
            },
            Site {
                name: "google.com".to_string(),
                schema: "strong".to_string(),
                increment: 0,
            },
            Site {
                name: "iphone.local".to_string(),
                schema: "pin".to_string(),
                increment: 0,
            },
        ];
        let default_schema = "[A-Za-z0-9_-]{16}".to_string();
        Config {
            default_schema,
            aliases,
            sites,
        }
    }
}

impl Config {
    // TODO: toml, figure out how to not emit 0 increments
    pub fn from_file(path: &Path) -> Result<Self> {
        let mut config = if path.exists() {
            serde_yaml::from_str(&fs::read_to_string(path)?)?
        } else {
            fs::create_dir_all(path.parent().context("invalid file path")?)?;
            let default_config = Config::default();
            fs::write(path, serde_yaml::to_string(&default_config)?)?;
            default_config
        };
        if let Some(schema) = config.aliases.get(&config.default_schema) {
            config.default_schema = schema.clone();
        }
        config.sites = config
            .sites
            .into_iter()
            .map(|site| {
                if let Some(schema) = config.aliases.get(&site.schema) {
                    Site {
                        schema: schema.clone(),
                        ..site
                    }
                } else {
                    site
                }
            })
            .collect();
        Ok(config)
    }
}

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// The site for which to generate a password
    site: String,

    /// Override the path of the config file (default: ~/.config/onepass/config.yaml)
    #[arg(short, long, env = "PASSGEN_CONFIG_FILE")]
    config: Option<String>,

    /// Read words from the specified newline-separated dictionary file (by default, uses words
    /// from the EFF large word list)
    #[arg(short, long, env = "PASSGEN_WORDS_FILE")]
    words: Option<String>,

    /// Print verbose password entropy output
    #[arg(short, long)]
    verbose: bool,

    /// Override schema to use for this site (may be a configured alias)
    #[arg(short, long)]
    schema: Option<String>,
}

include!(concat!(env!("OUT_DIR"), "/wordlist.rs"));

struct Blake3Rng(OutputReader);
impl RngCore for Blake3Rng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0u8; 4];
        self.0.fill(&mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.0.fill(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill(dst);
    }

    fn try_fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) -> std::result::Result<(), crypto_bigint::rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}

fn default_config_path() -> Result<Box<Path>> {
    let mut config_dir = match env::var("XDG_CONFIG_DIR") {
        Err(env::VarError::NotPresent) => {
            env::var("HOME").map(|home| PathBuf::from(home).join(".config"))
        }
        r => r.map(|config| config.into()),
    }
    .context("failed finding config dir")?;
    config_dir.push("onepass");
    config_dir.push("config.yaml");
    Ok(config_dir.into_boxed_path())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config_path = args
        .config
        .as_ref()
        .map(|s| -> Result<Box<Path>> { Ok(PathBuf::from(s).into()) })
        .unwrap_or_else(default_config_path)?;
    let config = Config::from_file(&config_path).unwrap_or_default();

    let words = args
        .words
        .as_ref()
        .map(|s| -> Result<Vec<String>> {
            let path = PathBuf::from(s).into_boxed_path();
            let file = File::open(path).context("open failed")?;
            let reader = BufReader::new(file);
            let mut words = Vec::new();
            for line in reader.lines() {
                words.push(String::from(line?.trim()));
            }
            Ok(words)
        })
        .transpose()
        .context("failed reading word list")?;

    if let Some(words) = &words {
        lookup_site(&config, &args, words)
    } else {
        lookup_site(&config, &args, EFF_WORDLIST)
    }
}

fn lookup_site<T: AsRef<str>>(config: &Config, args: &Args, words: &[T]) -> Result<()> {
    let site = config.sites.iter().find(|&site| site.name == args.site);
    let schema = args
        .schema
        .as_ref()
        .map(|schema| config.aliases.get(schema).unwrap_or(schema))
        .unwrap_or_else(|| {
            site.map(|site| &site.schema)
                .unwrap_or(&config.default_schema)
        });
    let increment = site.map(|site| site.increment).unwrap_or(0);
    let expr = Expr::parse(schema).context("invalid schema")?;
    let wl = WordList(words);
    let sz = wl.size(&expr);

    if args.verbose {
        eprintln!(
            "schema has about {0} bits of entropy ({1} possible passwords)",
            &sz.bits(),
            &sz.to_string().trim_start_matches('0')
        );
    }

    let password: Zeroizing<String> = prompt_password("Master password: ")
        .context("failed reading password")?
        .into();
    let salt = format!("{0}:{1}", increment, &args.site);
    let mut key_material = Zeroizing::new([0u8; 32]);
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2d,
        argon2::Version::V0x13,
        argon2::Params::default(),
    );
    argon2
        .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut *key_material)
        .map_err(|e| anyhow::anyhow!("argon2 failed: {e}"))?;

    let mut hasher = Zeroizing::new(blake3::Hasher::new());
    hasher.update(&*key_material);
    let mut rng = Blake3Rng(hasher.finalize_xof());
    let index = U256::random_mod(&mut rng, &NonZero::new(sz).unwrap());
    let res = wl.gen_at(&expr, index)?;
    let mut stdout = stdout();
    stdout.write_all(res.as_bytes())?;
    if stdout.is_terminal() {
        writeln!(stdout)?;
    }
    Ok(())
}
