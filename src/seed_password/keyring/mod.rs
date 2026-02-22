#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
mod macos_keychain;

#[cfg(not(any(
    feature = "keyring",
    all(target_os = "macos", feature = "macos-biometry")
)))]
compile_error!("either \"keyring\" or \"macos-biometry\" must be enabled");

use anyhow::{Context, Result};

#[cfg(not(all(target_os = "macos", feature = "macos-biometry")))]
pub(super) use keyring::{Entry, Error};
#[cfg(all(target_os = "macos", feature = "macos-biometry"))]
pub(super) use macos_keychain::{Entry, Error};

const SERVICE: &str = "onepass.app.whilezero.org";
const ACCOUNT: &str = "seed";

pub(super) fn get_entry() -> Result<Entry> {
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
