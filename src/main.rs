mod randexp;

use std::{
    collections::{HashMap, hash_map::Entry},
    fs,
    io::{Write, stdout},
};

use anyhow::{Context, Result};
use argon2::Argon2;
use blake3::OutputReader;
use clap::Parser;
use crypto_bigint::{NonZero, RandomMod, U256, rand_core::RngCore};
use randexp::Expr;
use rpassword::read_password;
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Debug, Deserialize)]
struct DiskConfig {
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    pub sites: Vec<Site>,
}

#[derive(Debug, Deserialize)]
struct Site {
    pub name: String,
    pub schema: String,
}

#[derive(Default)]
struct Config {
    sites: HashMap<String, String>,
}

impl Config {
    pub fn from_file<T: AsRef<str>>(path: T) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())?;
        let disk_config: DiskConfig = serde_yaml::from_str(&content)?;
        let mut sites: HashMap<String, String> = HashMap::new();
        for site in disk_config.sites.into_iter() {
            match sites.entry(site.name) {
                Entry::Vacant(entry) => {
                    entry.insert(
                        disk_config
                            .aliases
                            .get(&site.schema)
                            .unwrap_or(&site.schema)
                            .clone(),
                    );
                }
                Entry::Occupied(entry) => anyhow::bail!("duplicate site {0}", entry.key()),
            }
        }
        Ok(Config { sites })
    }
}

#[derive(Debug, Parser)]
struct Args {
    site: String,

    #[arg(short, long, default_value = "~/.config/passgen/config.yaml")]
    config: String,

    #[arg(long, default_value = "example salt")]
    salt: String,

    #[arg(short, long)]
    schema: Option<String>,

    #[arg(short, long, default_value = "[a-z0-9]{16}")]
    default_schema: String,
}

const WORDS: &[&str] = &["bob", "dole"];

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
    let config = Config::from_file(&config_file).unwrap_or_default();
    let schema = &args.schema.unwrap_or_else(|| {
        config
            .sites
            .get(&args.site)
            .unwrap_or(&args.default_schema)
            .into()
    });
    let expr = Expr::parse(schema).context("invalid schema")?;
    let sz = expr.size(WORDS.len() as u32);
    eprintln!(
        "schema has about {0} bits of entropy ({1} possible passwords)",
        &sz.bits(),
        &sz.to_string().trim_start_matches('0')
    );
    let password: Zeroizing<String> = read_password().context("failed reading password")?.into();
    let mut key_material = Zeroizing::new([0u8; 32]);
    Argon2::default()
        .hash_password_into(
            password.as_bytes(),
            args.salt.as_bytes(),
            &mut *key_material,
        )
        .map_err(|e| anyhow::anyhow!("argon2 failed: {e}"))?;
    let mut hasher = Zeroizing::new(blake3::Hasher::new());
    hasher.update(&*key_material);
    hasher.update(args.site.as_bytes());
    let mut rng = Blake3Rng(hasher.finalize_xof());
    let index = U256::random_mod(&mut rng, &NonZero::new(sz).unwrap());
    let res: Zeroizing<String> = Zeroizing::new(expr.gen_at_index(WORDS, index)?);
    let mut stdout = stdout();
    stdout.write_all(res.as_bytes())?;
    stdout.write_all(b"\n")?;
    Ok(())
}
