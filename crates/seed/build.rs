use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, Error, Write},
    path::PathBuf,
};

use onepass_base::dict::{BoxDict, Dict};

fn main() {
    println!("cargo:rerun-if-changed=data/eff_large_wordlist.txt");
    let f = File::open("data/eff_large_wordlist.txt").unwrap();
    let reader = BufReader::new(f);
    let words = reader
        .lines()
        .map(|line| -> Result<Option<_>, Error> { Ok(line?.split('\t').nth(1).map(Box::from)) })
        .filter_map(Result::transpose)
        .collect::<Result<Box<[_]>, _>>()
        .unwrap();
    let dict = BoxDict::from_iter(words.iter().map(AsRef::as_ref));

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let mut dest_path = PathBuf::from(&out_dir);
    dest_path.push("wordlist.rs");
    let mut f = File::create(dest_path).unwrap();
    writeln!(
        f,
        "// Generated at build time from data/eff_large_wordlist.txt"
    )
    .unwrap();

    let hash = dict.hash();
    writeln!(f, "const EFF_WORDLIST_HASH: [u8; {}] = [", hash.len()).unwrap();
    for &b in hash {
        writeln!(f, "    0x{b:02x},").unwrap();
    }
    writeln!(f, "];").unwrap();

    let words = dict.words();
    writeln!(f, "static EFF_WORDLIST_WORDS: [&str; {}] = [", words.len()).unwrap();
    for &word in words {
        writeln!(f, "    {word:?},").unwrap();
    }
    writeln!(f, "];").unwrap();
}
