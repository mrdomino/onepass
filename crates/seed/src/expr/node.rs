use std::io::{Result, Write};

use crypto_bigint::{CheckedSub, NonZero, One, U256, Word};
use zeroize::Zeroizing;

pub use super::generator::Context;
use super::{Eval, EvalContext, chars::Chars, generator::Generator, util::u256_saturating_pow};

#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Literal(Box<str>),
    Chars(Chars),
    List(Box<[Node]>),
    Count(Box<Node>, u32, u32),
    Generator(Generator),
}

impl EvalContext for Node {
    type Context<'a> = Context<'a>;

    fn size(&self, context: &Context) -> NonZero<U256> {
        match *self {
            Node::Literal(_) => NonZero::new(U256::ONE).unwrap(),
            Node::Chars(ref chars) => chars.size(),
            Node::List(ref nodes) => {
                NonZero::new(nodes.into_iter().fold(U256::ONE, |acc, node| {
                    acc.saturating_mul(&node.size(context))
                }))
                .unwrap()
            }

            Node::Count(ref node, min, max) => {
                let n = node.size(context);
                if n.is_one().into() {
                    return NonZero::new((max - min + 1).into()).unwrap();
                }
                // Closed form of n^k + … + n^l
                //              = n^k (1 + … + n^(l-k))
                //              = n^k (n^(l-k+1) - 1) / (n - 1)
                //              = (n^(l+1) - n^k) / (n - 1)
                let k = min;
                let l = max;
                let x = u256_saturating_pow(&n, (l + 1).into())
                    .checked_sub(&u256_saturating_pow(&n, Word::from(k)))
                    .unwrap();
                let (x, rem) = x.div_rem(&NonZero::new(n.saturating_sub(&U256::ONE)).unwrap());
                assert!(bool::from(rem.is_zero()));
                NonZero::new(x).unwrap()
            }

            Node::Generator(ref generator) => generator.size(context),
        }
    }

    fn write_to(&self, context: &Context, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        match *self {
            Node::Literal(ref s) => w.write_all(s.as_bytes()),
            Node::Chars(ref chars) => chars.write_to(w, index),

            Node::List(ref nodes) => nodes
                .into_iter()
                .try_fold(index, |mut index, node| {
                    let node_index;
                    let (a, b) = index.div_rem(&node.size(context));
                    (index, node_index) = (Zeroizing::new(a), Zeroizing::new(b));
                    node.write_to(context, w, node_index)?;
                    Ok(index)
                })
                .map(|_| ()),

            Node::Count(ref node, min, max) => {
                let mut index = index;
                let node = node.as_ref();
                let base = Zeroizing::new(node.size(context));
                let mut count = min;
                let mut n = Zeroizing::new(u256_saturating_pow(&base, Word::from(min)));
                while *n <= *index {
                    count += 1;
                    *index -= *n;
                    *n = n.saturating_mul(&base);
                }
                assert!(count <= max);
                for _ in 0..count {
                    let node_index;
                    let (a, b) = index.div_rem(&base);
                    (index, node_index) = (Zeroizing::new(a), Zeroizing::new(b));
                    node.write_to(context, w, node_index)?;
                }
                assert!(bool::from(index.is_zero()));
                Ok(())
            }

            Node::Generator(ref generator) => generator.write_to(context, w, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::util::*, *};

    use num_traits::PrimInt;

    #[test]
    fn test_counts() {
        let context = Context::empty();

        let tests = [
            ("a", 1, 1, 0, Some(1)),
            ("aa", 2, 2, 0, Some(1)),
            ("a", 1, 5, 0, Some(5)),
            ("aa", 1, 5, 1, None),
            ("aaaaa", 1, 5, 4, None),
        ];
        for (want, min, max, index, want_size) in tests {
            let prim = Node::Literal("a".into());
            let count = Node::Count(prim.into(), min, max);
            assert_eq!(
                want,
                &format_at_ctx(&count, &context, U256::from_u32(index))
            );
            if let Some(size) = want_size {
                assert_eq!(U256::from_u32(size), *count.size(&context));
            }
        }

        let tests = [
            ("a", 0),
            ("b", 1),
            ("aa", 26),
            ("ba", 27),
            ("zzzzz", 12356629),
        ];
        let prim = Node::Chars(Chars::from_ranges([('a', 'z')]));
        let count = Node::Count(prim.into(), 1, 5);
        assert_eq!(U256::from_u32(12356630), *count.size(&context));
        for (want, index) in tests {
            assert_eq!(
                want,
                &format_at_ctx(&count, &context, U256::from_u32(index))
            );
        }

        let tests = [
            ("aa", 0),
            ("ba", 1),
            ("za", 25),
            ("ab", 26),
            ("zz", 675),
            ("aaa", 676),
            ("zzzzz", 12356603),
        ];
        let prim = Node::Chars(Chars::from_ranges([('a', 'z')]));
        let count = Node::Count(prim.into(), 2, 5);
        assert_eq!(U256::from_u32(12356604), *count.size(&context));
        for (want, index) in tests {
            assert_eq!(
                want,
                &format_at_ctx(&count, &context, U256::from_u32(index))
            );
        }
    }

    #[test]
    fn test_lists() {
        let context = Context::empty();
        let prim = || Node::Chars(Chars::from_ranges([('a', 'z')]));
        let tests = [
            ("a", 1, 0),
            ("b", 1, 1),
            ("z", 1, 25),
            ("aa", 2, 0),
            ("ba", 2, 1),
            ("za", 2, 25),
            ("ab", 2, 26),
            ("zz", 2, 675),
            ("aaaaa", 5, 0),
        ];
        for (want, rep, index) in tests {
            let list = vec![prim(); rep];
            let node = Node::List(list.into());
            let size = 26.pow(rep as u32);
            assert_eq!(U256::from_u32(size), *node.size(&context));
            assert_eq!(want, &format_at_ctx(&node, &context, U256::from_u32(index)));
        }
    }

    #[test]
    fn test_generators() {
        let context = Context::default();
        let node = Node::Generator(Generator::new("word"));
        assert_eq!(U256::from_u32(7776), *node.size(&context));
        assert_eq!("abacus", &format_at_ctx(&node, &context, U256::ZERO));
        assert_eq!(
            "zoom",
            &format_at_ctx(&node, &context, U256::from_u32(7775))
        );
    }
}
