use std::io::{Result, Write};

use crypto_bigint::{CheckedSub, NonZero, One, U256, Word, Zero};
use zeroize::Zeroizing;

use super::{
    Eval,
    chars::Chars,
    generator::{Generator, GeneratorContext},
    util::u256_saturating_pow,
};

#[derive(Clone, Debug)]
pub enum Node {
    Literal(Box<str>),
    Chars(Chars),
    List(Box<[Node]>),
    Count(Box<Node>, u32, u32),
    Generator(Generator),
}

impl Eval for (&'_ Node, &'_ GeneratorContext) {
    fn size(&self) -> U256 {
        match self.0 {
            Node::Literal(_) => U256::ONE,
            Node::Chars(chars) => chars.size(),
            Node::List(nodes) => nodes.into_iter().fold(U256::ONE, |acc, node| {
                acc.saturating_mul(&(node, self.1).size())
            }),

            Node::Count(node, min, max) => {
                let n = (&**node, self.1).size();
                if n.is_one().into() {
                    return (*max - *min + 1).into();
                }
                // Closed form of n^k + … + n^l
                //              = n^k (1 + … + n^(l-k))
                //              = n^k (n^(l-k+1) - 1) / (n - 1)
                //              = (n^(l+1) - n^k) / (n - 1)
                let k = *min;
                let l = *max;
                let x = u256_saturating_pow(&n, (l + 1).into())
                    .checked_sub(&u256_saturating_pow(&n, Word::from(k)))
                    .unwrap();
                let (x, rem) = x.div_rem(&NonZero::new(n.saturating_sub(&U256::ONE)).unwrap());
                assert!(bool::from(rem.is_zero()));
                x
            }

            Node::Generator(generator) => (generator, self.1).size(),
        }
    }

    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        match self.0 {
            Node::Literal(s) => write!(w, "{s}"),
            Node::Chars(chars) => chars.write_to(w, index),

            Node::List(nodes) => nodes
                .into_iter()
                .try_fold(index, |mut index, node| {
                    let node_index;
                    let (a, b) = index.div_rem(&NonZero::new((node, self.1).size()).unwrap());
                    (index, node_index) = (Zeroizing::new(a), Zeroizing::new(b));
                    (node, self.1).write_to(w, node_index)?;
                    Ok(index)
                })
                .map(|_| ()),

            Node::Count(node, min, max) => {
                let mut index = index;
                let node = node.as_ref();
                let base = Zeroizing::new(NonZero::new((node, self.1).size()).unwrap());
                let mut count = *min;
                let mut n = Zeroizing::new(u256_saturating_pow(&base, Word::from(*min)));
                while *n <= *index {
                    count += 1;
                    *index -= *n;
                    *n = n.saturating_mul(&base);
                }
                assert!(count <= *max);
                for _ in 0..count {
                    let node_index;
                    let (a, b) = index.div_rem(&base);
                    (index, node_index) = (Zeroizing::new(a), Zeroizing::new(b));
                    (node, self.1).write_to(w, node_index)?;
                }
                assert!(bool::from(index.is_zero()));
                Ok(())
            }

            Node::Generator(generator) => (generator, self.1).write_to(w, index),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::BufWriter;

    use num_traits::PrimInt;

    use super::*;

    #[test]
    fn test_counts() {
        let context = GeneratorContext::default();

        let tests = [
            ("a", 1, 1, 0, Some(1)),
            ("aa", 2, 2, 0, Some(1)),
            ("a", 1, 5, 0, Some(5)),
            ("aa", 1, 5, 1, None),
            ("aaaaa", 1, 5, 4, None),
        ];
        for (want, min, max, index, want_size) in tests {
            let index = Zeroizing::new(U256::from_u32(index));
            let prim = Node::Literal("a".into());
            let count = Node::Count(prim.into(), min, max);
            let mut buf = BufWriter::new(Vec::new());
            (&count, &context).write_to(&mut buf, index).unwrap();
            let s = String::from_utf8(buf.into_inner().unwrap()).unwrap();
            assert_eq!(want, &s);
            if let Some(size) = want_size {
                assert_eq!(U256::from_u32(size), (&count, &context).size());
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
        assert_eq!(U256::from_u32(12356630), (&count, &context).size());
        for (want, index) in tests {
            let mut buf = BufWriter::new(Vec::new());
            (&count, &context)
                .write_to(&mut buf, U256::from_u32(index).into())
                .unwrap();
            let s = String::from_utf8(buf.into_inner().unwrap()).unwrap();
            assert_eq!(want, &s);
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
        assert_eq!(U256::from_u32(12356604), (&count, &context).size());
        for (want, index) in tests {
            let mut buf = BufWriter::new(Vec::new());
            (&count, &context)
                .write_to(&mut buf, U256::from_u32(index).into())
                .unwrap();
            let s = String::from_utf8(buf.into_inner().unwrap()).unwrap();
            assert_eq!(want, &s);
        }
    }

    #[test]
    fn test_lists() {
        let context = GeneratorContext::default();
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
            assert_eq!(U256::from_u32(size), (&node, &context).size());
            let mut buf = BufWriter::new(Vec::new());
            (&node, &context)
                .write_to(&mut buf, U256::from_u32(index).into())
                .unwrap();
            let s = String::from_utf8(buf.into_inner().unwrap()).unwrap();
            assert_eq!(want, &s);
        }
    }
}
