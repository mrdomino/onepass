#[cfg(keyring = "no")]
mod null_keyring;

use std::sync::OnceLock;

use anyhow::{self, Context};
use keyring_core::{self, Entry, set_default_store};

const SERVICE: &str = "onepass.app.whilezero.org";
const ACCOUNT: &str = "seed";

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

pub(super) fn get_entry() -> anyhow::Result<Entry> {
    match START.get_or_init(setup_store) {
        Ok(()) => (),
        Err(err) => anyhow::bail!("Store setup failed: {err}"),
    }
    Entry::new(SERVICE, ACCOUNT).context("failed getting keyring entry")
}
