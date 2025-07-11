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

use std::{fmt::Display, ptr};

use anyhow::Result;
use core_foundation::{
    base::{CFTypeRef, OSStatus, TCFType, kCFAllocatorDefault},
    boolean::CFBoolean,
    data::CFData,
    dictionary::CFMutableDictionary,
    string::CFString,
};
use objc2::{rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use objc2_local_authentication::LAContext;
use security_framework_sys::{
    access_control::{
        SecAccessControlCreateWithFlags, kSecAccessControlBiometryAny,
        kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
    },
    base::{errSecItemNotFound, errSecSuccess},
    item::{
        kSecAttrAccessControl, kSecAttrAccount, kSecAttrService, kSecClass,
        kSecClassGenericPassword, kSecReturnData, kSecUseAuthenticationContext, kSecValueData,
    },
    keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete},
};

pub(crate) struct Entry {
    pub service: String,
    pub account: String,
}

impl Entry {
    pub fn new(service: &str, account: &str) -> Result<Self> {
        Ok(Entry {
            service: service.into(),
            account: account.into(),
        })
    }

    pub fn set_password(&self, password: &str) -> Result<()> {
        let access_control = unsafe {
            SecAccessControlCreateWithFlags(
                kCFAllocatorDefault,
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly as CFTypeRef,
                kSecAccessControlBiometryAny,
                ptr::null_mut(),
            )
        };
        if access_control.is_null() {
            anyhow::bail!("failed to create access control");
        }
        let password_data = CFData::from_buffer(password.as_bytes());
        let service_str = CFString::new(&self.service);
        let account_str = CFString::new(&self.account);

        let mut query = CFMutableDictionary::new();
        unsafe {
            query.set(
                kSecClass as CFTypeRef,
                kSecClassGenericPassword as CFTypeRef,
            );
            query.set(kSecAttrService as CFTypeRef, service_str.as_CFTypeRef());
            query.set(kSecAttrAccount as CFTypeRef, account_str.as_CFTypeRef());
            query.set(kSecValueData as CFTypeRef, password_data.as_CFTypeRef());
            query.set(
                kSecAttrAccessControl as CFTypeRef,
                access_control as CFTypeRef,
            );
        }

        let status = unsafe { SecItemAdd(query.as_concrete_TypeRef(), ptr::null_mut()) };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(anyhow::anyhow!("failed to add password: {:?}", status))
        }
    }

    pub fn get_password(&self) -> core::result::Result<String, Error> {
        let context = unsafe { LAContext::new() };
        let reason = NSString::from_str("Access your seed password");
        unsafe { context.setLocalizedReason(&reason) };
        let service_str = CFString::new(&self.service);
        let account_str = CFString::new(&self.account);
        let mut query = CFMutableDictionary::new();

        unsafe {
            query.set(
                kSecClass as CFTypeRef,
                kSecClassGenericPassword as CFTypeRef,
            );
            query.set(kSecAttrService as CFTypeRef, service_str.as_CFTypeRef());
            query.set(kSecAttrAccount as CFTypeRef, account_str.as_CFTypeRef());
            query.set(
                kSecReturnData as CFTypeRef,
                CFBoolean::true_value().as_CFTypeRef(),
            );
            let context_ptr = Retained::as_ptr(&context) as *const AnyObject as CFTypeRef;
            query.set(kSecUseAuthenticationContext as CFTypeRef, context_ptr);
        }

        let mut result: CFTypeRef = ptr::null_mut();
        let status = unsafe { SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result) };
        match status {
            SEC_SUCCESS => {
                if result.is_null() {
                    return Err(Error::Other(anyhow::anyhow!(
                        "failed to read from keychain"
                    )));
                }
                let data = unsafe { CFData::wrap_under_create_rule(result as *mut _) };
                let bytes = data.bytes();
                let password = String::from_utf8_lossy(bytes).to_string();
                Ok(password)
            }
            SEC_ITEM_NOT_FOUND => Err(Error::NoEntry),
            _ => Err(Error::Other(anyhow::anyhow!(
                "keychain load failed: {:?}",
                status
            ))),
        }
    }

    pub fn delete_credential(&self) -> Result<()> {
        let service_str = CFString::new(&self.service);
        let account_str = CFString::new(&self.account);

        let mut query = CFMutableDictionary::new();
        unsafe {
            query.set(
                kSecClass as CFTypeRef,
                kSecClassGenericPassword as CFTypeRef,
            );
            query.set(kSecAttrService as CFTypeRef, service_str.as_CFTypeRef());
            query.set(kSecAttrAccount as CFTypeRef, account_str.as_CFTypeRef());
        }

        let status = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
        if status == SEC_SUCCESS || status == SEC_ITEM_NOT_FOUND {
            Ok(())
        } else {
            Err(anyhow::anyhow!("delete password failed: {:?}", status))
        }
    }
}

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

const SEC_SUCCESS: OSStatus = errSecSuccess;
const SEC_ITEM_NOT_FOUND: OSStatus = errSecItemNotFound;
