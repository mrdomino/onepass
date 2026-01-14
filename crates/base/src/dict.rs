use core::fmt::Write;
use std::ops::Deref;

use blake2::Blake2b256;
use digest::Digest;

use crate::fmt::{DigestWriter, Lines, TsvField};

/// This trait implements a hashed word list suitable for use in deterministic password generation.
/// The hash may be used as part of a derivation path to make generated passwords depend upon the
/// exact word list used.
pub trait Dict: Send + Sync {
    /// Return the full word list.
    fn words(&self) -> &[&str];

    /// Return the unique BLAKE2b256 hash of this word list.
    fn hash(&self) -> &[u8; 32];
}

/// This is a runtime generated, owned [`Dict`] with string slices out of some backing store.
/// These slices may come from a `Vec<String>`, or else from slices out of a single `String`.
pub struct BoxDict<'a>(Box<[&'a str]>, [u8; 32]);

/// This type provides a [`Dict`] over non-owned data. It may be used in tests, or to implement a
/// static compile-time dictionary, giving the compiler maximum freedom as to how to lay out the
/// string slices.
pub struct RefDict<'a>(&'a [&'a str], &'a [u8; 32]);

impl<'a> BoxDict<'a> {
    /// Construct a dictionary from a single string slice, taking each non-empty line, with leading
    /// and trailing whitespace trimmed, as a single word.
    pub fn from_lines(s: &'a str) -> Self {
        Self::from_iter(s.lines().map(str::trim))
    }

    /// Construct a dictionary from a single string slice, with fields separated by a separator.
    /// Individual words are not trimmed.
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

impl<'a> RefDict<'a> {
    /// Construct a dictionary from the given word slice and hash reference.
    /// # Safety
    /// This function is only safe if `hash` is the `BLAKE2b256` hash of the word list as if
    /// constructed via `BoxDict::from_iter(words.into_iter())`.
    pub const unsafe fn new(words: &'a [&'a str], hash: &'a [u8; 32]) -> Self {
        RefDict(words, hash)
    }
}

impl<'a> Deref for BoxDict<'a> {
    type Target = dyn Dict + 'a;
    fn deref(&self) -> &Self::Target {
        self
    }
}

impl<'a> Deref for RefDict<'a> {
    type Target = dyn Dict + 'a;
    fn deref(&self) -> &Self::Target {
        self
    }
}

impl Dict for BoxDict<'_> {
    fn words(&self) -> &[&str] {
        &self.0
    }
    fn hash(&self) -> &[u8; 32] {
        &self.1
    }
}

impl Dict for RefDict<'_> {
    fn words(&self) -> &[&str] {
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
            (
                "dec3a7b8941401737abb9ff3f37cde4b47c79c5be60bba8ba2ffb02fb84864ba",
                "a a",
                None,
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
