pub use onepass_base::dict::{BoxDict, Dict, RefDict};

pub const EFF_WORDLIST: RefDict = RefDict::new(&EFF_WORDLIST_WORDS, &EFF_WORDLIST_HASH);

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
