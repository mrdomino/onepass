mod seed_password;

use std::{
    fs,
    io::{IsTerminal, Write, stdout},
    num::NonZero,
    path::Path,
    sync::Arc,
};

use anyhow::{Context as _Context, Result};
use clap::{CommandFactory, Parser, error::ErrorKind};
use onepass_conf::{Config, Error, RawSite};
use onepass_seed::{
    dict::{BoxDict, Dict},
    expr::{Context, Eval},
};
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
    let config_path = args.config_path.as_deref();
    let config = Config::from_or_init(config_path).context("failed to read config")?;
    let use_keyring = args.keyring.or(config.global.use_keyring).unwrap_or(false);

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
    let dict = words
        .as_deref()
        .map(BoxDict::from_lines)
        .map(|d| -> Arc<dyn Dict + '_> { Arc::new(d) });

    let mut stdout = stdout();
    let context = dict.map_or_else(Context::default, Context::with_dict);
    for site in &args.sites {
        let res = gen_password_config(&seed, site, &config, &args, &context)?;
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
    let path = args
        .words_path
        .as_deref()
        .or(config.global.words_path.as_deref());
    path.map(|p| fs::read_to_string(p).map(|s| s.into_boxed_str()))
        .transpose()
        .context("failed reading words file")
}

fn gen_password_config(
    seed: &str,
    url: &str,
    config: &Config,
    args: &Args,
    context: &Context<'_>,
) -> Result<Zeroizing<String>> {
    let username = args.username.as_deref();
    let mut site = match config.find_site(url, username) {
        Ok(site) => site,
        Err(Error::UrlNotFound) => RawSite::new(url, username, None, 0),
        Err(err) => return Err(err).context("failed finding site"),
    };
    if args.schema.is_some() {
        site.schema = args.schema.as_deref();
    }
    if let Some(increment) = args.increment {
        site.increment = NonZero::new(increment);
    }
    // TODO(soon): do something about redundant default_schema call here
    let site = site.to_site_with_context(config.default_schema(), context)?;
    let size = site.expr.size();
    let salt = format!("{site}");

    // TODO(soon): mode that only describes passwords and shows entropy.
    if args.verbose {
        eprintln!(
            "schema for {2} has about {0} bits of entropy (0x{1} possible passwords)",
            &size.bits(),
            &size.to_string().trim_start_matches('0'),
            url,
        );
        eprintln!("salt: {salt:?}");
    }

    site.password(seed).context("failed generating password")
}

#[cfg(test)]
mod tests {
    use std::{fs::File, path::PathBuf};

    use onepass_seed::{
        dict::{BoxDict, Dict},
        site::Site,
    };
    use tempfile::TempDir;

    use super::*;

    #[test]
    #[ignore] // too slow
    fn test_passwords() -> Result<()> {
        let tests = [
            (
                "impeach-duckling-outage-spur",
                "arst",
                "google.com",
                "{words:4:-}",
                0,
            ),
            ("(%\")&#+(&!/$", "password", "apple.com", "[!-/]{12}", 1),
        ];
        for (want, seed, url, schema, increment) in tests {
            let site = Site::new(url, None, schema, increment)?;
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
        writeln!(config_file, "[global]")?;
        writeln!(config_file, "words_path = {words_path:?}")?;
        drop(config_file);

        let mut words_file = File::create(&words_path)?;
        writeln!(words_file, "bob")?;
        writeln!(words_file, "  dole")?;
        writeln!(words_file, "bob  ")?;
        writeln!(words_file, "a")?;
        drop(words_file);

        let config = Config::from_file(&config_path)?;
        let args: [&str; 0] = [];
        let words = read_words_str(&Args::parse_from(args.iter()), &config)?
            .context("failed reading words file")?;
        let dict = BoxDict::from_lines(&words);
        let words = dict.words();
        assert_eq!(&["a", "bob", "dole"], words);

        Ok(())
    }
}
