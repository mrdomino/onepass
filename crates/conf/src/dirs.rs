use std::{
    borrow::Cow,
    env,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

/// Error returned if `$HOME` is not set in the environment.
#[derive(Clone, Copy, Debug)]
pub struct HomeNotSet;

/// Tries to expand `~` to the user’s home dir.
///
/// If this sees a path whose first component is a `~`, it attempts to replace the `~` with the
/// user’s home dir (read via [`current_home`].) In all other cases, including when the home cannot
/// be computed, the literal input path is returned unchanged.
pub fn expand_home(path: &Path) -> Cow<'_, Path> {
    let mut iter = path.components().peekable();
    if let Some(Component::Normal(s)) = iter.next()
        && s == OsStr::new("~")
        && let Ok(mut path) = current_home()
    {
        path.extend(iter);
        return Cow::Owned(path);
    }
    Cow::Borrowed(path)
}

/// Returns the user’s config dir.
///
/// This only considers the shell environ; it does not check whether this directory exists, nor
/// what the system directory says. The rule is:
///
/// 1. If `$XDG_CONFIG_DIR` is set, use that.
/// 2. If on Windows and `%APPDATA%` is set, use that.
/// 3. If on Windows and `%USERPROFILE%` is set, use `%USERPROFILE%/.config`.
/// 4. Otherwise, use `$HOME/.config`.
pub fn config_dir() -> Result<PathBuf, HomeNotSet> {
    let Some(path) = env::var_os("XDG_CONFIG_DIR")
        .or_else(|| cfg!(windows).then(|| env::var_os("APPDATA")).flatten())
        .map(PathBuf::from)
    else {
        let mut path = current_home()?;
        path.push(".config");
        return Ok(path);
    };
    Ok(path)
}

/// Returns the user’s home dir.
///
/// This only considers the shell environ; on Windows, it checks `%USERPROFILE`; otherwise, it
/// returns `$HOME`.
pub fn current_home() -> Result<PathBuf, HomeNotSet> {
    #[cfg(windows)]
    if let Some(dir) = env::var_os("USERPROFILE").map(PathBuf::from) {
        return Ok(dir);
    }
    let dir = env::var_os("HOME").ok_or(HomeNotSet)?;
    Ok(PathBuf::from(dir))
}

impl core::error::Error for HomeNotSet {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        None
    }
}

impl core::fmt::Display for HomeNotSet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("failed reading $HOME")
    }
}
