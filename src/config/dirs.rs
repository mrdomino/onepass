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
    env,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

pub(crate) fn config_dir() -> Option<PathBuf> {
    if let Some(config_dir) = env::var_os("XDG_CONFIG_DIR").map(PathBuf::from) {
        return Some(config_dir);
    }
    #[cfg(windows)]
    if let Some(config_dir) = env::var_os("APPDATA").map(PathBuf::from) {
        return Some(config_dir);
    }
    let mut dir = current_home()?;
    dir.push(".config");
    Some(dir)
}

pub(crate) fn current_home() -> Option<PathBuf> {
    #[cfg(windows)]
    if let Some(dir) = env::var_os("USERPROFILE").map(PathBuf::from) {
        return Some(dir);
    }
    env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn expand_home<P: AsRef<Path>>(path: P) -> Option<PathBuf> {
    let mut ret = PathBuf::new();
    let mut components = path.as_ref().components();
    match components.next() {
        Some(Component::Normal(os)) if os == OsStr::new("~") => {
            ret.push(current_home()?);
        }
        Some(Component::Normal(os)) if os.to_str()?.starts_with("~") => {
            return None;
        }
        Some(component) => ret.push(component),
        None => return Some(ret),
    }
    for component in components {
        ret.push(component);
    }
    Some(ret)
}
