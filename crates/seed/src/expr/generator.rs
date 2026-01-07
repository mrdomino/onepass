use core::fmt;
use std::{
    collections::HashMap,
    io::{Result, Write},
    sync::Arc,
};

use crypto_bigint::{NonZero, U256, Word as _Word, Zero};
use onepass_base::dict::Dict;
use zeroize::Zeroizing;

use super::{
    EvalContext,
    util::{u256_saturating_pow, u256_to_word},
};
use crate::dict::EFF_WORDLIST;

#[derive(Clone, Debug)]
pub struct Generator(Box<str>);

pub trait GeneratorFunc: Send + Sync {
    fn name(&self) -> &'static str;
    fn size(&self, args: &[&str]) -> NonZero<U256>;
    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>, args: &[&str]) -> Result<()>;

    fn fmt(&self, f: &mut fmt::Formatter<'_>, args: &[&str]) -> fmt::Result {
        write!(f, "{}", self.name())?;
        // TODO(now): find first available punctuation char, proper escaping
        for arg in args {
            write!(f, ":{arg}")?;
        }
        Ok(())
    }
}

pub struct Context(HashMap<&'static str, Arc<dyn GeneratorFunc>>);

// TODO(someday): multiple dict lookup by hash
pub struct Word<'a, 'b>(&'a (dyn Dict<'b> + Sync));

pub struct Words<'a, 'b>(&'a (dyn Dict<'b> + Sync));

impl EvalContext for Generator {
    type Context = Context;

    fn size(&self, context: &Context) -> NonZero<U256> {
        let name = self.name();
        let func = context
            .get(name)
            .ok_or_else(|| format!("unknown generator {name}"))
            .unwrap();
        func.size(&self.args())
    }

    fn write_to(&self, context: &Context, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        let name = self.name();
        let func = context
            .get(name)
            .ok_or_else(|| format!("unknown generator {name}"))
            .unwrap();
        func.write_to(w, index, &self.args())
    }
}

impl Generator {
    pub fn from(s: impl Into<Box<str>>) -> Self {
        Generator(s.into())
    }

    pub fn new(s: &str) -> Self {
        Generator(s.into())
    }

    pub fn name(&self) -> &str {
        &self.0[..self
            .0
            .bytes()
            .enumerate()
            .filter(|&(_, b)| !b.is_ascii_lowercase())
            .map(|(i, _)| i)
            .next()
            .unwrap_or(self.0.len())]
    }

    pub fn args(&self) -> Box<[&str]> {
        let Some(sep) = self.0.chars().find(|&c| !c.is_ascii_lowercase()) else {
            return [].into();
        };
        self.0.split(sep).collect()
    }
}

impl Context {
    pub fn empty() -> Self {
        Context(HashMap::new())
    }

    pub fn get<'a>(&'a self, name: &str) -> Option<&'a dyn GeneratorFunc> {
        self.0.get(name).map(Arc::as_ref)
    }
}

impl Default for Context {
    fn default() -> Self {
        let generators: Vec<Arc<dyn GeneratorFunc>> = vec![
            Arc::new(Word(&EFF_WORDLIST)),
            Arc::new(Words(&EFF_WORDLIST)),
        ];
        Context(generators.into_iter().map(|g| (g.name(), g)).collect())
    }
}

fn fmt_with_hash(f: &mut fmt::Formatter<'_>, hash: &[u8; 32], args: &[&str]) -> fmt::Result {
    if !args.iter().copied().any(|arg| {
        let mut out = vec![0u8; 32];
        let Ok(()) = hex::decode_to_slice(arg, &mut out) else {
            return false;
        };
        out == hash
    }) {
        let mut out = vec![0u8; 64];
        hex::encode_to_slice(hash, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        write!(f, ":{out}")?;
    };
    for &arg in args {
        write!(f, ":{arg}")?;
    }
    Ok(())
}

impl GeneratorFunc for Word<'_, '_> {
    fn name(&self) -> &'static str {
        "word"
    }

    fn size(&self, args: &[&str]) -> NonZero<U256> {
        // TODO(soon): dict hash checking
        let _ = args;
        NonZero::new(_Word::try_from(self.0.words().len()).unwrap().into()).unwrap()
    }

    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>, args: &[&str]) -> Result<()> {
        // TODO(soon): case transformations
        let _ = args;
        write!(w, "{}", self.0.words()[u256_to_word(&index) as usize])
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>, args: &[&str]) -> fmt::Result {
        write!(f, "{}", self.name())?;
        fmt_with_hash(f, self.0.hash(), args)
    }
}

impl Words<'_, '_> {
    pub fn parse_args<'a>(args: &'_ [&'a str]) -> (u32, &'a str) {
        let mut count = 5;
        let mut sep = " ";
        for &arg in args {
            if let Some(c) = arg.chars().next() {
                if c.is_ascii_digit()
                    && let Ok(n) = arg.parse()
                {
                    count = n;
                } else if arg.len() == 1 && c.is_ascii_punctuation() {
                    sep = arg;
                }
            }
        }
        assert!(count > 0);
        (count, sep)
    }
}

impl GeneratorFunc for Words<'_, '_> {
    fn name(&self) -> &'static str {
        "words"
    }

    fn size(&self, args: &[&str]) -> NonZero<U256> {
        let (count, _) = Self::parse_args(args);
        // TODO(soon): hash checking
        let base = Word(self.0).size(&[]);
        NonZero::new(u256_saturating_pow(&base, count.into())).unwrap()
    }

    fn write_to(&self, w: &mut dyn Write, mut index: Zeroizing<U256>, args: &[&str]) -> Result<()> {
        let (count, sep) = Self::parse_args(args);
        // TODO(soon): hash checking
        let base = Word(self.0).size(&[]);
        for i in 0..count {
            if i != 0 {
                write!(w, "{sep}")?;
            }
            let word_index;
            let (a, b) = index.div_rem(&base);
            (index, word_index) = (Zeroizing::new(a), Zeroizing::new(b));
            // TODO(soon): case transforms
            Word(self.0).write_to(w, word_index, &[])?;
        }
        assert!(bool::from(index.is_zero()));
        Ok(())
    }

    fn fmt(&self, f: &mut fmt::Formatter<'_>, args: &[&str]) -> fmt::Result {
        write!(f, "{}", self.name())?;
        fmt_with_hash(f, self.0.hash(), args)
    }
}

impl PartialEq for Generator {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

#[cfg(test)]
mod tests {
    use super::{super::util::*, *};

    #[test]
    fn test_generators() {
        let ctx = Context::default();
        let g = Generator::new("word");
        assert_eq!(U256::from_u32(7776), *g.size(&ctx));
        assert_eq!("abacus", &format_at_ctx(&g, &ctx, U256::from_u32(0)));
        assert_eq!("zoom", &format_at_ctx(&g, &ctx, U256::from_u32(7775)));

        let g = Generator::new("words:4:-");
        assert_eq!(U256::from_u64(0xCFD41B9100000), *g.size(&ctx));
        assert_eq!(
            "abacus-abacus-abacus-abacus",
            &format_at_ctx(&g, &ctx, U256::from_u32(0))
        );
        assert_eq!(
            "zoom-zoom-zoom-zoom",
            &format_at_ctx(&g, &ctx, U256::from_u64(0xCFD41B90FFFFF))
        );
    }
}
