pub(crate) mod quantifiable;

use std::cmp;

use anyhow::{Context, Result};
use crypto_bigint::{NonZero, U256};
use nom::{
    Finish, IResult, Input, Parser,
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
use quantifiable::{Enumerable, Quantifiable};
use zeroize::Zeroizing;

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
    WOrd,
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
    let mut base = Zeroizing::new(*base);
    while exp > 0 {
        if exp & 1 == 1 {
            res = res.saturating_mul(&base);
        }
        exp >>= 1;
        *base = base.saturating_mul(&base);
    }
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

    fn parse_word(input: &str) -> IResult<&str, Expr> {
        alt((
            value(Expr::Word, tag("[:word:]")),
            value(Expr::WOrd, tag("[:Word:]")),
        )).parse(input)
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

pub(crate) struct WordCount(pub usize);

impl Quantifiable<Expr> for WordCount {
    fn size(&self, expr: &Expr) -> U256 {
        match expr {
            Expr::Word | Expr::WOrd => U256::from(self.0 as u64),
            Expr::Literal(_) => U256::ONE,

            Expr::CharClass(cc) => cc
                .ranges
                .iter()
                .map(|CharRange { start, end }| char_iter::new(*start, *end).len() as u32)
                .fold(U256::ZERO, |a, b| a.saturating_add(&U256::from(b))),

            Expr::Sequence(exprs) => exprs
                .iter()
                .fold(U256::ONE, |acc, expr| acc * self.size(expr)),

            Expr::Repeat(expr, min, max) => {
                let base_size = self.size(expr);
                (*min..=*max).fold(U256::ZERO, |mut acc, i| {
                    acc = acc.saturating_add(&u256_saturating_pow(&base_size, i));
                    acc
                })
            }
        }
    }
}

pub(crate) struct WordList<'a, T: AsRef<str>>(pub &'a [T]);

impl<T: AsRef<str>> Quantifiable<Expr> for WordList<'_, T> {
    fn size(&self, node: &Expr) -> U256 {
        WordCount(self.0.len()).size(node)
    }
}

impl<T: AsRef<str>> Enumerable<Expr> for WordList<'_, T> {
    fn gen_at(&self, expr: &Expr, index: U256) -> Result<Zeroizing<String>> {
        let mut index = Zeroizing::new(index);
        let res = match expr {
            Expr::Word => String::from(self.0[u256_to_usize(&index)].as_ref()),
            Expr::WOrd => {
                let mut chars = self.0[u256_to_usize(&index)].as_ref().chars();
                let first = chars.next().context("empty word")?.to_uppercase();
                first.chain(chars).collect()
            }
            Expr::Literal(s) => s.clone(),

            Expr::CharClass(cc) => {
                for CharRange { start, end } in &cc.ranges {
                    let mut it = char_iter::new(*start, *end);
                    let n = Zeroizing::new(U256::from(it.len() as u32));
                    if *index < *n {
                        let c = it.nth(u256_to_usize(&index)).unwrap();
                        // "zeroize" it...
                        while it.next().is_some() {}
                        return Ok(Zeroizing::new(c.into()));
                    }
                    *index -= *n;
                }
                anyhow::bail!("index too big");
            }

            Expr::Sequence(exprs) => {
                let mut acc = Zeroizing::new(Vec::with_capacity(exprs.len()));
                for expr in exprs {
                    let sz = NonZero::new(self.size(expr)).unwrap();
                    let (next_index, j) = index.div_rem(&sz);
                    let (mut next_index, j) = (Zeroizing::new(next_index), Zeroizing::new(j));
                    acc.push(self.gen_at(expr, *j)?);
                    std::mem::swap(&mut index, &mut next_index);
                }
                let n: usize = acc.iter().map(|s| s.len()).sum();
                let mut ret = String::with_capacity(n);
                for s in acc.iter() {
                    ret.extend(s.as_str().iter_elements());
                }
                ret
            }

            Expr::Repeat(expr, min, max) => {
                let base_size = Zeroizing::new(NonZero::new(self.size(expr)).unwrap());
                for i in (*min..=*max).rev() {
                    let n = Zeroizing::new(u256_saturating_pow(&base_size, i));
                    if *index < *n {
                        let mut acc = Zeroizing::new(Vec::with_capacity(i as usize));
                        for _ in 0..i {
                            let (next_index, j) = index.div_rem(&base_size);
                            let (mut next_index, j) =
                                (Zeroizing::new(next_index), Zeroizing::new(j));
                            acc.push(self.gen_at(expr, *j)?);
                            std::mem::swap(&mut index, &mut next_index);
                        }
                        let n: usize = acc.iter().map(|s| s.len()).sum();
                        let mut ret = Zeroizing::new(String::with_capacity(n));
                        for s in acc.iter() {
                            ret.extend(s.as_str().iter_elements());
                        }
                        return Ok(ret);
                    }
                    *index -= *n;
                }
                anyhow::bail!("index too big");
            }
        };
        Ok(Zeroizing::new(res))
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
        let sz = WordCount(2).size(&expr);
        assert_eq!(U256::from(6u32), sz);
        let words = vec!["a", "b"];
        let wl = WordList(&words);
        let strs: Vec<_> = (0u32..6)
            .map(|i| wl.gen_at(&expr, i.into()).unwrap())
            .collect();
        assert_eq!(
            vec!["1a", "2a", "3a", "1b", "2b", "3b"]
                .into_iter()
                .map(|s| Zeroizing::new(String::from(s)))
                .collect::<Vec<_>>(),
            strs
        );
        Ok(())
    }

    #[test]
    fn enumerate_passphrase() -> Result<()> {
        let words: Vec<_> = (0..7776).map(|i| format!("({i})")).collect();
        let wl = WordList(&words);
        let expr = Expr::parse("[:word:](-[:word:]){4}")?;
        let sz = U256::from_str_radix("28430288029929701376", 10)?;
        assert_eq!(sz, wl.size(&expr));
        assert_eq!("(0)-(0)-(0)-(0)-(0)", *wl.gen_at(&expr, U256::ZERO)?);
        assert_eq!(
            "(7775)-(7775)-(7775)-(7775)-(7775)",
            *wl.gen_at(&expr, sz.checked_sub(&U256::ONE).unwrap())?
        );
        Ok(())
    }

    #[test]
    fn enumerate_uppercase() -> Result<()> {
        let words = vec!["bob", "dole"];
        let wl = WordList(&words);
        let expr = Expr::parse("[:Word:]")?;
        assert_eq!("Bob", *wl.gen_at(&expr, U256::ZERO)?);
        Ok(())
    }
}
