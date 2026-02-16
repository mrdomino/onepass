use url::Url;

pub type Error = url::ParseError;

/// Apply some light normalization to an input URL.
///
/// This function does whatever [`url::Url::parse`] does, e.g. punycode conversion, case folding,
/// and path normalization. It also, if given a URL that does not parse successfully, tries
/// prepending `"https://"` and re-parsing, thus normalizing between schemeless and schemed URLs.
// TODO(someday): revisit the https:// prepending. It seems kinda sketchy, maybe we should only do
// it if the URL does not contain a : or does not start with "http://" or "https://". Maybe we
// should just normalize everything to HTTPS or HTTP, or drop the scheme.
pub fn normalize(input: &str) -> Result<String, Error> {
    Url::parse(input)
        .or_else(|_| Url::parse(format!("https://{input}").as_ref()))
        .map(Into::into)
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
            assert_eq!(String::from(url), normalize(url).unwrap());
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
            (
                "https://test%40email.example@google.com/",
                "test%40email.example@google.com",
            ),
        ];
        for (want, inp) in tests {
            assert_eq!(String::from(want), normalize(inp).unwrap());
        }
    }
}
