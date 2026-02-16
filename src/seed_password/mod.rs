mod keyring;

use anyhow::{Context, Result};
use keyring::{Error, get_entry};
use readpassphrase_3::getpass;
use zeroize::Zeroizing;

/// read reads the seed password from either the system keyring or the console.
///
/// If `confirm` is true, then the password is checked against a confirmation that is always read
/// from the console. This allows the user to confirm that the seed password is what they think it
/// is without otherwise exposing the password.
pub(crate) fn read(use_keyring: bool, confirm: bool) -> Result<Zeroizing<String>> {
    let password = use_keyring.then(load_keyring).transpose()?.flatten();
    if let Some(password) = password {
        if confirm && !check_confirm(&password)? {
            anyhow::bail!("passwords don’t match");
        }
        return Ok(password);
    }
    let password: Zeroizing<String> = getpass(c"Seed password: ")
        .context("failed reading password")?
        .into();
    if (use_keyring || confirm) && !check_confirm(&password)? {
        anyhow::bail!("passwords don’t match");
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
    }
    Ok(())
}

pub(crate) fn check_confirm(password: &str) -> Result<bool> {
    let confirmed: Zeroizing<String> = getpass(c"Confirmation: ")
        .context("failed reading confirmation")?
        .into();
    Ok(*confirmed == password)
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
