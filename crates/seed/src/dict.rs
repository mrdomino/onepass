//! This module re-exports [`onepass_base::dict`] and also defines the static [`EFF_WORDLIST`]
//! compile-time dictionary.

pub use onepass_base::dict::{BoxDict, Dict, RefDict};

/// This is the default word list corresponding to the [EFF large wordlist][0]. It contains 7776
/// words, starting with `"abacus"` and ending with `"zoom"`.
///
/// [0]: https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt
pub const EFF_WORDLIST: RefDict = unsafe { RefDict::new(&EFF_WORDLIST_WORDS, &EFF_WORDLIST_HASH) };

include!(concat!(env!("OUT_DIR"), "/wordlist.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eff_hash() {
        assert_eq!(
            "323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd",
            hex::encode(EFF_WORDLIST.hash())
        );
        assert_eq!(7776, EFF_WORDLIST.words().len());
        assert_eq!("abstract", EFF_WORDLIST.words()[22]);
    }
}
