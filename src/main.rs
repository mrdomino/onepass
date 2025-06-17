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

mod config;
mod randexp;
mod url;

use std::{
    fs::read_to_string,
    io::{IsTerminal, Write, stdout},
    path::Path,
};

use anyhow::{Context, Result};
use argon2::Argon2;
use blake3::OutputReader;
use clap::Parser;
use config::Config;
use crypto_bigint::{NonZero, RandomMod, U256};
use rand_core::RngCore;
use randexp::{Enumerable, Expr, Quantifiable, Words};
use rpassword::prompt_password;
use url::canonicalize;
use zeroize::Zeroizing;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// The site for which to generate a password
    site: String,

    /// Override the path of the config file (default: ~/.config/onepass/config.yaml)
    #[arg(
        short = 'f',
        long = "config",
        env = "ONEPASS_CONFIG_FILE",
        value_name = "CONFIG_FILE"
    )]
    config_path: Option<Box<Path>>,

    /// Read words from the specified newline-separated dictionary file (by default, uses words
    /// from the EFF large word list)
    #[arg(
        short,
        long = "words",
        env = "ONEPASS_WORDS_FILE",
        value_name = "WORDS_FILE"
    )]
    words_path: Option<Box<Path>>,

    /// Override schema to use for this site (may be a configured alias)
    #[arg(short, long)]
    schema: Option<String>,

    /// Override increment to use for this site
    #[arg(short, long, value_name = "NUM")]
    increment: Option<u32>,

    /// Override username to use for this site
    #[arg(short, long)]
    username: Option<String>,

    /// Confirm master password
    #[arg(short, long)]
    confirm: bool,

    /// Print verbose password entropy output
    #[arg(short, long)]
    verbose: bool,
}

include!(concat!(env!("OUT_DIR"), "/wordlist.rs"));

struct Blake3Rng(Zeroizing<OutputReader>);
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

fn main() -> Result<()> {
    let args = Args::parse();

    let config = Config::from_file(args.config_path.as_deref()).context("failed to read config")?;

    let words: Option<Box<str>> = args
        .words_path
        .or_else(|| config.words_path())
        .map(|path| read_to_string(path).map(|s| s.into()))
        .transpose()
        .context("failed reading words file")?;
    let words: Option<Box<[&str]>> = words
        .as_ref()
        .map(|words| words.lines().map(|line| line.trim()).collect());
    let words = Words::from(words.as_deref().unwrap_or(EFF_WORDLIST));

    let site = config.find_site(&args.site)?;
    let url = site.as_ref().map_or(&args.site, |(url, _)| url);
    let url = canonicalize(
        url,
        args.username.as_deref().or_else(|| {
            site.as_ref()
                .map(|(_, site)| site.username.as_deref())
                .flatten()
        }),
    )?;
    let schema = args.schema.as_ref().map_or_else(
        || {
            site.as_ref()
                .map_or(&config.default_schema, |(_, site)| &site.schema)
        },
        |schema| config.aliases.get(schema).unwrap_or(schema),
    );
    let increment = args
        .increment
        .unwrap_or_else(|| site.map_or(0, |(_, site)| site.increment));
    let expr = Expr::parse(schema).context("invalid schema")?;
    let size = words.size(&expr);

    let salt = format!("{0},{1}", increment, &url);

    if args.verbose {
        eprintln!(
            "schema has about {0} bits of entropy (0x{1} possible passwords)",
            &size.bits(),
            &size.to_string().trim_start_matches('0')
        );
        eprintln!("salt: {salt:?}");
    }

    let password: Zeroizing<String> = prompt_password("Master password: ")
        .context("failed reading password")?
        .into();
    if args.confirm {
        let confirmed: Zeroizing<String> = prompt_password("Confirm: ")
            .context("failed reading confirmation")?
            .into();
        if *confirmed != *password {
            anyhow::bail!("Passwords don’t match");
        }
    }
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
    let mut rng = Blake3Rng(Zeroizing::new(hasher.finalize_xof()));
    let index = U256::random_mod(&mut rng, &NonZero::new(size).unwrap());
    let res = words.gen_at(&expr, index)?;
    let mut stdout = stdout();
    stdout.write_all(res.as_bytes())?;
    if stdout.is_terminal() {
        writeln!(stdout)?;
    }
    Ok(())
}
