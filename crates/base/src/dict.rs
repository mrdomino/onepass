use core::{
    fmt::{self, Write},
    ops::Deref,
};
use std::ops::Range;

use blake2::Blake2b256;
use digest::Digest;

use crate::fmt::{DigestWriter, Lines, TsvField};

/// This trait implements a hashed word list suitable for use in deterministic password generation.
/// The hash may be used as part of a derivation path to make generated passwords depend upon the
/// exact word list used.
pub trait Dict: fmt::Debug + Send + Sync {
    /// Returns the number of words in the list.
    fn len(&self) -> usize;

    /// Returns true iff the word list is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return the `i`th word in the list.
    fn word(&self, i: usize) -> &str;

    /// Return the unique BLAKE2b256 hash of this word list.
    fn hash(&self) -> &[u8; 32];
}

/// This is a runtime generated, owned [`Dict`] with string slices out of some backing store.
/// These slices may come from a `Vec<String>`, or else from slices out of a single `String`.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct BoxDict {
    backing: Box<str>,
    spans: Box<[Range<usize>]>,
    hash: [u8; 32],
}

/// This type provides a [`Dict`] over non-owned data. It may be used in tests, or to implement a
/// static compile-time dictionary, giving the compiler maximum freedom as to how to lay out the
/// string slices.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RefDict<'a>(&'a [&'a str], &'a [u8; 32]);

impl BoxDict {
    /// Construct a dictionary from a single string slice, taking each non-empty line, with leading
    /// and trailing whitespace trimmed, as a single word.
    pub fn from_lines(backing: impl Into<Box<str>>) -> Self {
        let backing = backing.into();
        let (words, hash) = canonicalize(backing.lines().map(str::trim));
        let spans = adopt(&backing, words);
        Self {
            backing,
            spans,
            hash,
        }
    }

    /// Construct a dictionary from a single string slice, with fields separated by a separator.
    /// Individual words are not trimmed.
    pub fn from_sep(backing: impl Into<Box<str>>, sep: &str) -> Self {
        let backing = backing.into();
        let (words, hash) = canonicalize(backing.split(sep));
        let spans = adopt(&backing, words);
        Self {
            backing,
            spans,
            hash,
        }
    }
}

fn canonicalize<'a>(words: impl Iterator<Item = &'a str>) -> (Vec<&'a str>, [u8; 32]) {
    let mut words: Vec<_> = words.filter(|&w| !w.is_empty()).collect();
    words.sort_unstable();
    words.dedup();
    let mut w = DigestWriter(Blake2b256::new());
    write!(w, "{}", Lines(words.iter().map(TsvField))).unwrap();
    (words, w.0.finalize().into())
}

fn adopt(backing: &str, words: Vec<&str>) -> Box<[Range<usize>]> {
    let base = backing.as_ptr() as usize;
    words
        .into_iter()
        .map(|w| {
            let offset = (w.as_ptr() as usize).checked_sub(base).unwrap();
            let end = offset.checked_add(w.len()).unwrap();
            assert!(end <= backing.len());
            offset..end
        })
        .collect()
}

fn compact(words: Vec<&str>) -> (Box<str>, Box<[Range<usize>]>) {
    let len = words.iter().copied().map(|w| w.len()).sum();
    let mut backing = String::with_capacity(len);
    let spans = words
        .into_iter()
        .map(|word| {
            let start = backing.len();
            backing.push_str(word);
            start..backing.len()
        })
        .collect();
    (backing.into(), spans)
}

impl<S: AsRef<str>> FromIterator<S> for BoxDict {
    fn from_iter<T: IntoIterator<Item = S>>(iter: T) -> Self {
        let words: Vec<_> = iter.into_iter().collect();
        let (words, hash) = canonicalize(words.iter().map(S::as_ref));
        let (backing, spans) = compact(words);
        BoxDict {
            backing,
            spans,
            hash,
        }
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

impl Deref for BoxDict {
    type Target = dyn Dict;
    fn deref(&self) -> &Self::Target {
        self
    }
}

impl Deref for RefDict<'static> {
    type Target = dyn Dict;
    fn deref(&self) -> &Self::Target {
        self
    }
}

impl Dict for BoxDict {
    fn len(&self) -> usize {
        self.spans.len()
    }

    fn word(&self, i: usize) -> &str {
        &self.backing[self.spans[i].clone()]
    }

    fn hash(&self) -> &[u8; 32] {
        &self.hash
    }
}

impl Dict for RefDict<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }
    fn word(&self, i: usize) -> &str {
        self.0[i]
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
        for &(want, inp, sep) in tests {
            let dict = match sep {
                None => BoxDict::from_lines(inp),
                Some(sep) => BoxDict::from_sep(inp, sep),
            };
            assert_eq!(want, &hex::encode(dict.hash()), "{inp:?}");
        }
    }
}
