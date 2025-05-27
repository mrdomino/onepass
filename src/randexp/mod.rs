use std::cmp;

use anyhow::Result;
use nom::{
    IResult, Parser,
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
use num_bigint::BigUint;
use num_traits::{One, Zero};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Expr {
    Word,
    Literal(String),
    CharClass(CharClass),
    Sequence(Vec<Expr>),
    Repeat(Box<Expr>, u32, u32),
}

impl Expr {
    pub fn parse(input: &str) -> IResult<&str, Expr> {
        let (input, exprs) = many1(Expr::parse_repeat).parse(input)?;
        if exprs.len() == 1 {
            return Ok((input, exprs.into_iter().next().unwrap()));
        }
        Ok((input, Expr::Sequence(exprs)))
    }

    pub fn gen_at_index<T: AsRef<str>>(&self, words: &[T], mut index: BigUint) -> Result<String> {
        let word_count = words.len() as u32;
        let res = match self {
            Expr::Word => words[usize::try_from(index).unwrap()].as_ref().into(),
            Expr::Literal(s) => s.clone(),

            Expr::CharClass(cc) => {
                for CharRange { start, end } in &cc.ranges {
                    let mut it = char_iter::new(*start, *end);
                    let n = it.len().into();
                    if index < n {
                        return Ok(it.nth(usize::try_from(index).unwrap()).unwrap().into());
                    }
                    index -= n;
                }
                anyhow::bail!("index too big");
            },

            Expr::Sequence(exprs) => {
                let mut acc = Vec::new();
                for expr in exprs {
                    let sz = expr.size(word_count);
                    acc.push(expr.gen_at_index(words, &index % &sz)?);
                    index /= sz;
                }
                acc.concat()
            },

            Expr::Repeat(expr, min, max) => {
                let mut acc = Vec::new();
                let base_size = expr.size(word_count);
                for i in (*max..=*min).rev() {
                    let n = base_size.pow(i);
                    if index < n {
                        for _ in 0..i {
                            acc.push(expr.gen_at_index(words, &index % &base_size)?);
                            index /= &base_size;
                        }
                        return Ok(acc.concat());
                    }
                    index -= n;
                }
                anyhow::bail!("index too big");
            }
        };
        Ok(res)
    }

    pub fn size(&self, word_count: u32) -> BigUint {
        match self {
            Expr::Word => word_count.into(),
            Expr::Literal(_) => BigUint::one(),

            Expr::CharClass(cc) => cc
                .ranges
                .iter()
                .map(|CharRange { start, end }| char_iter::new(*start, *end).len())
                .sum(),

            Expr::Sequence(exprs) => exprs
                .iter()
                .fold(BigUint::one(), |acc, expr| acc * expr.size(word_count)),

            Expr::Repeat(expr, min, max) => {
                let base_size = expr.size(word_count);
                (*min..=*max).fold(BigUint::zero(), |mut acc, i| {
                    acc += base_size.pow(i);
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
        delimited(char('('), |input| Expr::parse(input), char(')')).parse(input)
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
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use num_traits::Num;

    use super::*;

    #[test]
    fn word() -> Result<()> {
        let (input, expr) = Expr::parse("[:word:]")?;
        assert_eq!("", input);
        assert_eq!(Expr::Word, expr);
        Ok(())
    }

    #[test]
    fn literal() -> Result<()> {
        let (input, expr) = Expr::parse("some literal")?;
        assert_eq!("", input);
        assert_eq!(Expr::Literal("some literal".into()), expr);
        Ok(())
    }

    #[test]
    fn char_classes() -> Result<()> {
        let (input, expr) = Expr::parse("[A-Za-z0123-9]")?;
        assert_eq!("", input);
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
            let (input, expr) = Expr::parse(test.1)?;
            assert_eq!("", input);
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
        let (input, expr) = Expr::parse("[:word:](-[:word:]){4}")?;
        assert_eq!("", input);
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
        let (input, expr) = Expr::parse("a{3,5}")?;
        assert_eq!("", input);
        assert_eq!(
            Expr::Repeat(Box::new(Expr::Literal("a".into())), 3, 5),
            expr
        );
        Ok(())
    }

    #[test]
    fn enumerate_full() -> Result<()> {
        let (input, expr) = Expr::parse("[123][:word:]")?;
        assert_eq!("", input);
        let sz = expr.size(2);
        assert_eq!(BigUint::from(6u32), sz);
            let words = vec!["a", "b"];
        let strs: Vec<_> = (0u32..6).map(|i| expr.gen_at_index(&words, BigUint::from(i)).unwrap()).collect();
        assert_eq!(vec!["1a", "2a", "3a", "1b", "2b", "3b"], strs);
        Ok(())
    }

    #[test]
    fn enumerate_passphrase() -> Result<()> {
        let words: Vec<_> = (0..7776).map(|i| format!("({i})")).collect();
        let (input, expr) = Expr::parse("[:word:](-[:word:]){4}")?;
        assert_eq!("", input);
        let sz = BigUint::from_str_radix("28430288029929701376", 10)?;
        assert_eq!(sz, expr.size(words.len() as u32));
        assert_eq!("(0)-(0)-(0)-(0)-(0)", expr.gen_at_index(&words, BigUint::zero())?);
        assert_eq!("(7775)-(7775)-(7775)-(7775)-(7775)", expr.gen_at_index(&words, sz - 1u32)?);
        Ok(())
    }
}
