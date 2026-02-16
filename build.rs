use std::{env, ffi::OsStr};

fn main() {
    embed_info_plist();
}

fn embed_info_plist() {
    // If we are using the biometric keychain API, we must embed Info.plist for the app to work.
    if env::var_os("CARGO_FEATURE_MACOS_BIOMETRY").is_none()
        || env::var_os("CARGO_CFG_TARGET_OS").is_some_and(|os| OsStr::new("macos") != os)
    {
        return;
    }
    println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,data/Info.plist");
    println!("cargo:rerun-if-changed=data/Info.plist");
}
