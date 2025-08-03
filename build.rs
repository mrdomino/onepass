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

    let file = File::open("data/eff_large_wordlist.txt")?;
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
        "// Generated at build time from data/eff_large_wordlist.txt"
    )?;
    writeln!(output, "pub const EFF_WORDLIST: &[&str] = &[")?;
    for word in words {
        writeln!(output, "    \"{word}\",")?;
    }
    writeln!(output, "];")?;

    println!("cargo:rerun-if-changed=data/eff_large_wordlist.txt");

    // Embed Info.plist on macOS with macos-biometry feature
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let macos_biometry = env::var("CARGO_FEATURE_MACOS_BIOMETRY").is_ok();
    if target_os == "macos" && macos_biometry {
        println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,data/Info.plist");
        println!("cargo:rerun-if-changed=data/Info.plist");
    }

    Ok(())
}
