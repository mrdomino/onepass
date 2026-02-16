use core::fmt;
use std::{io, sync::Arc};

use crypto_bigint::{NonZero, U256, Word as _Word};
use onepass_base::dict::Dict;
use zeroize::Zeroizing;

use super::{
    EvalContext,
    context::Context,
    repr::write_literal,
    util::{u256_saturating_pow, u256_to_word},
};
use crate::dict::EFF_WORDLIST;

pub trait GeneratorFunc: Send + Sync {
    fn name(&self) -> &'static str;

    // TODO(soon): return Result from size so we can report dict lookup failure
    fn size(&self, context: &Context<'_>, args: &[&str]) -> NonZero<U256>;

    fn write_to(
        &self,
        context: &Context<'_>,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()>;

    /// `GeneratorFunc`s know how to format themselves, which they may use to e.g. inject
    /// dictionary hashes for canonical serialization.
    // TODO(someday): standardize `write_sep_arg`, and instead have an optional trait method that
    // yields each argument.
    fn write_repr(&self, _: &Context<'_>, w: &mut dyn fmt::Write, args: &[&str]) -> fmt::Result {
        write!(w, "{}", self.name())?;
        for &arg in args {
            write_sep_arg(w, arg)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Generator(Box<str>);

pub struct Word;

pub struct Words;

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
        context
            .get_generator(self.name())
            .unwrap()
            .size(context, &self.args())
    }

    fn write_to(
        &self,
        context: &Context,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
    ) -> io::Result<()> {
        context
            .get_generator(self.name())
            .unwrap()
            .write_to(context, w, index, &self.args())
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
}

impl<'a> Context<'a> {
    // TODO(soon): remove
    pub fn with_dict(dict: Arc<dyn Dict + 'a>) -> Self {
        Context::default().with_default_dict(dict)
    }
}

impl Default for Context<'_> {
    fn default() -> Self {
        let generators: Vec<Arc<dyn GeneratorFunc>> = vec![Arc::new(Word), Arc::new(Words)];
        Context::new(generators, [], Arc::new(EFF_WORDLIST))
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

impl GeneratorFunc for Word {
    fn name(&self) -> &'static str {
        "word"
    }

    fn size(&self, context: &Context<'_>, args: &[&str]) -> NonZero<U256> {
        let dict = context.get_dict(&Context::dict_hash(args)).unwrap();
        NonZero::new(_Word::try_from(dict.words().len()).unwrap().into()).unwrap()
    }

    fn write_to(
        &self,
        context: &Context<'_>,
        w: &mut dyn io::Write,
        index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()> {
        let dict = context.get_dict(&Context::dict_hash(args)).unwrap();
        let upper = args.iter().copied().any(|s| s == "U");
        let word = dict.words()[u256_to_word(&index) as usize];
        if !upper {
            write!(w, "{word}")?;
            return Ok(());
        }
        let mut iter = word.chars();
        let first = iter.next().unwrap();
        write!(w, "{}", first.to_uppercase())?;
        for c in iter {
            write!(w, "{c}")?;
        }
        Ok(())
    }

    fn write_repr(
        &self,
        context: &Context<'_>,
        w: &mut dyn fmt::Write,
        args: &[&str],
    ) -> fmt::Result {
        // TODO(soon): clean up
        let hash = Context::dict_hash(args).unwrap_or_else(|| *context.default_dict.hash());
        write!(w, "{}", self.name())?;
        fmt_with_hash(w, &hash, args)
    }
}

impl Words {
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

impl GeneratorFunc for Words {
    fn name(&self) -> &'static str {
        "words"
    }

    fn size(&self, context: &Context<'_>, args: &[&str]) -> NonZero<U256> {
        let (count, _, upper) = Self::parse_args(args);
        let base = Word.size(context, args);
        let mut n = u256_saturating_pow(&base, count.into());
        if upper {
            n = n.saturating_mul(&U256::from_u32(count));
        }
        NonZero::new(n).unwrap()
    }

    fn write_to(
        &self,
        context: &Context<'_>,
        w: &mut dyn io::Write,
        mut index: Zeroizing<U256>,
        args: &[&str],
    ) -> io::Result<()> {
        let (count, sep, upper) = Self::parse_args(args);
        // TODO(soon): better Words -> Word arg mapping
        let base = Word.size(context, args);
        let j;
        if upper {
            let j_uint;
            (*index, j_uint) = index.div_rem(&NonZero::new(U256::from_u32(count)).unwrap());
            j = u32::try_from(u256_to_word(&j_uint)).unwrap();
        } else {
            j = 0;
        }
        for i in 0..count {
            if i != 0 {
                write!(w, "{sep}")?;
            }
            let word_index;
            let (a, b) = index.div_rem(&base);
            (index, word_index) = (Zeroizing::new(a), Zeroizing::new(b));
            let args: &[&str] = if upper && i == j { &["U"] } else { &[] };
            Word.write_to(context, w, word_index, args)?;
        }
        assert!(bool::from(index.is_zero()));
        Ok(())
    }

    fn write_repr(
        &self,
        context: &Context<'_>,
        w: &mut dyn fmt::Write,
        args: &[&str],
    ) -> fmt::Result {
        let hash = Context::dict_hash(args).unwrap_or_else(|| *context.default_dict.hash());
        write!(w, "{}", self.name())?;
        fmt_with_hash(w, &hash, args)
    }
}

impl<'a> fmt::Debug for dyn GeneratorFunc + 'a {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(soon): represent args, context
        write!(f, "GeneratorFunc({:?})", self.name())?;
        Ok(())
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
            (
                "words:2:U",
                0x7354800,
                &[
                    ("Abacus abacus", 0),
                    ("abacus Abacus", 1),
                    ("Abdomen abacus", 2),
                    ("abdomen Abacus", 3),
                    ("Zoom zoom", 0x73547fe),
                    ("zoom Zoom", 0x73547ff),
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
    fn test_hashes() {
        let mut ctx = Context::default();
        let dict_a = Arc::new(BoxDict::from_lines("a\nb"));
        let dict_b = Arc::new(BoxDict::from_lines("c\nd"));
        ctx.extend([dict_a as Arc<dyn Dict>, dict_b]);
        let ctx = ctx;
        let a =
            Generator::new("word|e622f861cfb90d7fc2773ebf739fd5331515e652d2d3bad8d5a24ec90bf505fd");
        let b =
            Generator::new("word|ca492d04b5ed9cb47f4405591bb0ca14f5cdf0e45ea86a1d38466e8965e9abb2");
        assert_eq!("a", &format_at_ctx(&a, &ctx, U256::ZERO));
        assert_eq!("c", &format_at_ctx(&b, &ctx, U256::ZERO));
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
        let dict = Arc::new(BoxDict::from_lines(&s));
        let ctx = Context::with_dict(dict);
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
