// Header {{{1

use core::str::{FromStr, Utf8Error, from_utf8};

use nom::{
    AsChar, Finish, IResult, Input, Parser,
    branch::alt,
    bytes::complete::{is_not, tag, take_while_m_n, take_while1},
    character::complete::{anychar, char, none_of, u32},
    combinator::{all_consuming, cut, map, map_res, opt, peek, value, verify},
    error::{Error as NomError, ErrorKind, ParseError},
    multi::{fold, many1},
    sequence::{delimited, preceded, separated_pair},
};

use super::{Context, Expr, Node, chars::Chars, generator::Generator};

enum StringFragment<'a> {
    Verbatim(&'a str),
    Escaped(char),
}

enum CharFragment {
    Single((char, char)),
    Multi(&'static [(char, char)]),
}

enum Brace {
    Paren,
    Curly,
    Square,
}

pub type Error = NomError<String>;

// Expr {{{1

impl Expr {
    /// Expressions can be parsed from UTF-8 strings.
    ///
    /// The following syntax is supported:
    ///
    /// # String literals
    /// Any literal string that does not otherwise consist of syntax characters stands for itself.
    /// A schema consisting of a literal string generates itself as the single password. Other
    /// characters may be escaped with `'\\'`; aside from newline, carriage return, and tab, any
    /// non-alphanumeric character stands for itself as a literal value when preceded by a
    /// backslash.
    /// ```
    /// # use {onepass_seed::expr::Node, core::str::FromStr};
    /// assert_eq!(Node::Literal("test".into()), "test".parse().unwrap());
    /// assert_eq!(Node::Literal("(escape){}[]".into()), r#"\(escape\)\{\}\[\]"#.parse().unwrap());
    /// ```
    ///
    /// Arbitrary Unicode characters may also be insterted as `\uXXXX`, or hex sequences (so long
    /// as they encode valid ASCII or UTF-8 byte sequences) as `\xXX`.
    ///
    /// # Character classes
    /// The special character classes `\w` and `\d` stand for word (alphanumeric plus underscore)
    /// and digit characters respectively. They may show up anywhere in an expression and stand for
    /// a single character in their range.
    ///
    /// Square bracket character classes are also supported, including the following POSIX
    /// character classes:
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
    /// Any of these ranges may be combined within square brackets; `[[:upper:][a-z]\d]`
    /// corresponds to uppercase ASCII, lowercase ASCII, and decimal digits.
    ///
    /// ```
    /// # use {onepass_seed::expr::Node, core::str::FromStr};
    /// assert_eq!("[a-z]".parse::<Node>().unwrap(), "[[:lower:]]".parse().unwrap());
    /// assert_eq!("[A-Za-z0-9_]".parse::<Node>().unwrap(), "\\w".parse().unwrap());
    /// ```
    ///
    /// # Lists
    /// A sequence of nodes is represented by its concatenation. A nested list may be created using
    /// parentheses (`()`). This is of limited utility since the language does not support choices,
    /// but does allow e.g. setting a count on a sequence, like:
    /// `([[:lower:]][[:digit:]][[:lower:]]){3}`.
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
    /// `expr{min,max}`. If `max` is omitted, i.e. `expr{min}`, then `max == min`. If `min` is
    /// omitted, i.e. `expr{,max}`, then `min == 0`.
    ///
    /// **NB.** In the current revision of the schema language, a count after a literal applies to
    /// the whole string, not just the last character; so `ab{2}` is equivalent to `(ab){2}`, not
    /// `a(b){2}`:
    /// ```
    /// # use {onepass_seed::expr::Node, core::str::FromStr};
    /// assert_eq!("(ab){2}".parse::<Node>().unwrap(), "ab{2}".parse().unwrap());
    /// ```
    ///
    /// # Generators
    /// Arbitrary library-suppliable generators may be called. The library includes two: `word` to
    /// produce a single word, and `words` to produce a sequence of words. Generators are
    /// surrounded by curly braces and must start with a lowercase ASCII letter, e.g. `{word}`.
    /// (This rule is what differentiates them from counts, which must start with an ASCII digit.)
    ///
    /// Generators may take arguments. The first non–lowercase-ASCII character in a generator
    /// expression is taken as an argument separator, so e.g. `{words:2:U}` calls generator `words`
    /// with arguments `"2"` and `"U"`.
    ///
    /// # Reserved syntax
    /// The `|` character may be used inside of generators as an argument separator, like
    /// `{word|U}`, but may not be used unescaped anywhere else in an expression. This syntax is
    /// reserved for possible future expansion.
    ///
    /// # Errors
    /// It is an error to write a character class with the higher character before the lower
    /// character, e.g. `[b-a]`.
    /// ```
    /// # use core::str::FromStr;
    /// # use onepass_seed::expr::Node;
    /// assert!("[b-a]".parse::<Node>().is_err());
    /// ```
    ///
    /// Partial remainders, e.g. in the case of unbalanced delimiters, yield an error. (These can,
    /// and should, simply be backslash-escaped.)
    /// ```
    /// # use core::str::FromStr;
    /// # use onepass_seed::expr::Node;
    /// assert!("abcd}".parse::<Node>().is_err());
    /// assert!("abcd\\}".parse::<Node>().is_ok());
    /// ```
    ///
    /// At present, hex sequences that do not encode valid UTF-8 encoded text are an error.
    ///
    /// The syntax `[:word:]` (and `[:Word:]`) used to be the way to generate a word from a
    /// dictionary in `onepass` v2. In the current syntax, these would both parse to degenerate
    /// character classes, e.g. `[:dorw]`. Since this is virtually never intended, those specific
    /// strings are presently a parse error.
    ///
    /// # Context
    /// This function returns an expression against the default context.
    /// [`Self::parse_with_context`] may be used to parse an expression against a custom context.
    pub fn parse(input: &str) -> Result<Self, Error> {
        Ok(Expr::new(input.parse()?))
    }

    /// [`parse`][Self::parse] an expression with the given [`Context`].
    pub fn parse_with_context(input: &str, context: &Context) -> Result<Self, Error> {
        Ok(Expr::with_context(input.parse()?, context))
    }
}

// Node {{{1
// Top {{{2

impl FromStr for Node {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_node(s).map_err(|e| Error::from_error_kind(e.input.to_string(), e.code))
    }
}

/// Parse a [`Node`].
///
/// This function is identical to [`Node::from_str`] (and [`Expr::parse`]) aside from the return
/// and error types.
pub fn parse_node(input: &str) -> Result<Node, NomError<&'_ str>> {
    all_consuming(parse_node_inner)
        .parse(input)
        .finish()
        .map(|(_, node)| node)
}

fn parse_node_inner(input: &str) -> IResult<&str, Node> {
    map(many1(parse_count), Node::from_iter).parse(input)
}

fn parse_count(input: &str) -> IResult<&str, Node> {
    let (input, node) = parse_single(input)?;
    let (remaining, count) = opt(braced(
        Brace::Curly,
        alt((
            separated_pair(u32, char(','), u32),
            map(u32, |n| (n, n)),
            map(preceded(char(','), u32), |n| (0, n)),
        )),
    ))
    .parse(input)?;
    match count {
        None => Ok((remaining, node)),
        Some((min, max)) if max >= min => Ok((remaining, Node::Count(Box::new(node), min, max))),
        _ => Err(verify_failure(input)),
    }
}

fn parse_single(input: &str) -> IResult<&str, Node> {
    alt((
        map(parse_chars, Node::Chars),
        map(parse_literal, Node::Literal),
        map(parse_generator, Node::Generator),
        parse_list,
    ))
    .parse(input)
}

fn parse_list(input: &str) -> IResult<&str, Node> {
    braced(Brace::Paren, parse_node_inner).parse(input)
}

// Literals {{{2

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

fn parse_literal_verbatim(input: &str) -> IResult<&str, &str> {
    let (input, res) = verify(is_not("\\[](){}|"), |s: &str| !s.is_empty()).parse(input)?;
    Ok((input, res))
}

fn parse_literal_escaped(input: &str) -> IResult<&str, char> {
    let _ = peek((char('\\'), is_not("dw"))).parse(input)?;
    cut(alt((
        parse_hex_char,
        parse_unicode_char,
        preceded(
            char('\\'),
            alt((
                value('\n', char('n')),
                value('\r', char('r')),
                value('\t', char('t')),
                verify(anychar, |&c| !c.is_ascii_alphanumeric()),
            )),
        ),
    )))
    .parse(input)
    .map_err(|e| e.map(|inner| NomError::from_error_kind(input, inner.code)))
}

fn parse_unicode_char(input: &str) -> IResult<&str, char> {
    let (remaining, n) = preceded(
        tag("\\u"),
        cut(map_res(
            alt((
                take_while_m_n(4, 4, |c: char| c.is_ascii_hexdigit()),
                braced(
                    Brace::Curly,
                    take_while_m_n(1, 6, |c: char| c.is_ascii_hexdigit()),
                ),
            )),
            |s| u32::from_str_radix(s, 16),
        )),
    )
    .parse(input)?;
    Ok((
        remaining,
        char::try_from(n).map_err(|_| verify_failure(input))?,
    ))
}

fn parse_hex_char(input: &str) -> IResult<&str, char> {
    let (mut remaining, b) = parse_hex_byte(input)?;
    let size = b.leading_ones() as usize;
    if size == 0 {
        return Ok((remaining, b as char));
    }
    if size == 1 || size > 4 {
        return Err(verify_failure(input));
    }
    let mut bs = [0u8; 4];
    bs[0] = b;
    for slot in &mut bs[1..size] {
        (remaining, *slot) = cut(parse_hex_byte).parse(remaining)?;
    }
    let c = str_to_char(&bs[..size]).map_err(|_| verify_failure(input))?;
    Ok((remaining, c))
}

fn str_to_char(bs: &[u8]) -> Result<char, Utf8Error> {
    let s = from_utf8(bs)?;
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

// Chars {{{2

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
        Ok(_) => Err(verify_failure(input)),
        Err(e) => Err(e),
    }
}

fn parse_chars_brackets(input: &str) -> IResult<&str, Chars> {
    braced(
        Brace::Square,
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
    let (remaining, class) = delimited(
        tag("[:"),
        take_while1(|c: char| c.is_ascii_alphabetic()),
        tag(":]"),
    )
    .parse(input)?;
    Ok((
        remaining,
        match class {
            "lower" => LOWER,
            "upper" => UPPER,
            "alpha" => ALPHA,
            "alnum" => ALNUM,
            "digit" => DIGIT,
            "xdigit" => XDIGIT,
            "punct" => PUNCT,
            "print" => PRINT,
            _ => return Err(verify_failure(input)),
        },
    ))
}

fn parse_chars_range(input: &str) -> IResult<&str, (char, char)> {
    if let (remaining, Some((a, b))) = opt(separated_pair(
        parse_chars_single,
        char('-'),
        parse_chars_single,
    ))
    .parse(input)?
    {
        if a <= b {
            return Ok((remaining, (a, b)));
        }
        return Err(verify_failure(input));
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

// Generators {{{2

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
    braced(Brace::Curly, preceded(verify_inner, parse_inner)).parse(input)
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

// Utility {{{2

fn braced<I, O, E, F>(brace: Brace, inner: F) -> impl Parser<I, Output = O, Error = E>
where
    I: Input,
    <I as Input>::Item: AsChar,
    E: ParseError<I>,
    F: Parser<I, Output = O, Error = E>,
{
    let (open, close) = match brace {
        Brace::Paren => ('(', ')'),
        Brace::Curly => ('{', '}'),
        Brace::Square => ('[', ']'),
    };
    delimited(char(open), inner, cut(char(close)))
}

fn verify_failure<I, E: ParseError<I>>(input: I) -> nom::Err<E> {
    nom::Err::Failure(E::from_error_kind(input, ErrorKind::Verify))
}

// Tests {{{1

#[cfg(test)]
mod tests {
    use super::*;

    use std::assert_matches;

    macro_rules! assert_parse {
        ($input:expr, $ast:expr $(,)?) => {
            assert_eq!(Ok($ast), parse_node($input))
        };
    }

    macro_rules! assert_err {
        ($input:expr, $rest:expr, $kind:ident $(,)?) => {
            assert_eq!(
                Err(NomError::new($rest, ErrorKind::$kind)),
                parse_node($input)
            )
        };
    }

    #[test]
    fn test_literal() {
        assert_parse!("cats", Node::Literal("cats".into()));
        assert_parse!(
            r#"\\cats\tand\[dogs\]\{woof\}"#,
            Node::Literal("\\cats\tand[dogs]{woof}".into())
        );
    }

    #[test]
    fn test_bad_escape() {
        assert_err!("\\a", "\\a", Verify);
    }

    #[test]
    fn test_chars() {
        assert_parse!(
            "[A-Za-z0123-9]",
            Node::Chars(unsafe {
                Chars::from_ranges_unchecked([('0', '9'), ('A', 'Z'), ('a', 'z')])
            }),
        );
        assert_err!("[z-a]", "z-a]", Verify);
    }

    #[test]
    fn test_chars_table() {
        let tests = [
            (vec![('A', 'Z')], "[A-MD-Z]"),
            (vec![('A', 'Z')], "[D-ZA-M]"),
            (vec![('a', 'j')], "[a-cb-ea-fb-j]"),
            (vec![('a', 'a'), ('c', 'c')], "[ac]"),
            (vec![('0', '9'), ('A', 'Z'), ('_', '_'), ('a', 'z')], "\\w"),
            (vec![('0', '9')], "[[:digit:]]"),
            (vec![('a', 'z')], "[[:lower:]]"),
            (
                vec![('!', '/'), (':', '@'), ('[', '`'), ('{', '~')],
                "[[:punct:]]",
            ),
            (vec![('!', '~')], "[[:punct:]\\w]"),
        ];
        for (ranges, inp) in tests {
            assert_parse!(
                inp,
                Node::Chars(unsafe { Chars::from_ranges_unchecked(ranges) }),
            );
        }
    }

    #[test]
    fn test_chars_after_literal() {
        let chars = "\\w".parse().unwrap();
        assert_parse!(
            "a\\w",
            Node::List([Node::Literal("a".into()), chars].into())
        );
    }

    #[test]
    fn test_posix_errors() {
        assert_err!("[[:foo:]]", "[:foo:]]", Verify);
        assert_err!("[[:Digit:]]", "[:Digit:]]", Verify);
        assert_matches!(parse_node("[[:]"), Ok(Node::Chars(_)));
    }

    #[test]
    fn test_generators() {
        assert_parse!(
            "{word\\tup\\}}",
            Node::Generator(Generator::new("word\tup}")),
        );
    }

    #[test]
    fn test_multi() {
        assert_parse!(
            "{word}(-{word}){4}",
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
        );
    }

    #[test]
    fn test_legacy_words_err() {
        assert_err!("[:word:]", "[:word:]", Verify);
    }

    #[test]
    fn test_literal_digits() {
        assert_parse!(r#"\xe2\x80\x94"#, Node::Literal("—".into()),);
        assert_parse!("\\u2014", Node::Literal("—".into()));
        assert_parse!("\\u{002014}", Node::Literal("—".into()));
        assert_err!("\\x80", "\\x80", Verify);
        assert_err!("\\xd0\\x00", "\\xd0\\x00", Verify);
        assert_err!("\\ud800", "\\ud800", Verify);
    }

    #[test]
    fn test_remaining() {
        assert_err!("a\\", "\\", Eof);
    }

    #[test]
    fn test_reserved() {
        assert_err!("a|test", "|test", Eof);
    }

    #[test]
    fn test_counts() {
        fn extract_count(s: &str) -> Result<(u32, u32), NomError<&'_ str>> {
            parse_node(s).map(|node| match node {
                Node::Count(_, min, max) => (min, max),
                _ => panic!(),
            })
        }
        assert_matches!(extract_count("a{2,5}"), Ok((2, 5)));
        assert_matches!(extract_count("a{,3}"), Ok((0, 3)));
        assert_matches!(
            extract_count(&format!("a{{{},{}}}", u32::MAX, u32::MAX)),
            Ok((u32::MAX, u32::MAX))
        );
        assert_err!("a{5,2}", "{5,2}", Verify);
        assert_err!("a{3,}", ",}", Char);
    }
}

// Coda
// vim:fdm=marker
