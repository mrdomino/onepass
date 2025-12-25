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

use std::{env, ffi::OsStr};

fn main() {
    embed_info_plist();
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
