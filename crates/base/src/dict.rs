use core::fmt::Write;

use blake2::Blake2b256;
use digest::Digest;

use crate::fmt::{DigestWriter, Lines, TsvField};

pub trait Dict<'a> {
    fn words(&self) -> &[&'a str];
    fn hash(&self) -> &[u8; 32];
}

pub struct BoxDict<'a>(Box<[&'a str]>, [u8; 32]);
pub struct RefDict<'a, 'b: 'a>(&'b [&'a str], &'b [u8; 32]);

impl<'a> BoxDict<'a> {
    pub fn from_lines(s: &'a str) -> Self {
        Self::from_iter(s.lines().map(str::trim))
    }

    pub fn from_sep(s: &'a str, sep: &str) -> Self {
        Self::from_iter(s.split(sep))
    }
}

impl<'a> FromIterator<&'a str> for BoxDict<'a> {
    fn from_iter<T: IntoIterator<Item = &'a str>>(iter: T) -> Self {
        let mut items: Vec<_> = iter.into_iter().filter(|&l| !l.is_empty()).collect();
        items.sort_unstable();
        items.dedup();
        let mut w = DigestWriter(Blake2b256::new());
        // Does not panic: `Update` is infallible.
        write!(w, "{}", Lines(items.iter().map(TsvField))).unwrap();
        BoxDict(items.into(), w.0.finalize().into())
    }
}

impl<'a, 'b: 'a> RefDict<'a, 'b> {
    pub const fn new(words: &'b [&'a str], hash: &'b [u8; 32]) -> Self {
        RefDict(words, hash)
    }
}

impl<'a> Dict<'a> for BoxDict<'a> {
    fn words(&self) -> &[&'a str] {
        &self.0
    }
    fn hash(&self) -> &[u8; 32] {
        &self.1
    }
}

impl<'a> Dict<'a> for RefDict<'a, '_> {
    fn words(&self) -> &[&'a str] {
        self.0
    }
    fn hash(&self) -> &[u8; 32] {
        self.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_dict_hash_vectors() {
        let tests: &[(&str, &str, Option<&str>)] = &[
            (
                "749a7ee32cf838199eae943516767f7ef02d49b212202f1aad74cacd645e2edf",
                "bob\ndole",
                None,
            ),
            (
                "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8",
                "",
                None,
            ),
            (
                "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8",
                " \n",
                None,
            ),
            (
                "f9a96c938288e95ab3b8804104a69daf44e925fd962565233d9de5d26e951068",
                "bob\ndole",
                Some("\0"),
            ),
            (
                "3b4312af5a1f7e9eb79c27b4503f734d303e6664d2df2796ec034b4c34195dbf",
                "a\nb\nc",
                None,
            ),
            (
                "3b4312af5a1f7e9eb79c27b4503f734d303e6664d2df2796ec034b4c34195dbf",
                "b\nc\na",
                None,
            ),
            (
                "3b4312af5a1f7e9eb79c27b4503f734d303e6664d2df2796ec034b4c34195dbf",
                "  b \na   \nc\n\n\n",
                None,
            ),
            (
                "3b4312af5a1f7e9eb79c27b4503f734d303e6664d2df2796ec034b4c34195dbf",
                "a\0b\0c",
                Some("\0"),
            ),
            (
                "3b4312af5a1f7e9eb79c27b4503f734d303e6664d2df2796ec034b4c34195dbf",
                "c\0b\0a\0a\0a",
                Some("\0"),
            ),
            (
                "3b42ee5c745153f2fe8533b19c35411d8d45c70bbecf0dc3ac9e60b7eb5ea07d",
                " \0",
                Some("\0"),
            ),
            (
                "ff11901891de4daf46c9ffc4a5c23ae22c4fa2597dc1beb86d2ef5bf87d9c878",
                "\\\r\n\t",
                Some("\0"),
            ),
        ];
        for (want, inp, sep) in tests {
            let dict = match sep {
                None => BoxDict::from_lines(inp),
                Some(sep) => BoxDict::from_sep(inp, sep),
            };
            assert_eq!(want, &hex::encode(dict.hash()), "{:?}", dict.words());
        }
    }
}
