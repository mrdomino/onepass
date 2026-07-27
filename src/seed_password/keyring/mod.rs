#[cfg(not(keyring = "rs"))]
mod error;
#[cfg(keyring = "macos")]
mod macos_keychain;
#[cfg(keyring = "no")]
mod null_keyring;

use anyhow::{Context, Result};

#[cfg(keyring = "rs")]
pub(super) use keyring::{Entry, Error};

#[cfg(keyring = "macos")]
pub(super) use macos_keychain::Entry;
#[cfg(keyring = "no")]
pub(super) use null_keyring::Entry;

#[cfg(not(keyring = "rs"))]
pub(super) use error::Error;

const SERVICE: &str = "onepass.app.whilezero.org";
const ACCOUNT: &str = "seed";

pub(super) fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
