#[cfg(keyring = "no")]
mod null_keyring;

use std::sync::OnceLock;

use anyhow::{self, Context};
use keyring_core::{self, Entry, set_default_store};

#[cfg(keyring = "macos")]
use std::{collections::HashMap, sync::LazyLock};

const SERVICE: &str = "onepass.app.whilezero.org";
const ACCOUNT: &str = "seed";

pub(super) fn get_entry() -> anyhow::Result<Entry> {
    raw_setup_store().context("failed to set up store")?;
    raw_get_entry().context("failed getting keyring entry")
}

fn setup_store() -> keyring_core::Result<()> {
    #[cfg(keyring = "no")]
    {
        use null_keyring::Store;
        set_default_store(Store::new()?);
        Ok(())
    }
    #[cfg(keyring = "macos")]
    {
        use apple_native_keyring_store::protected::Store;
        set_default_store(Store::new()?);
        Ok(())
    }
    #[cfg(keyring = "rs")]
    {
        #[cfg(target_os = "linux")]
        {
            use dbus_secret_service_keyring_store::Store;
            set_default_store(Store::new()?);
            Ok(())
        }
        #[cfg(target_os = "macos")]
        {
            use apple_native_keyring_store::keychain::Store;
            set_default_store(Store::new()?);
            Ok(())
        }
        #[cfg(target_os = "windows")]
        {
            use windows_native_keyring_store::Store;
            set_default_store(Store::new()?);
            Ok(())
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(Error::NotSupportedByStore(
                "Unsupported platform".to_string(),
            ))
        }
    }
}

static START: OnceLock<keyring_core::Result<()>> = OnceLock::new();

#[cfg(keyring = "macos")]
static MODS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| HashMap::from([("access-policy", "require-user-presence")]));

// We split this out from raw_get_entry because Error is not Clone.
fn raw_setup_store() -> Result<(), &'static keyring_core::Error> {
    match START.get_or_init(setup_store) {
        Ok(()) => Ok(()),
        Err(err) => Err(err),
    }
}

fn raw_get_entry() -> keyring_core::Result<Entry> {
    #[cfg(keyring = "macos")]
    {
        Entry::new_with_modifiers(SERVICE, ACCOUNT, &MODS)
    }
    #[cfg(not(keyring = "macos"))]
    {
        Entry::new(SERVICE, ACCOUNT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::assert_matches;

    use keyring_core::Error;

    #[cfg(not(keyring = "no"))]
    #[test]
    fn get_entry_succeeds() {
        if let Err(err) = raw_setup_store() {
            // XXX brittle
            assert!(err.to_string().contains("Platform failure: DBus error: The name org.freedesktop.secrets was not provided by any .service files"), "{err:?}");
            return;
        }

        assert_matches!(raw_get_entry(), Ok(_) | Err(Error::NoEntry));
    }

    #[cfg(keyring = "no")]
    #[test]
    fn get_entry_fails_unsupported() {
        raw_setup_store().unwrap();
        let err = raw_get_entry().unwrap_err();
        assert_matches!(err, Error::NoEntry);
    }
}
