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

use std::{
    fmt::Display,
    ptr::{self, NonNull},
    slice,
};

use anyhow::{Context, Result};
use objc2::runtime::AnyObject;
use objc2_core_foundation::{
    CFDictionary, CFMutableData, CFRetained, CFString, CFType, kCFBooleanTrue,
};
use objc2_foundation::NSString;
use objc2_local_authentication::LAContext;
use objc2_security::{
    SecAccessControl, SecAccessControlCreateFlags, SecItemAdd, SecItemCopyMatching, SecItemDelete,
    errSecItemNotFound, errSecSuccess, errSecUserCanceled, kSecAttrAccessControl,
    kSecAttrAccessibleWhenUnlockedThisDeviceOnly, kSecAttrAccount, kSecAttrService, kSecClass,
    kSecClassGenericPassword, kSecReturnData, kSecUseAuthenticationContext, kSecValueData,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Entry exposes an API superficially compatible with the subset of keyring::Entry used by
/// onepass. The difference is that it asks for the password it saves to be protected by biometric
/// unlock. This requires using an LAContext when we unlock the password, and also requires the app
/// to be codesigned with the correct entitlements.
pub(crate) struct Entry {
    pub service: CFRetained<CFString>,
    pub account: CFRetained<CFString>,
}

impl Entry {
    pub fn new(service: &str, account: &str) -> Result<Self> {
        Ok(Entry {
            service: CFString::from_str(service),
            account: CFString::from_str(account),
        })
    }

    pub fn set_password(&self, password: &str) -> Result<()> {
        let access_control = unsafe {
            SecAccessControl::with_flags(
                None,
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
                SecAccessControlCreateFlags::BiometryAny,
                ptr::null_mut(),
            )
        }
        .context("failed creating access control")?;
        let password_bytes = password.as_bytes();
        let password = SecureData(
            CFMutableData::new(None, password_bytes.len() as isize)
                .context("failed to allocate buffer")?,
        );
        unsafe {
            CFMutableData::append_bytes(
                Some(&password.0),
                password_bytes.as_ptr(),
                password_bytes.len() as isize,
            );
        }
        let query = unsafe {
            CFDictionary::from_slices(
                &[
                    kSecClass,
                    kSecAttrService,
                    kSecAttrAccount,
                    kSecValueData,
                    kSecAttrAccessControl,
                ],
                &[
                    kSecClassGenericPassword as &CFType,
                    &self.service,
                    &self.account,
                    &password.0,
                    &access_control,
                ],
            )
        };
        let status = unsafe { SecItemAdd(query.as_opaque(), ptr::null_mut()) };
        if status != errSecSuccess {
            anyhow::bail!("failed to set password: {status}");
        }
        Ok(())
    }

    pub fn get_password(&self) -> core::result::Result<String, Error> {
        let reason = NSString::from_str("load your seed password");
        let context = unsafe { LAContext::new() };
        unsafe { context.setLocalizedReason(&reason) };
        let context = unsafe { std::mem::transmute::<&AnyObject, &CFType>(&context) };
        let query = unsafe {
            CFDictionary::from_slices(
                &[
                    kSecClass,
                    kSecAttrService,
                    kSecAttrAccount,
                    kSecReturnData,
                    kSecUseAuthenticationContext,
                ],
                &[
                    kSecClassGenericPassword as &CFType,
                    &self.service,
                    &self.account,
                    kCFBooleanTrue.unwrap(),
                    context,
                ],
            )
        };
        let mut result: *const CFType = ptr::null();
        let status =
            unsafe { SecItemCopyMatching(query.as_opaque(), &mut result as *mut *const CFType) };
        if status == errSecUserCanceled {
            return Err(Error::Other(anyhow::anyhow!("Authentication canceled")));
        } else if status == errSecItemNotFound {
            return Err(Error::NoEntry);
        } else if status != errSecSuccess {
            return Err(Error::Other(anyhow::anyhow!(
                "failed to load password: {status:?}"
            )));
        }
        // XXX: `SecItemCopyMatching` actually returns `CFData`, so this is not allowed, but
        // empirically it has been working so far.
        let result = NonNull::new(result as *mut CFMutableData)
            .context("nil result from keychain")
            .map_err(Error::Other)?;
        let result = SecureData(unsafe { CFRetained::from_raw(result) });
        let password = str::from_utf8(unsafe { result.0.as_bytes_unchecked() })
            .context("non-utf8 password; delete with -r")
            .map_err(Error::Other)?;
        Ok(String::from(password))
    }

    pub fn delete_credential(&self) -> core::result::Result<(), Error> {
        let query = unsafe {
            CFDictionary::from_slices(
                &[kSecClass, kSecAttrService, kSecAttrAccount],
                &[kSecClassGenericPassword, &self.service, &self.account],
            )
        };
        let status = unsafe { SecItemDelete(query.as_opaque()) };
        if status == errSecItemNotFound {
            return Err(Error::NoEntry);
        } else if status != errSecSuccess {
            return Err(Error::Other(anyhow::anyhow!(
                "failed to delete password: {status:?}"
            )));
        }
        Ok(())
    }
}

/// Error is a superficially compatible type with keyring::Error. In particular, it must expose a
/// NoEntry option, so this can be checked by the functions that call keyring.
#[derive(Debug)]
pub(crate) enum Error {
    NoEntry,
    Other(anyhow::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoEntry => write!(f, "entry not found"),
            Error::Other(err) => err.fmt(f),
        }
    }
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Other(err) => Some(err.as_ref()),
            _ => None,
        }
    }
}

struct SecureData(CFRetained<CFMutableData>);

impl Drop for SecureData {
    fn drop(&mut self) {
        unsafe {
            let ptr = CFMutableData::mutable_byte_ptr(Some(&self.0));
            let len = self.0.len();
            if !ptr.is_null() {
                let slice: &mut [u8] = slice::from_raw_parts_mut(ptr, len);
                slice.zeroize();
            }
        }
    }
}
impl ZeroizeOnDrop for SecureData {}
