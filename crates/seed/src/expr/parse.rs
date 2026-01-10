use core::str;

use nom::{
    Finish, IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{self, anychar, char, none_of},
    combinator::{map, opt, peek, value, verify},
    error::{self, ErrorKind},
    multi::{fold, many1},
    sequence::{delimited, preceded, separated_pair},
};

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

static LOWER: &[(char, char)] = &[('a', 'z')];
static UPPER: &[(char, char)] = &[('A', 'Z')];
static ALPHA: &[(char, char)] = &[('A', 'Z'), ('a', 'z')];
static ALNUM: &[(char, char)] = &[('0', '9'), ('A', 'Z'), ('a', 'z')];
static DIGIT: &[(char, char)] = &[('0', '9')];
static XDIGIT: &[(char, char)] = &[('0', '9'), ('a', 'f')];
static PUNCT: &[(char, char)] = &[('!', '/'), (':', '@'), ('[', '`'), ('{', '~')];
static PRINT: &[(char, char)] = &[(' ', '~')];
static WORD: &[(char, char)] = &[('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')];

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
    preceded(
        char('\\'),
        alt((
            value('\n', char('n')),
            value('\r', char('r')),
            value('\t', char('t')),
            verify(anychar, |&c| !c.is_ascii_alphanumeric()),
            // TODO(soon): unicode/hex digits
        )),
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
    let verify_inner = peek(verify(anychar, |c| c.is_ascii_alphabetic()));
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
}
