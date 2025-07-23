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

#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
use crate::macos_keychain::{Entry, Error};
use anyhow::{Context, Result};
#[cfg(not(all(target_os = "macos", feature = "macos-biometry")))]
use keyring::{Entry, Error};
use rpassword::prompt_password;
use zeroize::Zeroizing;

const SERVICE: &str = "org.whilezero.app.onepass";
const ACCOUNT: &str = "seed";

/// read reads the seed password from either the system keyring or the console.
///
/// If `confirm` is true, then the password is checked against a confirmation that is always read
/// from the console. This allows the user to confirm that the seed password is what they think it
/// is without otherwise exposing the password.
pub(crate) fn read(use_keyring: bool, confirm: bool) -> Result<Zeroizing<String>> {
    let password = use_keyring.then(load_keyring).transpose()?.flatten();
    if let Some(password) = password {
        if confirm {
            check_confirm(&password)?;
        }
        return Ok(password);
    }
    let password: Zeroizing<String> = prompt_password("Seed password: ")
        .context("failed reading password")?
        .into();
    if use_keyring || confirm {
        check_confirm(&password)?;
    }
    if use_keyring {
        save_keyring(&password)?;
    }
    Ok(password)
}

/// delete clears the password from the system keyring. It succeeds if the password was either
/// deleted or not set.
pub(crate) fn delete() -> Result<()> {
    match get_entry()?.delete_credential() {
        Err(Error::NoEntry) => (),
        r => r.context("failed deleting password")?,
    };
    Ok(())
}

pub(crate) fn check_confirm(password: &str) -> Result<()> {
    let confirmed: Zeroizing<String> = prompt_password("Confirmation: ")
        .context("failed reading confirmation")?
        .into();
    if *confirmed != password {
        anyhow::bail!("passwords don’t match");
    }
    Ok(())
}

fn load_keyring() -> Result<Option<Zeroizing<String>>> {
    match get_entry()?.get_password() {
        Err(Error::NoEntry) => Ok(None),
        r => Ok(Some(r?.into())),
    }
}

fn save_keyring(password: &str) -> Result<()> {
    get_entry()?
        .set_password(password)
        .context("failed setting password")
}

fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
