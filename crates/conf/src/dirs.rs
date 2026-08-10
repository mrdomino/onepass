use std::{
    borrow::Cow,
    env,
    ffi::OsStr,
    fs,
    path::{Component, Path, PathBuf},
};

pub use crate::error::HomeNotSet;

/// Tries to expand `~` to the user’s home dir.
///
/// If this sees a path whose first component is a `~`, it attempts to replace the `~` with the
/// user’s home dir (read via [`current_home`].) In all other cases, the literal input path is
/// returned unchanged.
pub fn expand_home(path: &Path) -> Result<Cow<'_, Path>, HomeNotSet> {
    let mut iter = path.components();
    if let Some(Component::Normal(s)) = iter.next()
        && s == OsStr::new("~")
    {
        let mut path = current_home()?;
        path.extend(iter);
        return Ok(Cow::Owned(path));
    }
    Ok(Cow::Borrowed(path))
}

/// Returns the user’s config dir.
///
/// This only considers the shell environ; it does not check whether this directory exists, nor
/// what the system directory says. The rule is:
///
/// 1. If `$XDG_CONFIG_HOME` is set, use that.
/// 2. If on Windows and `%APPDATA%` is set, use that.
/// 3. If on Windows and `%USERPROFILE%` is set, use `%USERPROFILE%/.config`.
/// 4. Otherwise, use `$HOME/.config`.
///
/// Old versions of onepass accidentally read the non-standard `$XDG_CONFIG_DIR` instead of
/// `$XDG_CONFIG_HOME`. To aid in the transition, this function peeks beyond the shell environ: if
/// an existing onepass config is detected on the filesystem according to the old logic, it will be
/// used and a warning will be displayed to migrate.
pub fn config_dir() -> Result<PathBuf, HomeNotSet> {
    // TODO(major): drop XDG_CONFIG_DIR.
    let new_home = config_home("XDG_CONFIG_HOME");
    let old_home = config_home("XDG_CONFIG_DIR");
    if env::var_os("XDG_CONFIG_DIR").is_some() && new_home != old_home {
        eprintln!("WARNING: XDG_CONFIG_DIR is deprecated. Switch to XDG_CONFIG_HOME.");
    }
    match (old_home, new_home) {
        (None, None) => Err(HomeNotSet),
        (None, Some(new)) => Ok(new),
        (Some(old), None) => Ok(old),
        (Some(old), Some(new)) if old == new => Ok(new),
        (Some(old), Some(new)) => match fs::exists(old.join("onepass")) {
            Ok(true) | Err(_) => {
                eprintln!(
                    concat!(
                        "WARNING: A future release will use {0:?} instead of {1:?}.\n",
                        "Move your config over now and remove {1:?}."
                    ),
                    new.join("onepass"),
                    old.join("onepass")
                );
                Ok(old)
            }
            Ok(false) => Ok(new),
        },
    }
}

fn config_home(var_name: &str) -> Option<PathBuf> {
    env::var_os(var_name)
        .or_else(|| cfg!(windows).then(|| env::var_os("APPDATA")).flatten())
        .map(PathBuf::from)
        .or_else(|| {
            current_home()
                .map(|mut path| {
                    path.push(".config");
                    path
                })
                .ok()
        })
}

/// Returns the user’s home dir.
///
/// This only considers the shell environ; it does not check whether this directory exists, nor
/// what the system directory says.
///
/// If on Windows and `%USERPROFILE%` is set, that is returned. Otherwise, if `$HOME` is set, that
/// is returned.
pub fn current_home() -> Result<PathBuf, HomeNotSet> {
    #[cfg(windows)]
    if let Some(dir) = env::var_os("USERPROFILE").map(PathBuf::from) {
        return Ok(dir);
    }
    let dir = env::var_os("HOME").ok_or(HomeNotSet)?;
    Ok(PathBuf::from(dir))
}
