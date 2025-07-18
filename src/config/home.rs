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

use std::env;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};
use nix::unistd::{Uid, User};

pub(crate) fn expand_home<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let mut ret = PathBuf::new();
    let mut components = path.as_ref().components();
    match components.next() {
        Some(Component::Normal(os)) => {
            if let Some(s) = os.to_str() {
                if s == "~" {
                    if let Some(home) = env::var_os("HOME") {
                        ret.push(home);
                    } else {
                        ret.push(get_current_home()?);
                    }
                } else if let Some(s) = s.strip_prefix("~") {
                    ret.push(get_home(s)?);
                } else {
                    ret.push(s);
                }
            } else {
                ret.push(os)
            }
        }
        Some(component) => ret.push(component),
        None => return Ok(ret),
    }
    for component in components {
        ret.push(component);
    }
    Ok(ret)
}

fn get_home(name: &str) -> Result<PathBuf> {
    let user = User::from_name(name)?.context("failed to lookup user")?;
    Ok(user.dir)
}

fn get_current_home() -> Result<PathBuf> {
    let user = User::from_uid(Uid::current())?.context("failed to lookup user")?;
    Ok(user.dir)
}
