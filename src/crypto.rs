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

use anyhow::{Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use blake3::OutputReader;
use keyring::Entry;
use rand_core::RngCore;
use rpassword::prompt_password;
use whoami::fallible::username;
use zeroize::Zeroizing;

pub(crate) fn read_password(use_keyring: bool, confirm: bool) -> Result<Zeroizing<String>> {
    let password = use_keyring
        .then(|| read_password_keyring(confirm))
        .transpose()?
        .flatten();
    if let Some(password) = password {
        return Ok(password);
    }
    let password: Zeroizing<String> = prompt_password("Master password: ")
        .context("failed reading password")?
        .into();
    if !confirm
        .then(|| check_confirm(&password))
        .transpose()?
        .unwrap_or(true)
    {
        anyhow::bail!("passwords don't match");
    }
    if use_keyring {
        let entry = get_onepass_entry()?;
        if let Err(e) = entry.set_password(password.as_str()) {
            eprintln!("failed storing password in keychain: {e}");
        }
    }
    Ok(password)
}

fn read_password_keyring(confirm: bool) -> Result<Option<Zeroizing<String>>> {
    let entry = get_onepass_entry()?;
    let password: Zeroizing<String> = match entry.get_password() {
        Err(keyring::Error::NoEntry) => return Ok(None),
        r => r.context("failed getting password from keyring")?.into(),
    };
    if !confirm
        .then(|| check_confirm(&password))
        .transpose()?
        .unwrap_or(true)
    {
        anyhow::bail!("passwords don't match");
    }
    Ok(Some(password))
}

fn check_confirm(password: &Zeroizing<String>) -> Result<bool> {
    let confirm: Zeroizing<String> = prompt_password("Confirm: ")
        .context("failed reading password confirmation")?
        .into();
    Ok(password.as_str() == confirm.as_str())
}

pub fn get_onepass_entry() -> Result<Entry> {
    let user = username().context("failed getting username")?;
    Entry::new("onepass", &user).context("failed constructing keyring entry")
}

pub(crate) struct Blake3Rng(Zeroizing<OutputReader>);

impl Blake3Rng {
    pub fn from_password_salt(password: Zeroizing<String>, salt: String) -> Result<Self> {
        let mut key_material = Zeroizing::new([0u8; 32]);
        let params = Params::new(32 * 1024, 3, 1, None)
            .map_err(|e| anyhow::anyhow!("argon2 params failed: {e}"))?;
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut *key_material)
            .map_err(|e| anyhow::anyhow!("argon2 failed: {e}"))?;
        Ok(Blake3Rng::new(key_material))
    }

    pub fn new(key_material: Zeroizing<[u8; 32]>) -> Self {
        let mut hasher = Zeroizing::new(blake3::Hasher::new());
        hasher.update(&*key_material);
        Blake3Rng(Zeroizing::new(hasher.finalize_xof()))
    }
}

impl RngCore for Blake3Rng {
    fn next_u32(&mut self) -> u32 {
        let mut bytes = [0u8; 4];
        self.0.fill(&mut bytes);
        u32::from_le_bytes(bytes)
    }

    fn next_u64(&mut self) -> u64 {
        let mut bytes = [0u8; 8];
        self.0.fill(&mut bytes);
        u64::from_le_bytes(bytes)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill(dst);
    }

    fn try_fill_bytes(
        &mut self,
        dest: &mut [u8],
    ) -> std::result::Result<(), crypto_bigint::rand_core::Error> {
        self.fill_bytes(dest);
        Ok(())
    }
}
