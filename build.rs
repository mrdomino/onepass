use core::error;
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
    result,
};

type Error = Box<dyn error::Error>;
type Result<T> = result::Result<T, Error>;

fn main() -> Result<()> {
    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("wordlist.rs");

    let file = File::open("eff_large_wordlist.txt")?;
    let reader = BufReader::new(file);

    let mut words = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        words.push(line.split('\t').nth(1).ok_or("parse failure")?.to_string());
    }

    let mut output = File::create(&dest_path)?;
    writeln!(
        output,
        "// Generated at build time from eff_large_wordlist.txt"
    )?;
    writeln!(output, "pub const EFF_WORDLIST: &[&str] = &[")?;
    for word in words {
        writeln!(output, "    \"{}\",", word)?;
    }
    writeln!(output, "];")?;

    println!("cargo:rerun-if-changed=eff_large_wordlist.txt");
    Ok(())
}
