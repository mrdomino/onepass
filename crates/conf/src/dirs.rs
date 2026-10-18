use std::{
    borrow::Cow,
    env,
    ffi::OsStr,
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
pub fn config_dir() -> Result<PathBuf, HomeNotSet> {
    env::var_os("XDG_CONFIG_HOME")
        .or_else(|| cfg!(windows).then(|| env::var_os("APPDATA")).flatten())
        .map(PathBuf::from)
        .ok_or(HomeNotSet)
        .or_else(|_| {
            current_home().map(|mut path| {
                path.push(".config");
                path
            })
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
