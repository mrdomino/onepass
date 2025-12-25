use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, Error, Write},
    path::PathBuf,
};

use onepass_base::dict::{BoxDict, Dict};

fn main() {
    println!("cargo:rerun-if-changed=data/eff_large_wordlist.txt");
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let mut dest_path = PathBuf::from(&out_dir);
    dest_path.push("wordlist.rs");
    let f = File::open("data/eff_large_wordlist.txt").unwrap();
    let reader = BufReader::new(f);
    let words = reader
        .lines()
        .try_fold(Vec::new(), |mut words, line| {
            let line = line?;
            if line.is_empty() {
                return Err(Error::other("empty line"));
            }
            let word = line
                .split('\t')
                .nth(1)
                .ok_or_else(|| Error::other("malformed word list"))?;
            words.push(word.to_string());
            Ok(words)
        })
        .unwrap();
    let dict = BoxDict::from_iter(words.iter().map(AsRef::as_ref));
    let mut f = File::create(dest_path).unwrap();
    writeln!(
        f,
        "// Generated at build time from data/eff_large_wordlist.txt"
    )
    .unwrap();
    writeln!(f, "static EFF_WORDLIST_HASH: [u8; 32] = [").unwrap();
    for &b in dict.hash() {
        writeln!(f, "    0x{:02x},", b).unwrap();
    }
    writeln!(f, "];").unwrap();
    writeln!(
        f,
        "static EFF_WORDLIST_WORDS: [&str; {}] = [",
        dict.words().len()
    )
    .unwrap();
    for &word in dict.words() {
        writeln!(f, "    {word:?},").unwrap();
    }
    writeln!(f, "];").unwrap();
}
