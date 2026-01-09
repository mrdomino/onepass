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

use url::Url;

#[derive(Clone, Copy, Debug)]
pub enum Error {
    SetUsernameError,
    ParseError(url::ParseError),
}

#[derive(Clone, Copy, Debug)]
pub struct SetUsernameError;

pub fn normalize(input: &str, username: Option<&str>) -> Result<String, Error> {
    let mut url = Url::parse(input)
        .or_else(|_| Url::parse(format!("https://{input}").as_ref()))
        .map_err(Error::ParseError)?;
    if let Some(username) = username {
        url.set_username(username)
            .map_err(|()| Error::SetUsernameError)?
    }
    Ok(url.into())
}

impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SetUsernameError => None,
            Self::ParseError(e) => Some(e),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetUsernameError => f.write_str("failed setting username"),
            Self::ParseError(e) => e.fmt(f),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_identity() {
        let tests = [
            "https://google.com/",
            "mailto:me@example.com",
            "http://localhost/",
        ];
        for url in tests {
            assert_eq!(String::from(url), normalize(url, None).unwrap());
        }
    }

    #[test]
    fn normalize_host() {
        let tests = [
            ("https://google.com/", "google.com"),
            ("https://iphone.local/", "iphone.local"),
            ("https://localhost/", "localhost"),
            ("https://google.com/", "https://GOOGLE.COM/"),
            ("http://www.google.com/", "http://WWW.GOogle.COM"),
            ("https://xn--4db.ws/", "https://א.ws"),
        ];
        for (want, inp) in tests {
            assert_eq!(String::from(want), normalize(inp, None).unwrap());
        }
    }

    #[test]
    fn normalize_username() {
        let tests = [
            ("https://test@example.com/", ("example.com", "test")),
            (
                "https://foo%40bar@baz.com/",
                ("https://baz.com/", "foo@bar"),
            ),
        ];
        for (want, (url, username)) in tests {
            assert_eq!(String::from(want), normalize(url, Some(username)).unwrap());
        }
    }
}
