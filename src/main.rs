mod randexp;

use std::{
    collections::HashMap,
    fs,
    hash::Hash,
    io::{IsTerminal, Write, stdout},
    path::Path,
};

use anyhow::{Context, Result};
use argon2::Argon2;
use blake3::OutputReader;
use clap::Parser;
use crypto_bigint::{NonZero, RandomMod, U256};
use rand_core::RngCore;
use randexp::Expr;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

fn default_schema() -> String {
    "[A-Za-z0-9]{16}".into()
}

fn default_salt() -> String {
    "insecure salt".into()
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default = "default_schema")]
    pub default_schema: String,
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    pub sites: Vec<Site>,
    #[serde(default = "default_salt")]
    pub salt: String,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
struct Site {
    pub name: String,
    pub schema: String,
    #[serde(default)]
    pub increment: u32,
}

impl Hash for Site {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state)
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut aliases = HashMap::new();
        aliases.insert("strong".to_string(), "[A-Za-z0-9]{18}".to_string());
        aliases.insert(
            "apple".to_string(),
            "[A-Z][:word:](-[:word:]){4}[!-/]".to_string(),
        );
        aliases.insert("mobile".to_string(), "[a-z0-9]{16}".to_string());
        aliases.insert("phrase".to_string(), "[:word:](-[:word:]){4}".to_string());
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
        ];
        let default_schema = "[A-Za-z0-9_-]{16}".to_string();
        let salt = "insecure salt".to_string();
        Config {
            default_schema,
            aliases,
            sites,
            salt,
        }
    }
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self> {
        let mut config = if path.exists() {
            serde_yaml::from_str(&fs::read_to_string(path)?)?
        } else {
            fs::create_dir_all(path.parent().context("invalid file path")?)?;
            let default_config = Config::default();
            fs::write(path, serde_yaml::to_string(&default_config)?)?;
            default_config
        };
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
struct Args {
    site: String,

    #[arg(short, long, default_value = "~/.config/passgen/config.yaml")]
    config: String,

    #[arg(short, long)]
    verbose: bool,
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

fn main() -> Result<()> {
    let args = Args::parse();
    let config_file = shellexpand::tilde(&args.config);
    let config = Config::from_file(Path::new(config_file.as_ref())).unwrap_or_default();
    let site = config.sites.iter().find(|site| site.name == args.site);
    let schema = site
        .map(|site| &site.schema)
        .unwrap_or(&config.default_schema);
    let increment = site.map(|site| site.increment).unwrap_or(0);
    let expr = Expr::parse(schema).context("invalid schema")?;
    let sz = expr.size(EFF_WORDLIST.len() as u32);
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
    let mut buf: Zeroizing<Vec<u8>> =
        Vec::with_capacity(password.len() + 4 + args.site.len()).into();
    buf.extend(password.as_bytes());
    buf.extend(increment.to_le_bytes());
    buf.extend(args.site.as_bytes());
    let mut key_material = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(&buf, config.salt.as_bytes(), &mut *key_material)
        .map_err(|e| anyhow::anyhow!("argon2 failed: {e}"))?;
    let mut hasher = Zeroizing::new(blake3::Hasher::new());
    hasher.update(&*key_material);
    let mut rng = Blake3Rng(hasher.finalize_xof());
    let index = U256::random_mod(&mut rng, &NonZero::new(sz).unwrap());
    let res: Zeroizing<String> = Zeroizing::new(expr.gen_at_index(EFF_WORDLIST, index)?);
    let mut stdout = stdout();
    stdout.write_all(res.as_bytes())?;
    if stdout.is_terminal() {
        writeln!(stdout)?;
    }
    Ok(())
}
