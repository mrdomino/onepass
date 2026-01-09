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
#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
mod macos_keychain;
mod seed_password;
mod url;

use std::{
    io::{IsTerminal, Write, stdout},
    path::Path,
};

use anyhow::{Context as _Context, Result};
use clap::{CommandFactory, Parser, error::ErrorKind};
use config::Config;
use onepass_seed::{
    crypto::Derivation,
    data::Site,
    dict::BoxDict,
    expr::{Context, Eval, Expr},
};
use url::canonicalize;
use zeroize::Zeroizing;

#[derive(Debug, Parser)]
#[command(version, about, next_help_heading = "Site Options")]
struct Args {
    /// Site(s) for which to generate a password
    #[arg(value_name = "SITE", help_heading = None)]
    sites: Vec<String>,

    /// Override schema to use (may be a configured alias)
    #[arg(short, long)]
    schema: Option<String>,

    /// Override increment to use
    #[arg(short, long, value_name = "NUM")]
    increment: Option<u32>,

    /// Override username to use
    #[arg(short, long)]
    username: Option<String>,

    /// Store the seed password in the OS keyring
    #[arg(
        short,
        long,
        env = "ONEPASS_USE_KEYRING",
        default_missing_value = "true",
        num_args = 0..=1,
        require_equals = true,
        help_heading = "Keyring Integration",
    )]
    keyring: Option<bool>,

    /// Do not store the seed password
    #[arg(
        short = 'K',
        long,
        conflicts_with = "keyring",
        help_heading = "Keyring Integration"
    )]
    no_keyring: bool,

    /// Clear the seed password keyring entry
    #[arg(short, long, help_heading = "Keyring Integration")]
    reset_keyring: bool,

    /// Confirm seed password
    #[arg(short, long, help_heading = "Password Entry")]
    confirm: bool,

    /// Learn the site password by retyping it
    #[arg(
        short,
        long,
        value_name = "COUNT",
        default_missing_value = "1",
        num_args=0..=1,
        require_equals = true,
        help_heading = "Password Entry",
    )]
    learn: Option<u32>,

    /// Override word list
    #[arg(
        short,
        long = "words",
        env = "ONEPASS_WORDS_FILE",
        value_name = "WORDS_FILE",
        help_heading = "Configuration"
    )]
    words_path: Option<Box<Path>>,

    /// Override config file
    #[arg(
        short = 'f',
        long = "config",
        env = "ONEPASS_CONFIG_FILE",
        value_name = "CONFIG_FILE",
        help_heading = "Configuration"
    )]
    config_path: Option<Box<Path>>,

    /// Print verbose site password entropy output
    #[arg(short, long)]
    verbose: bool,
}

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
        Args::command()
            .error(ErrorKind::TooFewValues, "specify at least one site")
            .exit();
    }
    let seed = seed_password::read(use_keyring, args.confirm)?;

    let words: Option<_> = read_words_str(&args, &config)?;
    let dict = words.as_deref().map(BoxDict::from_lines);

    let mut stdout = stdout();
    for site in &args.sites {
        let context = dict.as_ref().map(|dict| Context::with_dict(dict));
        let context = context.unwrap_or_else(Context::default);
        let res = gen_password_config(&seed, site, &config, &args, context)?;
        stdout.write_all(res.as_bytes())?;
        if stdout.is_terminal() || args.sites.len() > 1 {
            writeln!(stdout)?;
        }
        if let Some(count) = args.learn {
            let mut failures = 0;
            for _ in 0..count {
                if !seed_password::check_confirm(&res)? {
                    failures += 1;
                    eprint!("✘ ");
                }
            }
            if failures != 0 {
                if count == 1 {
                    anyhow::bail!("password mismatch");
                }
                anyhow::bail!("{failures}/{count} attempts failed");
            }
        }
    }
    Ok(())
}

fn read_words_str(args: &Args, config: &Config) -> Result<Option<Box<str>>> {
    use std::fs::read_to_string;

    let path = args.words_path.as_deref().or(config.words_path.as_deref());
    path.map(|p| read_to_string(p).map(|s| s.into_boxed_str()))
        .transpose()
        .context("failed reading words file")
}

fn gen_password_config(
    seed: &str,
    req: &str,
    config: &Config,
    args: &Args,
    context: Context<'_>,
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
            site.as_ref().map_or(config.default_schema(), |(_, site)| {
                config.site_schema(site)
            })
        },
        |schema| config.aliases.get(schema).unwrap_or(schema),
    );
    let increment = args
        .increment
        .unwrap_or_else(|| site.map_or(0, |(_, site)| site.increment));
    let root = schema.parse().context("invalid schema")?;
    let expr = Expr::with_context(root, context);
    let size = expr.size();
    let site = Site {
        url,
        username: None,
        schema: expr,
        increment,
    };
    let salt = format!("{}", Derivation(&site));

    if args.verbose {
        eprintln!(
            "schema for {2} has about {0} bits of entropy (0x{1} possible passwords)",
            &size.bits(),
            &size.to_string().trim_start_matches('0'),
            req,
        );
        eprintln!("salt: {salt:?}");
    }

    site.password(seed).context("failed generating password")
}

#[cfg(test)]
mod tests {
    use std::{fs::File, path::PathBuf};

    use onepass_seed::dict::{BoxDict, Dict};
    use tempfile::TempDir;

    use super::*;

    #[test]
    #[ignore] // too slow
    fn test_passwords() -> Result<()> {
        let tests = [
            (
                "deprive-cargo-richly-sibling",
                "arst",
                "google.com",
                "{words:4:-}",
                0,
            ),
            ("!((-%(')*'\"/", "password", "apple.com", "[!-/]{12}", 1),
        ];
        for (want, seed, url, schema, increment) in tests {
            let url = canonicalize(url, None)?;
            let expr = Expr::new(schema.parse()?);
            let site = Site {
                url,
                username: None,
                schema: expr,
                increment,
            };
            let got = site.password(seed)?;
            assert_eq!(want, *got);
        }
        Ok(())
    }

    #[test]
    fn test_words_file() -> Result<()> {
        let dir = TempDir::new()?;
        let mut dir_path = PathBuf::from(dir.path());
        dir_path.push("config");
        let config_path = dir_path.clone().into_boxed_path();
        dir_path.pop();
        dir_path.push("words");
        let words_path = dir_path.into_boxed_path();

        let mut config_file = File::create(&config_path)?;
        writeln!(config_file, "words_path: words")?;
        writeln!(config_file, "sites:")?;
        drop(config_file);

        let mut words_file = File::create(&words_path)?;
        writeln!(words_file, "bob")?;
        writeln!(words_file, "  dole")?;
        writeln!(words_file, "bob  ")?;
        writeln!(words_file, "a")?;
        drop(words_file);

        let config = Config::from_file(Some(&config_path))?;
        let args: [&str; 0] = [];
        let words = read_words_str(&Args::parse_from(args.iter()), &config)?
            .context("failed reading words file")?;
        let dict = BoxDict::from_lines(&words);
        let words = dict.words();
        assert_eq!(&["a", "bob", "dole"], words);

        Ok(())
    }
}
