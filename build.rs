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

use std::{
    env,
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader, Write},
    path::Path,
};

fn main() {
    gen_eff_wordlist();
    embed_info_plist();
}

fn gen_eff_wordlist() {
    println!("cargo:rerun-if-changed=data/eff_large_wordlist.txt");
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("wordlist.rs");
    let file = File::open("data/eff_large_wordlist.txt").unwrap();
    let reader = BufReader::new(file);
    let mut words = Vec::new();
    for line in reader.lines() {
        let line = line.unwrap();
        let line = line.split('\t').nth(1).unwrap().trim();
        if line.is_empty() {
            continue;
        }
        words.push(line.to_string());
    }

    let mut output = File::create(&dest_path).unwrap();
    writeln!(
        output,
        "// Generated at build time from data/eff_large_wordlist.txt"
    )
    .unwrap();
    writeln!(output, "pub const EFF_WORDLIST: &[&str] = &[").unwrap();
    for word in words {
        writeln!(output, "    \"{word}\",").unwrap();
    }
    writeln!(output, "];").unwrap();
}

fn embed_info_plist() {
    // If we are using the biometric keychain API, we must embed Info.plist for the app to work.
    if env::var_os("CARGO_FEATURE_MACOS_BIOMETRY").is_none()
        || env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| OsStr::new("macos") != os)
    {
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,data/Info.plist");
    println!("cargo:rerun-if-changed=data/Info.plist");
}
