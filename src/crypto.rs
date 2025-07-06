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
use keyring::Entry;
use rand_chacha::ChaCha20Rng;
use rand_core::{RngCore, SeedableRng};
use rpassword::prompt_password;
use zeroize::{Zeroize, Zeroizing};

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
    Entry::new("org.whilezero.app.onepass", "seed").context("failed constructing keyring entry")
}

pub(crate) struct Rng(ChaCha20Rng);

impl Rng {
    pub fn from_password_salt(password: Zeroizing<String>, salt: String) -> Result<Self> {
        let mut key_material = Zeroizing::new([0u8; 32]);
        let params =
            Params::new(32 * 1024, 3, 1, None).map_err(|e| anyhow::anyhow!("Params::new: {e}"))?;
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password.as_bytes(), salt.as_bytes(), &mut *key_material)
            .map_err(|e| anyhow::anyhow!("argon2 failed: {e}"))?;
        Ok(Rng(ChaCha20Rng::from_seed(*key_material)))
    }
}

impl RngCore for Rng {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dst: &mut [u8]) {
        self.0.fill_bytes(dst)
    }
}

impl Drop for Rng {
    fn drop(&mut self) {
        unsafe {
            let ptr = &mut self.0 as *mut ChaCha20Rng as *mut u8;
            let size = std::mem::size_of::<ChaCha20Rng>();
            let slice = std::slice::from_raw_parts_mut(ptr, size);
            slice.zeroize();
        }
    }
}
