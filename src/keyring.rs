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

#[cfg(target_os = "macos")]
use crate::macos_keychain::{Entry, Error};
use anyhow::{Context, Result};
#[cfg(not(target_os = "macos"))]
use keyring::{Entry, Error};
use zeroize::Zeroizing;

const SERVICE: &str = "org.whilezero.app.onepass";
const ACCOUNT: &str = "seed";

pub(crate) fn load_password() -> Result<Option<Zeroizing<String>>> {
    match get_entry()?.get_password() {
        Err(Error::NoEntry) => return Ok(None),
        r => Ok(Some(r?.into())),
    }
}

pub(crate) fn save_password(password: &str) -> Result<()> {
    get_entry()?
        .set_password(password)
        .context("failed setting password")
}

pub(crate) fn delete_password() -> Result<()> {
    get_entry()?
        .delete_credential()
        .context("failed deleting password")
}

fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
