use std::{env, ffi::OsStr};

fn main() {
    // Opt out of default always-rerun behavior.
    println!("cargo::rerun-if-changed=build.rs");

    println!("cargo::rustc-check-cfg=cfg(keyring, values(\"macos\", \"no\", \"rs\"))");
    if env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| os == OsStr::new("macos"))
        && env::var_os("CARGO_FEATURE_MACOS_BIOMETRY").is_some()
    {
        println!("cargo::rustc-cfg=keyring=\"macos\"");
        // If we are using the biometric keychain API, we must embed Info.plist for the app to work.
        println!("cargo::rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,data/Info.plist");
        println!("cargo::rerun-if-changed=data/Info.plist");
    } else if env::var_os("CARGO_FEATURE_KEYRING").is_some() {
        println!("cargo::rustc-cfg=keyring=\"rs\"");
    } else {
        println!("cargo::rustc-cfg=keyring=\"no\"");
    }
}
