use core::result;

use anyhow::{self};

use super::Error;

pub(crate) struct Entry {}

// TODO(someday): make this and macos_keychain.rs consistent with each other (Result, etc)

impl Entry {
    pub fn new(_service: &str, _account: &str) -> anyhow::Result<Self> {
        Ok(Entry {})
    }

    pub fn set_password(&self, _password: &str) -> anyhow::Result<()> {
        eprintln!("skipping set_password: keyring support is disabled");
        Ok(())
    }

    pub fn get_password(&self) -> result::Result<String, Error> {
        eprintln!("skipping get_password: keyring support is disabled");
        Err(Error::NoEntry)
    }

    pub fn delete_credential(&self) -> result::Result<(), Error> {
        eprintln!("skipping delete_credential: keyring support is disabled");
        Ok(())
    }
}
