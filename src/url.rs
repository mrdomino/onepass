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
use url::Url;

pub(crate) fn canonicalize(input: &str, username: Option<&str>) -> Result<String> {
    let mut url = Url::parse(input)
        .or_else(|_| Url::parse(format!("https://{input}").as_ref()))
        .context("invalid url")?;
    if let Some(username) = username {
        url.set_username(username)
            .map_err(|()| anyhow::anyhow!("failed setting username"))?;
    }
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalize_identity() -> Result<()> {
        let tests = [
            "https://google.com/",
            "mailto:me@example.com",
            "http://localhost/",
        ];
        for url in tests {
            assert_eq!(String::from(url), canonicalize(url, None)?);
        }
        Ok(())
    }

    #[test]
    fn canonicalize_host() -> Result<()> {
        let tests = [
            ("https://google.com/", "google.com"),
            ("https://iphone.local/", "iphone.local"),
            ("https://localhost/", "localhost"),
            ("https://google.com/", "https://GOOGLE.COM/"),
            ("http://www.google.com/", "http://WWW.GOogle.COM"),
            ("https://xn--4db.ws/", "https://א.ws"),
        ];
        for (want, inp) in tests {
            assert_eq!(String::from(want), canonicalize(inp, None)?);
        }
        Ok(())
    }

    #[test]
    fn canonicalize_username() -> Result<()> {
        let tests = [
            ("https://test@example.com/", ("example.com", "test")),
            (
                "https://foo%40bar@baz.com/",
                ("https://baz.com/", "foo@bar"),
            ),
        ];
        for (want, (url, username)) in tests {
            assert_eq!(String::from(want), canonicalize(url, Some(username))?);
        }
        Ok(())
    }
}
