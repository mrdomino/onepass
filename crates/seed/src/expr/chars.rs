use core::cmp::max;
use std::io::{Result, Write};

use crypto_bigint::{NonZero, U256};
use zeroize::Zeroizing;

use super::{Eval, util::u256_to_word};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Chars(pub Box<[CharRange]>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharRange {
    pub start: char,
    pub end: char,
}

impl From<(char, char)> for CharRange {
    fn from((start, end): (char, char)) -> Self {
        CharRange { start, end }
    }
}

impl Chars {
    /// # Safety
    /// This function is only safe for evaluation if the ranges are non-overlapping.
    pub unsafe fn from_ranges_unchecked<T, V>(ranges: V) -> Self
    where
        V: IntoIterator<Item = T>,
        T: Into<CharRange>,
    {
        Chars(ranges.into_iter().map(Into::into).collect())
    }

    pub fn from_ranges<T, V>(ranges: V) -> Self
    where
        V: IntoIterator<Item = T>,
        T: Into<CharRange>,
    {
        let mut ranges = ranges.into_iter().map(Into::into).collect::<Vec<_>>();
        ranges.sort_unstable_by(|a, b| a.start.cmp(&b.start));
        let mut i = 0;
        let mut j = 1;
        while j < ranges.len() {
            let Some(next) = next_char(ranges[i].end) else {
                break;
            };
            if next >= ranges[j].start {
                ranges[i].end = max(ranges[i].end, ranges[j].end);
                j += 1;
                continue;
            }
            if j != i + 1 {
                ranges.swap(i + 1, j);
            }
            i += 1;
            j += 1;
        }
        ranges.drain(i + 1..);
        Chars(ranges.into())
    }

    fn size(&self) -> u32 {
        self.0.iter().map(|range| range.size()).sum()
    }

    fn nth(&self, mut n: u32) -> char {
        for range in &self.0 {
            let sz = range.size();
            if n < sz {
                return range.nth(n);
            }
            n -= sz;
        }
        unreachable!()
    }
}

fn next_char(c: char) -> Option<char> {
    char::from_u32(match c {
        '\u{d799}' => 0xe00,
        _ => u32::from(c) + 1,
    })
}

impl CharRange {
    // TODO(someday): replace these with `Step` methods once those are stabilized.

    fn size(&self) -> u32 {
        let start = self.start as u32;
        let end = self.end as u32;
        assert!(start <= end, "{:?} > {:?}", self.start, self.end);
        let count = end - start + 1;
        if start < 0xD800 && 0xE000 <= end {
            count - 0x800
        } else {
            count
        }
    }

    fn nth(&self, n: u32) -> char {
        let start = self.start as u32;
        let res = start + n;
        let res = if start < 0xD800 && res >= 0xD800 {
            res + 0x800
        } else {
            res
        };
        let res = char::from_u32(res).unwrap();
        assert!(res <= self.end);
        res
    }
}

impl Eval for Chars {
    fn size(&self) -> NonZero<U256> {
        NonZero::new(Chars::size(self).into()).unwrap()
    }

    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        let c: Zeroizing<_> = self.nth(u256_to_word(&index).try_into().unwrap()).into();
        write!(w, "{}", *c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_ranges() {
        let rs = Chars::from_ranges([('b', 'e'), ('a', 'c'), ('z', 'z')]);
        assert_eq!(6, rs.size());
        assert_eq!('a', rs.nth(0));
        assert_eq!('z', rs.nth(5));

        let rs = Chars::from_ranges(vec![('a', 'z')]);
        assert_eq!(26, rs.size());
    }
}
