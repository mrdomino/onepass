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
#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
mod macos_keychain;
mod randexp;
mod seed_password;
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
use randexp::{Enumerable, Expr, Quantifiable, Words};
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

    /// Use the system keyring to store the seed password
    #[arg(
        short,
        long,
        env = "ONEPASS_USE_KEYRING",
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
    )]
    keyring: Option<bool>,

    /// Do not use the system keyring to store the seed password
    #[arg(short = 'K', long, conflicts_with = "keyring")]
    no_keyring: bool,

    /// Explicitly reset system keyring seed password
    #[arg(short, long)]
    reset_keyring: bool,

    /// Learning mode: retype the password to memorize it
    #[arg(
        short,
        long,
        value_name = "COUNT",
        default_missing_value = "1",
        num_args=0..=1,
        require_equals = true,
    )]
    learn: Option<u32>,

    /// Confirm seed password
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
    let use_keyring = args.keyring.or(config.use_keyring).unwrap_or(false);

    if args.reset_keyring {
        seed_password::delete()?;
    }
    if args.sites.is_empty() {
        if args.confirm {
            let _ = seed_password::read(use_keyring, true)?;
        }
        if args.reset_keyring || args.confirm {
            return Ok(());
        }
        eprintln!("Specify at least one site\n");
        eprintln!("{}", Args::command().render_help());
        exit(1);
    }
    let seed = seed_password::read(use_keyring, args.confirm)?;

    // The following construction gives us only two allocations: one to store the string data of
    // the user word file and another to store the list of slices corresponding to the words. We
    // need both declarations to be around so we can refer to the latter slice list as a &[&str]
    // so it fits into the same type as the built-in EFF word list.
    let words: Option<_> = read_words_str(&args, &config)?;
    let words = words.as_deref().map(words_arr);
    let words = Words::from(words.as_deref().unwrap_or(EFF_WORDLIST));

    let mut stdout = stdout();
    for site in &args.sites {
        let res = gen_password_config(&seed, site, &config, &args, &words)?;
        stdout.write_all(res.as_bytes())?;
        if stdout.is_terminal() || args.sites.len() > 1 {
            writeln!(stdout)?;
        }
        if let Some(count) = args.learn {
            for _ in 0..count {
                seed_password::check_confirm(&res)?;
            }
        }
    }
    Ok(())
}

fn read_words_str(args: &Args, config: &Config) -> Result<Option<Box<str>>> {
    let path = args.words_path.as_deref();
    let config_path = if path.is_none() {
        config.words_path()
    } else {
        None
    };
    let path = path.or(config_path.as_deref());
    path.map(|p| read_to_string(p).map(|s| s.into_boxed_str()))
        .transpose()
        .context("failed reading words file")
}

fn words_arr(words: &'_ str) -> Box<[&'_ str]> {
    let set = words
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<BTreeSet<_>>();
    set.into_iter().collect()
}

fn gen_password_config(
    seed: &str,
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

    gen_password(seed, &url, &expr, increment, words)
}

fn gen_password(
    seed: &str,
    url: &str,
    expr: &Expr,
    increment: u32,
    words: &Words,
) -> Result<Zeroizing<String>> {
    let size = words.size(expr);
    let salt = format!("{increment},{url}");
    let mut rng = Rng::from_password_salt(seed, salt)?;
    let index = U256::random_mod(&mut rng, &NonZero::new(size).unwrap());
    words.gen_at(expr, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_passwords() -> Result<()> {
        let tests = [
            (
                "pointing-unshaven-asparagus-geography",
                "arst",
                "google.com",
                "[:word:](-[:word:]){3}",
                0,
            ),
            ("!#()/!!%#&!%", "password", "apple.com", "[!-/]{12}", 1),
        ];
        let words = Words(EFF_WORDLIST);
        for (want, seed, url, schema, increment) in tests {
            let url = canonicalize(url, None)?;
            let expr = Expr::parse(schema)?;
            let got = gen_password(seed, &url, &expr, increment, &words)?;
            assert_eq!(want, *got);
        }
        Ok(())
    }
}
