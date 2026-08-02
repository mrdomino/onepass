use std::{collections::HashMap, sync::Arc};

use keyring_core::{Entry, Error, Result, api::CredentialStoreApi};

pub(crate) struct Store {}

impl Store {
    pub fn new() -> Result<Arc<Self>> {
        Ok(Arc::new(Store {}))
    }
}

impl CredentialStoreApi for Store {
    fn id(&self) -> String {
        "null_keyring_store".to_string()
    }

    fn build(
        &self,
        _service: &str,
        _user: &str,
        _modifiers: Option<&HashMap<&str, &str>>,
    ) -> Result<Entry> {
        eprintln!("skipping get_password: keyring support is disabled");
        Err(Error::NoEntry)
    }

    fn vendor(&self) -> String {
        "https://crates.io/crates/onepass".to_string()
    }

    fn search(&self, _spec: &HashMap<&str, &str>) -> Result<Vec<Entry>> {
        Err(Error::NotSupportedByStore(
            "keyring support disabled".to_string(),
        ))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
