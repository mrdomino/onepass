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
mod crypto;
mod keyring;
#[cfg(target_os = "macos")]
mod macos_keychain;
mod randexp;
mod url;

use std::{
    collections::BTreeSet,
    fs::read_to_string,
    io::{IsTerminal, Write, stdout},
    path::Path,
    process::exit,
};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use config::Config;
use crypto::Rng;
use crypto_bigint::{NonZero, RandomMod, U256};
use keyring::delete_password;
use randexp::{Enumerable, Expr, Quantifiable, Words};
use rpassword::prompt_password;
use url::canonicalize;
use zeroize::Zeroizing;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Site(s) for which to generate a password
    #[arg(value_name = "SITE")]
    sites: Vec<String>,

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

    /// Override schema to use (may be a configured alias)
    #[arg(short, long)]
    schema: Option<String>,

    /// Override increment to use
    #[arg(short, long, value_name = "NUM")]
    increment: Option<u32>,

    /// Override username to use
    #[arg(short, long)]
    username: Option<String>,

    /// Use the system keyring to store the master password
    #[arg(
        short,
        long,
        env = "ONEPASS_USE_KEYRING",
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
    )]
    keyring: Option<bool>,

    /// Do not use the system keyring to store the master password
    #[arg(short = 'K', long, conflicts_with = "keyring")]
    no_keyring: bool,

    /// Explicitly reset system keyring master password
    #[arg(short, long)]
    reset_keyring: bool,

    /// Confirm master password
    #[arg(short, long)]
    confirm: bool,

    /// Print verbose password entropy output
    #[arg(short, long)]
    verbose: bool,
}

include!(concat!(env!("OUT_DIR"), "/wordlist.rs"));

fn main() -> Result<()> {
    let mut args = Args::parse();
    if args.no_keyring {
        args.keyring = Some(false);
    }

    let config = Config::from_file(args.config_path.as_deref()).context("failed to read config")?;

    let words: Option<Box<str>> = args
        .words_path
        .clone()
        .or_else(|| config.words_path())
        .map(|path| read_to_string(path).map(|s| s.into()))
        .transpose()
        .context("failed reading words file")?;
    let words: Option<Box<[&str]>> = words
        .as_deref()
        .map(|words| {
            words
                .lines()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .collect::<BTreeSet<_>>()
        })
        .map(|words| words.into_iter().collect());
    let words = Words::from(words.as_deref().unwrap_or(EFF_WORDLIST));

    if args.reset_keyring {
        delete_password()?;
        if args.sites.is_empty() {
            return Ok(());
        }
    }

    if args.sites.is_empty() {
        eprintln!("Specify at least one site\n");
        eprintln!("{}", Args::command().render_help());
        exit(1);
    }

    let use_keyring = args.keyring.or(config.use_keyring).unwrap_or(false);
    let password = read_password(use_keyring, args.confirm)?;

    let mut stdout = stdout();
    for site in &args.sites {
        let res = gen_password(&password, site, &config, &args, &words)?;
        stdout.write_all(res.as_bytes())?;
        if stdout.is_terminal() || args.sites.len() > 1 {
            writeln!(stdout)?;
        }
    }
    Ok(())
}

fn gen_password(
    password: &str,
    req: &str,
    config: &Config,
    args: &Args,
    words: &Words,
) -> Result<Zeroizing<String>> {
    let site = config.find_site(req)?;
    let url = site.as_ref().map_or(req, |(url, _)| url);
    let url = canonicalize(
        url,
        args.username
            .as_deref()
            .or_else(|| site.as_ref().and_then(|(_, site)| site.username.as_deref())),
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
            "schema for {2} has about {0} bits of entropy (0x{1} possible passwords)",
            &size.bits(),
            &size.to_string().trim_start_matches('0'),
            req,
        );
        eprintln!("salt: {salt:?}");
    }

    let mut rng = Rng::from_password_salt(password, salt)?;
    let index = U256::random_mod(&mut rng, &NonZero::new(size).unwrap());
    let res = words.gen_at(&expr, index)?;
    Ok(res)
}

fn read_password(use_keyring: bool, confirm: bool) -> Result<Zeroizing<String>> {
    let password = use_keyring
        .then(keyring::load_password)
        .transpose()?
        .flatten();
    if let Some(password) = password {
        return Ok(password);
    }
    let password: Zeroizing<String> = prompt_password("Seed password: ")
        .context("failed reading password")?
        .into();
    if use_keyring || confirm {
        check_confirm(&password)?;
    }
    if use_keyring {
        keyring::save_password(&password)?;
    }
    Ok(password)
}

fn check_confirm(password: &str) -> Result<()> {
    let confirmed: Zeroizing<String> = prompt_password("Confirmation: ")
        .context("failed reading confirmation")?
        .into();
    if *confirmed != password {
        anyhow::bail!("passwords don’t match");
    }
    Ok(())
}
