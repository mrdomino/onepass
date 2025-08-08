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

#[cfg(not(any(
    feature = "keyring",
    all(target_os = "macos", feature = "macos-biometry")
)))]
compile_error!("either \"keyring\" or \"macos-biometry\" must be enabled");

use anyhow::{Context, Result};

#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
pub(super) use crate::macos_keychain::{Entry, Error};
#[cfg(not(all(target_os = "macos", feature = "macos-biometry")))]
pub(super) use keyring::{Entry, Error};

const SERVICE: &str = "onepass.app.whilezero.org";
const ACCOUNT: &str = "seed";

pub(super) fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
