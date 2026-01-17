use core::fmt;
use std::{collections::HashMap, io, sync::Arc};

use crypto_bigint::{NonZero, U256, Word as _Word};
use onepass_base::dict::Dict;
use zeroize::Zeroizing;

use super::{
    EvalContext,
    repr::write_literal,
    util::{u256_saturating_pow, u256_to_word},
};
use crate::dict::EFF_WORDLIST;

pub trait GeneratorFunc: Send + Sync {
    fn name(&self) -> &'static str;
    fn size(&self, args: &[&str]) -> NonZero<U256>;
    fn write_to(
        &self,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()>;

    // `GeneratorFunc`s know how to format themselves, which they may use to e.g. inject dictionary
    // hashes for canonical serialization.
    fn write_repr(&self, w: &mut dyn fmt::Write, args: &[&str]) -> fmt::Result {
        write!(w, "{}", self.name())?;
        for &arg in args {
            write_sep_arg(w, arg)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Generator(Box<str>);

#[derive(Debug)]
pub struct Context<'a>(HashMap<&'static str, Arc<dyn GeneratorFunc + 'a>>);

// TODO(someday): multiple dict lookup by hash
pub struct Word<'a>(&'a dyn Dict);

pub struct Words<'a>(&'a dyn Dict);

fn write_sep_arg<W>(w: &mut W, arg: &str) -> fmt::Result
where
    W: fmt::Write + ?Sized,
{
    w.write_char('|')?;
    write_literal(w, arg)?;
    Ok(())
}

impl EvalContext for Generator {
    type Context<'a> = Context<'a>;

    fn size(&self, context: &Context) -> NonZero<U256> {
        self.func(context).size(&self.args())
    }

    fn write_to(
        &self,
        context: &Context,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
    ) -> io::Result<()> {
        self.func(context).write_to(w, index, &self.args())
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
        let n = self
            .0
            .find(|c: char| !c.is_ascii_lowercase())
            .unwrap_or(self.0.len());
        &self.0[..n]
    }

    pub fn args(&self) -> Box<[&str]> {
        let Some(sep) = self.0.chars().find(|&c| !c.is_ascii_lowercase()) else {
            return [].into();
        };
        self.0.split(sep).skip(1).collect()
    }

    fn func<'a>(&self, context: &'a Context) -> &'a dyn GeneratorFunc {
        let name = self.name();
        context
            .get(name)
            .ok_or_else(|| format!("unknown generator {name}"))
            .unwrap()
    }
}

impl<'a> Context<'a> {
    pub fn with_dict(dict: &'a dyn Dict) -> Self {
        let generators: Vec<Arc<dyn GeneratorFunc + 'a>> =
            vec![Arc::new(Word(dict)), Arc::new(Words(dict))];
        Self::from_iter(generators)
    }
}

impl Context<'_> {
    pub fn empty() -> Self {
        Context(HashMap::new())
    }

    pub fn get<'a>(&'a self, name: &str) -> Option<&'a dyn GeneratorFunc> {
        self.0.get(name).map(Arc::as_ref)
    }
}

impl Default for Context<'_> {
    fn default() -> Self {
        let generators: Vec<Arc<dyn GeneratorFunc>> = vec![
            Arc::new(Word(&EFF_WORDLIST)),
            Arc::new(Words(&EFF_WORDLIST)),
        ];
        Self::from_iter(generators)
    }
}

impl<'a> FromIterator<Arc<dyn GeneratorFunc + 'a>> for Context<'a> {
    fn from_iter<T: IntoIterator<Item = Arc<dyn GeneratorFunc + 'a>>>(iter: T) -> Self {
        Context(iter.into_iter().map(|g| (g.name(), g)).collect())
    }
}

impl<'a> Extend<Arc<dyn GeneratorFunc + 'a>> for Context<'a> {
    fn extend<T: IntoIterator<Item = Arc<dyn GeneratorFunc + 'a>>>(&mut self, iter: T) {
        self.0.extend(iter.into_iter().map(|g| (g.name(), g)));
    }
}

fn fmt_with_hash<W>(w: &mut W, hash: &[u8; 32], args: &[&str]) -> fmt::Result
where
    W: fmt::Write + ?Sized,
{
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
        write_sep_arg(w, &out)?;
    };
    for &arg in args {
        write_sep_arg(w, arg)?;
    }
    Ok(())
}

impl GeneratorFunc for Word<'_> {
    fn name(&self) -> &'static str {
        "word"
    }

    fn size(&self, args: &[&str]) -> NonZero<U256> {
        // TODO(soon): dict hash checking
        let _ = args;
        NonZero::new(_Word::try_from(self.0.words().len()).unwrap().into()).unwrap()
    }

    fn write_to(
        &self,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()> {
        let upper = args.iter().copied().any(|s| s == "U");
        if !upper {
            write!(w, "{}", self.0.words()[u256_to_word(&index) as usize])?;
            return Ok(());
        }
        let word = self.0.words()[u256_to_word(&index) as usize];
        let mut iter = word.chars();
        let first = iter.next().unwrap();
        write!(w, "{}", first.to_uppercase())?;
        for c in iter {
            write!(w, "{c}")?;
        }
        Ok(())
    }

    fn write_repr(&self, w: &mut dyn fmt::Write, args: &[&str]) -> fmt::Result {
        write!(w, "{}", self.name())?;
        fmt_with_hash(w, self.0.hash(), args)
    }
}

impl Words<'_> {
    pub fn parse_args<'a>(args: &'_ [&'a str]) -> (u32, &'a str, bool) {
        let mut count = 5;
        let mut sep = " ";
        let mut upper = false;
        for &arg in args {
            if let Some(c) = arg.chars().next() {
                if c.is_ascii_digit()
                    && let Ok(n) = arg.parse()
                {
                    count = n;
                } else if arg.len() == 1 {
                    if c.is_ascii_punctuation() {
                        sep = arg;
                    } else if c == 'U' {
                        upper = true;
                    }
                }
            } else {
                sep = "";
            }
        }
        assert!(count > 0);
        (count, sep, upper)
    }
}

impl GeneratorFunc for Words<'_> {
    fn name(&self) -> &'static str {
        "words"
    }

    fn size(&self, args: &[&str]) -> NonZero<U256> {
        let (count, _, _) = Self::parse_args(args);
        // TODO(soon): hash checking, arbitrary case transforms
        let base = Word(self.0).size(&[]);
        NonZero::new(u256_saturating_pow(&base, count.into())).unwrap()
    }

    fn write_to(
        &self,
        w: &mut dyn io::Write,
        mut index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()> {
        let (count, sep, upper) = Self::parse_args(args);
        // TODO(soon): hash checking
        let base = Word(self.0).size(&[]);
        for i in 0..count {
            if i != 0 {
                write!(w, "{sep}")?;
            }
            let word_index;
            let (a, b) = index.div_rem(&base);
            (index, word_index) = (Zeroizing::new(a), Zeroizing::new(b));
            let args: &[&str] = if upper && i == 0 { &["U"] } else { &[] };
            Word(self.0).write_to(w, word_index, args)?;
        }
        assert!(bool::from(index.is_zero()));
        Ok(())
    }

    fn write_repr(&self, w: &mut dyn fmt::Write, args: &[&str]) -> fmt::Result {
        write!(w, "{}", self.name())?;
        fmt_with_hash(w, self.0.hash(), args)
    }
}

impl PartialEq for Generator {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<'a> fmt::Debug for dyn GeneratorFunc + 'a {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GeneratorFunc(")?;
        self.write_repr(f, &[])?;
        write!(f, ")")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{Expr, Node, util::*},
        *,
    };
    use crate::dict::BoxDict;

    #[test]
    fn test_generators() {
        let ctx = Context::default();
        let tests: [(&str, u64, &[(&str, u64)]); _] = [
            ("word", 7776, &[("abacus", 0), ("zoom", 7775)]),
            (
                "words:4:-",
                0xCFD41B9100000,
                &[
                    ("abacus-abacus-abacus-abacus", 0),
                    ("abdomen-abacus-abacus-abacus", 1),
                    ("abacus-abdomen-abacus-abacus", 7776),
                    ("zoology-zoom-zoom-zoom", 0xCFD41B90FFFFE),
                    ("zoom-zoom-zoom-zoom", 0xCFD41B90FFFFF),
                ],
            ),
        ];
        for (g, sz, tt) in tests {
            let g = Generator::new(g);
            assert_eq!(U256::from_u64(sz), *g.size(&ctx));
            for (s, i) in tt {
                assert_eq!(s, &format_at_ctx(&g, &ctx, U256::from_u64(*i)));
            }
        }
    }

    #[test]
    fn test_case() {
        let ctx = Context::default();
        let g = Generator::new("word:U");
        assert_eq!("Abacus", &format_at_ctx(&g, &ctx, U256::ZERO));
        let g = Generator::new("words:U:3:");
        assert_eq!("Abacusabacusabacus", &format_at_ctx(&g, &ctx, U256::ZERO));
    }

    #[test]
    fn test_lifetimes() {
        let s = "bob\ndole".to_string();
        let dict = BoxDict::from_lines(&s);
        let ctx = Context::with_dict(&dict);
        let g = Generator::new("word");
        assert_eq!(U256::from_u32(2), *g.size(&ctx));
        assert_eq!("bob", &format_at_ctx(&g, &ctx, U256::from_u32(0)));
        assert_eq!("dole", &format_at_ctx(&g, &ctx, U256::from_u32(1)));
    }

    #[test]
    fn test_fmt() {
        let expr = Expr::new(Node::Generator(Generator::new("word")));
        assert_eq!(
            "{word|323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd}",
            &format!("{expr}"),
        );
        let expr = Expr::new(Node::Generator(Generator::new("word:up|:too")));
        assert_eq!(
            "{word|323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd|up\\||too}",
            &format!("{expr}"),
        );
        let expr = Expr::new(Node::Generator(Generator::new("word|up:|too")));
        assert_eq!(
            "{word|323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd|up:|too}",
            &format!("{expr}"),
        );
    }
}
