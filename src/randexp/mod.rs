use std::cmp;

use anyhow::Result;
use crypto_bigint::{NonZero, U256, zeroize::Zeroize};
use nom::{
    Finish, IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::{
        complete::{self, char, none_of},
        one_of,
    },
    combinator::{fail, map, opt, value},
    multi::many1,
    sequence::{delimited, preceded, separated_pair},
};
use num_traits::{One, Zero};

/// Expr represents a subset of regular expressions that allows for literal strings, character
/// classes, sequences, and counts. It also has a concept of a "Word", which is equivalent to a
/// group containing a literal for each word in a dictionary, with the dictionary suppliable at
/// execute time.
///
/// This language subset is intended for use in password schemas; it allows the universe of strings
/// matching the language to be mapped to a U256, producing a unique (assuming the language does
/// not have multiple valid ways of recognizing a given string) string for each different number in
/// the half-open interval `[0, expr.size())`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Expr {
    Word,
    Literal(String),
    CharClass(CharClass),
    Sequence(Vec<Expr>),
    Repeat(Box<Expr>, u32, u32),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CharRange {
    start: char,
    end: char,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CharClass {
    ranges: Vec<CharRange>,
}

impl CharRange {
    fn try_merge(&self, other: &CharRange) -> Option<CharRange> {
        let self_start = self.start as u32;
        let self_end = self.end as u32;
        let other_start = other.start as u32;
        let other_end = other.end as u32;

        if self_end + 1 >= other_start && other_end + 1 >= self_start {
            let start = char::from_u32(cmp::min(self_start, other_start)).unwrap();
            let end = char::from_u32(cmp::max(self_end, other_end)).unwrap();
            Some(CharRange { start, end })
        } else {
            None
        }
    }
}

impl CharClass {
    fn from_ranges(mut input_ranges: Vec<CharRange>) -> Self {
        if input_ranges.is_empty() {
            return CharClass { ranges: Vec::new() };
        }
        input_ranges.sort_by_key(|r| r.start);
        let mut ranges = Vec::new();
        let mut current = input_ranges[0].clone();
        for range in input_ranges {
            if let Some(merged) = current.try_merge(&range) {
                current = merged;
            } else {
                ranges.push(current);
                current = range;
            }
        }
        ranges.push(current);
        CharClass { ranges }
    }
}

fn u256_to_usize(n: &U256) -> usize {
    n.as_limbs()[0].0 as usize
}

fn u256_saturating_pow(base: &U256, mut exp: u32) -> U256 {
    let mut res = U256::ONE;
    if exp == 0 {
        return res;
    }
    let mut base = *base;
    while exp > 0 {
        if exp & 1 == 1 {
            res = res.saturating_mul(&base);
        }
        exp >>= 1;
        base = base.saturating_mul(&base);
    }
    base.zeroize();
    res
}

impl Expr {
    pub fn parse(input: &str) -> Result<Self> {
        let (rem, expr) = Expr::parse_expr(input).finish().map_err(|e| {
            anyhow::anyhow!("Parse error at {}: {}", e.input.len(), e.code.description())
        })?;
        if !rem.is_empty() {
            anyhow::bail!("leftover input: {rem}");
        }
        Ok(expr)
    }

    pub fn gen_at_index<T: AsRef<str>>(&self, words: &[T], mut index: U256) -> Result<String> {
        let word_count = words.len() as u32;
        let res = match self {
            Expr::Word => words[u256_to_usize(&index)].as_ref().into(),
            Expr::Literal(s) => s.clone(),

            Expr::CharClass(cc) => {
                for CharRange { start, end } in &cc.ranges {
                    let mut it = char_iter::new(*start, *end);
                    let mut n = U256::from(it.len() as u32);
                    if index < n {
                        return Ok(it.nth(u256_to_usize(&index)).unwrap().into());
                    }
                    index -= n;
                    n.zeroize();
                }
                anyhow::bail!("index too big");
            }

            Expr::Sequence(exprs) => {
                let mut acc = Vec::new();
                for expr in exprs {
                    let sz = NonZero::new(expr.size(word_count)).unwrap();
                    let (next_index, j) = index.div_rem(&sz);
                    acc.push(expr.gen_at_index(words, j)?);
                    index = next_index;
                }
                // XXX zeroize?
                acc.concat()
            }

            Expr::Repeat(expr, min, max) => {
                let mut acc = Vec::new();
                let mut base_size = NonZero::new(expr.size(word_count)).unwrap();
                for i in (*min..=*max).rev() {
                    let mut n = u256_saturating_pow(&base_size, i);
                    if index < n || i == *min {
                        for _ in 0..i {
                            let (next_index, j) = index.div_rem(&base_size);
                            acc.push(expr.gen_at_index(words, j)?);
                            index = next_index;
                        }
                        n.zeroize();
                        base_size.zeroize();
                        // XXX zeroize?
                        return Ok(acc.concat());
                    }
                    index -= n;
                    n.zeroize();
                }
                base_size.zeroize();
                anyhow::bail!("index too big");
            }
        };
        Ok(res)
    }

    pub fn size(&self, word_count: u32) -> U256 {
        match self {
            Expr::Word => word_count.into(),
            Expr::Literal(_) => U256::one(),

            Expr::CharClass(cc) => cc
                .ranges
                .iter()
                .map(|CharRange { start, end }| char_iter::new(*start, *end).len() as u32)
                .fold(U256::ZERO, |a, b| a.saturating_add(&U256::from(b))),

            Expr::Sequence(exprs) => exprs
                .iter()
                .fold(U256::one(), |acc, expr| acc * expr.size(word_count)),

            Expr::Repeat(expr, min, max) => {
                let base_size = expr.size(word_count);
                (*min..=*max).fold(U256::zero(), |mut acc, i| {
                    acc = acc.saturating_add(&u256_saturating_pow(&base_size, i));
                    acc
                })
            }
        }
    }

    fn parse_word(input: &str) -> IResult<&str, Expr> {
        value(Expr::Word, tag("[:word:]")).parse(input)
    }

    fn parse_literal(input: &str) -> IResult<&str, Expr> {
        let (input, res) = many1(alt((
            preceded(char('\\'), one_of("nrt[]{}()|\\")),
            none_of("[]{}()|\\"),
        )))
        .parse(input)?;
        Ok((input, Expr::Literal(res.into_iter().collect())))
    }

    fn parse_special_range(input: &str) -> IResult<&str, Expr> {
        let (input, ranges) = preceded(
            char('\\'),
            alt((
                value(vec![('0', '9')], char('d')),
                value(vec![('0', '9'), ('a', 'z'), ('A', 'Z')], char('w')),
            )),
        )
        .parse(input)?;
        Ok((
            input,
            Expr::CharClass(CharClass::from_ranges(
                ranges
                    .into_iter()
                    .map(|(start, end)| CharRange { start, end })
                    .collect(),
            )),
        ))
    }

    fn parse_char_class_inner(input: &str) -> IResult<&str, CharClass> {
        let (input, negated) = map(opt(char('^')), |x| x.is_some()).parse(input)?;
        if negated {
            let (input, only_caret) = map(opt(char(']')), |x| x.is_some()).parse(input)?;
            if only_caret {
                return Ok((
                    input,
                    CharClass::from_ranges(vec![CharRange {
                        start: '^',
                        end: '^',
                    }]),
                ));
            }
            // TODO? negated classes
            return fail().parse(input);
        }
        let (input, rs) = many1(alt((
            separated_pair(none_of("\\]"), char('-'), none_of("\\]")),
            map(none_of("\\]"), |c| (c, c)),
        )))
        .parse(input)?;
        let rs = rs
            .into_iter()
            .map(|(start, end)| CharRange { start, end })
            .collect();
        Ok((input, CharClass::from_ranges(rs)))
    }

    fn parse_char_class(input: &str) -> IResult<&str, Expr> {
        let (input, cc) =
            delimited(char('['), Expr::parse_char_class_inner, char(']')).parse(input)?;
        Ok((input, Expr::CharClass(cc)))
    }

    fn parse_group(input: &str) -> IResult<&str, Expr> {
        delimited(char('('), |input| Expr::parse_expr(input), char(')')).parse(input)
    }

    fn parse_basic_expr(input: &str) -> IResult<&str, Expr> {
        alt((
            Expr::parse_word,
            Expr::parse_literal,
            Expr::parse_special_range,
            Expr::parse_char_class,
            Expr::parse_group,
        ))
        .parse(input)
    }

    fn parse_repeat(input: &str) -> IResult<&str, Expr> {
        let (input, expr) = Expr::parse_basic_expr(input)?;
        let (input, count) = opt(delimited(
            char('{'),
            alt((
                separated_pair(complete::u32, char(','), complete::u32),
                map(complete::u32, |n| (n, n)),
            )),
            char('}'),
        ))
        .parse(input)?;
        if let Some((min, max)) = count {
            return Ok((input, Expr::Repeat(Box::new(expr), min, max)));
        }
        Ok((input, expr))
    }

    fn parse_expr(input: &str) -> IResult<&str, Expr> {
        let (input, exprs) = many1(Expr::parse_repeat).parse(input)?;
        if exprs.len() == 1 {
            return Ok((input, exprs.into_iter().next().unwrap()));
        }
        Ok((input, Expr::Sequence(exprs)))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use crypto_bigint::CheckedSub;
    use num_traits::Num;

    use super::*;

    #[test]
    fn word() -> Result<()> {
        let expr = Expr::parse("[:word:]")?;
        assert_eq!(Expr::Word, expr);
        Ok(())
    }

    #[test]
    fn literal() -> Result<()> {
        let expr = Expr::parse("some literal")?;
        assert_eq!(Expr::Literal("some literal".into()), expr);
        Ok(())
    }

    #[test]
    fn char_classes() -> Result<()> {
        let expr = Expr::parse("[A-Za-z0123-9]")?;
        assert_eq!(
            Expr::CharClass(CharClass {
                ranges: vec![
                    CharRange {
                        start: '0',
                        end: '9'
                    },
                    CharRange {
                        start: 'A',
                        end: 'Z'
                    },
                    CharRange {
                        start: 'a',
                        end: 'z'
                    }
                ]
            }),
            expr
        );
        Ok(())
    }

    #[test]
    fn char_class_table() -> Result<()> {
        let tests = vec![
            (vec![('A', 'Z')], "[A-MD-Z]"),
            (vec![('A', 'Z')], "[D-ZA-M]"),
            (vec![('a', 'j')], "[a-cb-ea-fb-j]"),
            (vec![('a', 'a'), ('c', 'c')], "[ac]"),
        ];
        for test in tests {
            let expr = Expr::parse(test.1)?;
            let ranges = test
                .0
                .into_iter()
                .map(|(start, end)| CharRange { start, end })
                .collect();
            let expected = Expr::CharClass(CharClass { ranges });
            assert_eq!(expected, expr);
        }
        Ok(())
    }

    #[test]
    fn groups_repeat() -> Result<()> {
        let expr = Expr::parse("[:word:](-[:word:]){4}")?;
        assert_eq!(
            Expr::Sequence(vec![
                Expr::Word,
                Expr::Repeat(
                    Box::new(Expr::Sequence(vec![Expr::Literal("-".into()), Expr::Word])),
                    4,
                    4
                )
            ]),
            expr
        );
        Ok(())
    }

    #[test]
    fn multi_group() -> Result<()> {
        let expr = Expr::parse("a{3,5}")?;
        assert_eq!(
            Expr::Repeat(Box::new(Expr::Literal("a".into())), 3, 5),
            expr
        );
        Ok(())
    }

    #[test]
    fn enumerate_full() -> Result<()> {
        let expr = Expr::parse("[123][:word:]")?;
        let sz = expr.size(2);
        assert_eq!(U256::from(6u32), sz);
        let words = vec!["a", "b"];
        let strs: Vec<_> = (0u32..6)
            .map(|i| expr.gen_at_index(&words, U256::from(i)).unwrap())
            .collect();
        assert_eq!(vec!["1a", "2a", "3a", "1b", "2b", "3b"], strs);
        Ok(())
    }

    #[test]
    fn enumerate_passphrase() -> Result<()> {
        let words: Vec<_> = (0..7776).map(|i| format!("({i})")).collect();
        let expr = Expr::parse("[:word:](-[:word:]){4}")?;
        let sz = U256::from_str_radix("28430288029929701376", 10)?;
        assert_eq!(sz, expr.size(words.len() as u32));
        assert_eq!(
            "(0)-(0)-(0)-(0)-(0)",
            expr.gen_at_index(&words, U256::zero())?
        );
        assert_eq!(
            "(7775)-(7775)-(7775)-(7775)-(7775)",
            expr.gen_at_index(&words, sz.checked_sub(&U256::from(1u8)).unwrap())?
        );
        Ok(())
    }
}
