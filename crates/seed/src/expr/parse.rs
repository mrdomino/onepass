use core::str;
use std::str::Utf8Error;

use nom::{
    Finish, IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag, take_while_m_n},
    character::complete::{self, anychar, char, none_of},
    combinator::{map, map_res, opt, peek, value, verify},
    error::{self, ErrorKind},
    multi::{fold, many1},
    sequence::{delimited, preceded, separated_pair},
};

use crate::expr::{Context, Expr};

use super::{Node, chars::Chars, generator::Generator};

enum StringFragment<'a> {
    Verbatim(&'a str),
    Escaped(char),
}

enum CharFragment {
    Single((char, char)),
    Multi(&'static [(char, char)]),
}

pub type Error = nom::error::Error<String>;

impl Expr<'_> {
    /// Expressions can be parsed from UTF-8 strings.
    ///
    /// The following syntax is supported:
    ///
    /// # String literals
    /// Any literal string that does not otherwise consist of syntax characters stands for itself. A
    /// schema consisting of a literal string generates itself as the single password. Other characters
    /// may be escaped with `'\\'`; aside from newline, carriage return, and tab, any non-alphanumeric
    /// character stands for itself as a literal value when preceded by a backslash.
    /// ```
    /// # use {onepass_seed::expr::Node, core::str::FromStr};
    /// assert_eq!(Node::Literal("test".into()), "test".parse().unwrap());
    /// assert_eq!(Node::Literal("(escape){}[]".into()), r#"\(escape\)\{\}\[\]"#.parse().unwrap());
    /// ```
    ///
    /// Arbitrary Unicode characters may also be insterted as `\uXXXX`, or hex sequences (so long as
    /// they encode valid ASCII or UTF-8 byte sequences) as `\xXX`.
    ///
    /// # Character classes
    /// The special character classes `\w` and `\d` stand for word (alphanumeric plus underscore) and
    /// digit characters respectively. They may show up anywhere in an expression and stand for a
    /// single character in their range.
    ///
    /// Square bracket character classes are also supported, including the following POSIX character
    /// classes:
    /// - `[:lower:]` - lowercase ASCII letters
    /// - `[:upper:]` - uppercase ASCII letters
    /// - `[:alpha:]` - upper or lowercase ASCII letters
    /// - `[:digit:]` - decimal digits
    /// - `[:xdigit:]` - lowercase hexadecimal digits
    /// - `[:punct:]` - ASCII punctuation, aka special characters
    /// - `[:print:]` - printable ASCII characters
    ///
    /// Single characters (`[a]`) and unicode character ranges (`[a-z]`) are also supported.
    ///
    /// Any of these ranges may be combined within square brackets; `[[:upper:][a-z]\d]` corresponds to
    /// uppercase ASCII, lowercase ASCII, and decimal digits.
    ///
    /// ```
    /// # use {onepass_seed::expr::Node, core::str::FromStr};
    /// assert_eq!("[a-z]".parse::<Node>().unwrap(), "[[:lower:]]".parse().unwrap());
    /// assert_eq!("[A-Za-z0-9_]".parse::<Node>().unwrap(), "\\w".parse().unwrap());
    /// ```
    ///
    /// # Lists
    /// A sequence of nodes is represented by its concatenation. A nested list may be created using
    /// parentheses (`()`). This is of limited utility since the language does not support choices, but
    /// does allow e.g. setting a count on a sequence, like: `([[:lower:]][[:digit:]][[:lower:]]){3}`.
    ///
    /// ```
    /// # use core::str::FromStr;
    /// # use crypto_bigint::{NonZero, U256};
    /// # use num_traits::pow;
    /// # use onepass_seed::expr::{Eval, Expr};
    /// assert_eq!(
    ///     NonZero::new(U256::from_u64((26u64*10*26).pow(3))).unwrap(),
    ///     Expr::new("([[:lower:]][[:digit:]][[:lower:]]){3}".parse().unwrap()).size()
    /// );
    /// ```
    ///
    /// # Counts
    /// As alluded to, expressions may be repeated for specified counts. The syntax is
    /// `expr{min,max}`. If `max` is omitted, i.e. `expr{min}`, then `max == min`. If `min` is omitted,
    /// i.e. `expr{,max}`, `min == 0`.
    ///
    /// # Generators
    /// Arbitrary library-suppliable generators may be called. The library includes two: `word` to
    /// produce a single word, and `words` to produce a sequence of words. Generators are surrounded by
    /// curly braces and must start with a lowercase ASCII letter, e.g. `{word}`.
    ///
    /// Generators may take arguments. The first non–lowercase-ASCII character in a generator
    /// expression is taken as an argument separator, so e.g. `{words:2:U}` calls generator `words`
    /// with arguments `"2"` and `"U"`.
    ///
    /// # Reserved syntax
    /// The `|` character may be used inside of generators as an argument separator, like `{word|U}`,
    /// but may not be used unescaped anywhere else in an expression. This syntax is reserved for
    /// possible future expansion.
    ///
    /// # Context
    /// This function returns an expression against the default context.
    /// [`Self::parse_with_context`] may be used to parse an expression againsta custom context.
    pub fn parse(input: &str) -> Result<Self, Error> {
        Ok(Expr::new(input.parse()?))
    }
}

impl<'a> Expr<'a> {
    /// [`parse`][Self::parse] an expression with the given [`Context`].
    pub fn parse_with_context(input: &str, context: Context<'a>) -> Result<Self, Error> {
        Ok(Expr::with_context(input.parse()?, context))
    }
}

impl Node {
    pub fn parse(input: &str) -> IResult<&str, Node> {
        let (input, list) = many1(parse_count).parse(input)?;
        if list.len() == 1 {
            return Ok((input, list.into_iter().next().unwrap()));
        }
        Ok((input, Node::List(list.into_boxed_slice())))
    }
}

impl str::FromStr for Node {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match Node::parse(s).finish() {
            Ok((_remaining, node)) => Ok(node),
            Err(error::Error { input, code }) => Err(Error {
                input: input.to_string(),
                code,
            }),
        }
    }
}

fn parse_count(input: &str) -> IResult<&str, Node> {
    let (input, node) = parse_single(input)?;
    let (input, count) = opt(delimited(
        char('{'),
        alt((
            separated_pair(complete::u32, char(','), complete::u32),
            map(complete::u32, |n| (n, n)),
            map(preceded(char(','), complete::u32), |n| (0, n)),
        )),
        char('}'),
    ))
    .parse(input)?;
    Ok((
        input,
        match count {
            None => node,
            Some((a, b)) => Node::Count(Box::new(node), a, b),
        },
    ))
}

fn parse_single(input: &str) -> IResult<&str, Node> {
    alt((
        map(parse_literal, Node::Literal),
        map(parse_chars, Node::Chars),
        map(parse_generator, Node::Generator),
        parse_list,
    ))
    .parse(input)
}

fn parse_literal(input: &str) -> IResult<&str, Box<str>> {
    map(
        fold(
            1..,
            parse_literal_fragment,
            String::new,
            |mut string, fragment| {
                match fragment {
                    StringFragment::Escaped(c) => string.push(c),
                    StringFragment::Verbatim(s) => string.push_str(s),
                }
                string
            },
        ),
        Into::into,
    )
    .parse(input)
}

fn parse_literal_fragment(input: &str) -> IResult<&str, StringFragment<'_>> {
    alt((
        map(parse_literal_verbatim, StringFragment::Verbatim),
        map(parse_literal_escaped, StringFragment::Escaped),
    ))
    .parse(input)
}

fn parse_literal_escaped(input: &str) -> IResult<&str, char> {
    alt((
        preceded(
            char('\\'),
            alt((
                value('\n', char('n')),
                value('\r', char('r')),
                value('\t', char('t')),
                verify(anychar, |&c| !c.is_ascii_alphanumeric()),
            )),
        ),
        parse_hex_char,
        parse_unicode_char,
    ))
    .parse(input)
}

fn parse_unicode_char(input: &str) -> IResult<&str, char> {
    map_res(parse_unicode_digits, char::try_from).parse(input)
}

fn parse_unicode_digits(input: &str) -> IResult<&str, u32> {
    preceded(
        tag("\\u"),
        map_res(
            alt((
                take_while_m_n(4, 4, |c: char| c.is_ascii_hexdigit()),
                delimited(
                    char('{'),
                    take_while_m_n(1, 6, |c: char| c.is_ascii_hexdigit()),
                    char('}'),
                ),
            )),
            |s| u32::from_str_radix(s, 16),
        ),
    )
    .parse(input)
}

fn parse_hex_char(input: &str) -> IResult<&str, char> {
    let (input, b) = parse_hex_byte(input)?;
    if b < 0b1000_0000 {
        return Ok((input, b as char));
    }
    if b & 0b1110_0000 == 0b1100_0000 {
        map_res(parse_hex_byte, |b2| {
            let bs = [b, b2];
            str_to_char(&bs)
        })
        .parse(input)
    } else if b & 0b1111_0000 == 0b1110_0000 {
        map_res((parse_hex_byte, parse_hex_byte), |(b2, b3)| {
            let bs = [b, b2, b3];
            str_to_char(&bs)
        })
        .parse(input)
    } else if b & 0b1111_1000 == 0b1111_0000 {
        map_res(
            (parse_hex_byte, parse_hex_byte, parse_hex_byte),
            |(b2, b3, b4)| {
                let bs = [b, b2, b3, b4];
                str_to_char(&bs)
            },
        )
        .parse(input)
    } else {
        Err(nom::Err::Error(error::Error::new(input, ErrorKind::Verify)))
    }
}

fn str_to_char(bs: &[u8]) -> Result<char, Utf8Error> {
    let s = str::from_utf8(bs)?;
    let mut iter = s.chars();
    let c = iter.next().expect(s);
    assert!(iter.next().is_none());
    Ok(c)
}

fn parse_hex_byte(input: &str) -> IResult<&str, u8> {
    preceded(
        tag("\\x"),
        map_res(take_while_m_n(2, 2, |c: char| c.is_ascii_hexdigit()), |s| {
            u8::from_str_radix(s, 16)
        }),
    )
    .parse(input)
}

fn parse_literal_verbatim(input: &str) -> IResult<&str, &str> {
    let (input, res) = verify(is_not("\\[](){}|"), |s: &str| !s.is_empty()).parse(input)?;
    Ok((input, res))
}

fn parse_chars(input: &str) -> IResult<&str, Chars> {
    alt((
        parse_legacy_words_err,
        parse_chars_brackets,
        map(parse_chars_special, |ps| {
            Chars::from_ranges(ps.iter().copied())
        }),
    ))
    .parse(input)
}

fn parse_legacy_words_err(input: &str) -> IResult<&str, Chars> {
    let res = alt((tag("[:word:]"), tag("[:Word:]"))).parse(input);
    match res {
        Ok(_) => Err(nom::Err::Failure(error::Error::new(
            input,
            ErrorKind::Verify,
        ))),
        Err(e) => Err(e),
    }
}

fn parse_chars_brackets(input: &str) -> IResult<&str, Chars> {
    delimited(
        char('['),
        map(
            fold(
                1..,
                alt((
                    map(parse_chars_posix, CharFragment::Multi),
                    map(parse_chars_special, CharFragment::Multi),
                    map(parse_chars_range, CharFragment::Single),
                )),
                Vec::new,
                |mut chars, fragment| {
                    match fragment {
                        CharFragment::Single(p) => chars.push(p),
                        CharFragment::Multi(ps) => chars.extend(ps),
                    }
                    chars
                },
            ),
            Chars::from_ranges,
        ),
        char(']'),
    )
    .parse(input)
}

static LOWER: &[(char, char)] = &[('a', 'z')];
static UPPER: &[(char, char)] = &[('A', 'Z')];
static ALPHA: &[(char, char)] = &[('A', 'Z'), ('a', 'z')];
static ALNUM: &[(char, char)] = &[('0', '9'), ('A', 'Z'), ('a', 'z')];
static DIGIT: &[(char, char)] = &[('0', '9')];
static XDIGIT: &[(char, char)] = &[('0', '9'), ('a', 'f')];
static PUNCT: &[(char, char)] = &[('!', '/'), (':', '@'), ('[', '`'), ('{', '~')];
static PRINT: &[(char, char)] = &[(' ', '~')];
static WORD: &[(char, char)] = &[('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')];

fn parse_chars_posix(input: &str) -> IResult<&str, &'static [(char, char)]> {
    delimited(
        tag("[:"),
        alt((
            value(LOWER, tag("lower")),
            value(UPPER, tag("upper")),
            value(ALPHA, tag("alpha")),
            value(ALNUM, tag("alnum")),
            value(DIGIT, tag("digit")),
            value(XDIGIT, tag("xdigit")),
            value(PUNCT, tag("punct")),
            value(PRINT, tag("print")),
        )),
        tag(":]"),
    )
    .parse(input)
}

fn parse_chars_range(input: &str) -> IResult<&str, (char, char)> {
    if let (input2, Some((a, b))) = opt(separated_pair(
        parse_chars_single,
        char('-'),
        parse_chars_single,
    ))
    .parse(input)?
    {
        if a <= b {
            return Ok((input2, (a, b)));
        }
        return Err(nom::Err::Failure(error::Error::new(
            input,
            ErrorKind::Verify,
        )));
    }
    map(parse_chars_single, |c| (c, c)).parse(input)
}

fn parse_chars_single(input: &str) -> IResult<&str, char> {
    alt((none_of("\\]"), parse_literal_escaped)).parse(input)
}

fn parse_chars_special(input: &str) -> IResult<&str, &'static [(char, char)]> {
    preceded(
        char('\\'),
        alt((value(WORD, char('w')), value(DIGIT, char('d')))),
    )
    .parse(input)
}

fn parse_generator(input: &str) -> IResult<&str, Generator> {
    let verify_inner = peek(verify(anychar, |c| c.is_ascii_lowercase()));
    let parse_inner = map(
        fold(
            1..,
            parse_generator_fragment,
            String::new,
            |mut string, fragment| {
                match fragment {
                    StringFragment::Escaped(c) => string.push(c),
                    StringFragment::Verbatim(s) => string.push_str(s),
                }
                string
            },
        ),
        Generator::from,
    );
    delimited(char('{'), preceded(verify_inner, parse_inner), char('}')).parse(input)
}

fn parse_generator_fragment(input: &str) -> IResult<&str, StringFragment<'_>> {
    alt((
        map(parse_generator_verbatim, StringFragment::Verbatim),
        map(parse_literal_escaped, StringFragment::Escaped),
    ))
    .parse(input)
}

fn parse_generator_verbatim(input: &str) -> IResult<&str, &str> {
    verify(is_not("\\}"), |s: &str| !s.is_empty()).parse(input)
}

fn parse_list(input: &str) -> IResult<&str, Node> {
    delimited(char('('), Node::parse, char(')')).parse(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        let node = "cats".parse().unwrap();
        assert_eq!(Node::Literal("cats".into()), node);
        let node = r#"\\cats\tand\[dogs\]\{woof\}}"#.parse().unwrap();
        assert_eq!(Node::Literal("\\cats\tand[dogs]{woof}".into()), node);
    }

    #[test]
    fn test_chars() {
        assert_eq!(
            Node::Chars(unsafe {
                Chars::from_ranges_unchecked([('0', '9'), ('A', 'Z'), ('a', 'z')])
            }),
            "[A-Za-z0123-9]".parse::<Node>().unwrap(),
        );
        let res = Node::parse("[z-a]");
        assert!(res.is_err(), "{res:?}");
        let e = format!("{}", res.unwrap_err());
        assert!(e.contains(r#"input: "z-a]""#));
    }

    #[test]
    fn test_chars_table() {
        let tests = [
            (vec![('A', 'Z')], "[A-MD-Z]"),
            (vec![('A', 'Z')], "[D-ZA-M]"),
            (vec![('a', 'j')], "[a-cb-ea-fb-j]"),
            (vec![('a', 'a'), ('c', 'c')], "[ac]"),
            (vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')], "\\w"),
            (vec![('a', 'z')], "[[:lower:]]"),
            (
                vec![('!', '/'), (':', '@'), ('[', '`'), ('{', '~')],
                "[[:punct:]]",
            ),
            (vec![('!', '~')], "[[:punct:]\\w]"),
        ];
        for (ranges, inp) in tests {
            assert_eq!(
                Node::Chars(unsafe { Chars::from_ranges_unchecked(ranges) }),
                inp.parse().unwrap(),
            );
        }
    }

    #[test]
    fn test_generators() {
        assert_eq!(
            Node::Generator(Generator::new("word\tup}")),
            "{word\\tup\\}}".parse().unwrap()
        );
    }

    #[test]
    fn test_multi() {
        assert_eq!(
            Node::List(
                vec![
                    Node::Generator(Generator::new("word")),
                    Node::Count(
                        Node::List(
                            vec![
                                Node::Literal("-".into()),
                                Node::Generator(Generator::new("word")),
                            ]
                            .into()
                        )
                        .into(),
                        4,
                        4
                    ),
                ]
                .into()
            ),
            "{word}(-{word}){4}".parse().unwrap(),
        );
    }

    #[test]
    fn test_legacy_words_err() {
        let res = "[:word:]".parse::<Node>();
        assert_eq!(
            "error Verify at: [:word:]",
            &format!("{}", res.unwrap_err())
        );
    }

    #[test]
    fn test_literal_digits() {
        assert_eq!(
            Node::Literal("—".into()),
            r#"\xe2\x80\x94"#.parse().unwrap()
        );
        assert_eq!(Node::Literal("—".into()), "\\u2014".parse().unwrap());
        assert_eq!(Node::Literal("—".into()), "\\u{002014}".parse().unwrap());
        assert_eq!(
            Err(error::Error {
                input: "\\x80".into(),
                code: ErrorKind::Char
            }),
            "\\x80".parse::<Node>(),
        );
        assert_eq!(
            Err(error::Error {
                input: "\\ud800".into(),
                code: ErrorKind::Char
            }),
            "\\ud800".parse::<Node>(),
        );
    }
}
