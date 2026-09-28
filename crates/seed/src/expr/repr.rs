use core::{
    fmt::{self, Result, Write},
    mem,
};

use super::{
    Context, Expr, Node,
    chars::{CharRange, Chars, next_char},
};

struct ReprState<'a>(bool, &'a Context);

impl Expr {
    /// Write the canonical serialization of this expression.
    ///
    /// This function implements this type’s [`fmt::Display`].
    pub fn write_repr<W>(&self, w: &mut W) -> Result
    where
        W: Write,
    {
        ReprState(false, self.context()).write(w, &self.root)
    }
}

impl Chars {
    /// Canonically serialize a character class.
    ///
    /// Character ranges are written sorted according to the Unicode ordering of the start of their
    /// range, with the exception of ranges that start or end with a hyphen, or start with a caret.
    /// A range starting with a hyphen is written at the start of the class; a range ending with a
    /// hyphen is written at the end of the class; and a caret is never written at the beginning of
    /// a class.
    ///
    /// The hyphen rules are to ensure that character classes parse correctly; the class containing
    /// the three characters `!`, `-`, and `z` would otherwise be written `[!-z]`, which would
    /// parse as the class ranging from `!` to `z`.
    ///
    /// The caret rule is to prevent a class’s canonical representation from looking like a negated
    /// character class in traditional regex languages. Onepass does not have a concept of negated
    /// character classes, so `[^a-z]` and `[a-z^]` would be equivalent if the former were allowed.
    ///
    /// In case the caret range is exactly two or three characters, it is written `_^` or `` _^ ``.
    /// Otherwise, if there is another (non-hyphen) range in the class, it is written first.
    /// Otherwise, the remainder of the range after the caret is written first (`[_^-z]`.)
    /// Otherwise, if there is an ending hyphen range, its first character is written first.
    /// Finally, if the range is a single caret, it is backslash-escaped.
    ///
    /// A user or program that does not need to write canonical forms may simply backslash-escape a
    /// caret (or hyphen) anywhere in a range.
    pub fn write_repr<W>(&self, w: &mut W) -> Result
    where
        W: Write,
    {
        let start_hyphen = self.0.iter().find(|r| r.start == '-');
        let mut end_hyphen = self
            .0
            .iter()
            .copied()
            .find(|r| r.end == '-' && r.start != '-');
        let mut rest = self
            .0
            .iter()
            .copied()
            .filter(|r| r.start != '-' && r.end != '-')
            .peekable();
        write!(w, "[")?;
        if let Some(r) = start_hyphen {
            fmt_charclass(w, r)?;
        } else if let Some(mut r) = rest.next_if(|r| r.start == '^') {
            if r.size() == 2 {
                write!(w, "_^")?;
            } else if r.size() == 3 {
                write!(w, "_^`")?;
            } else if let Some(q) = rest.next() {
                fmt_charclass(w, &q)?;
                fmt_charclass(w, &r)?;
            } else if r.end != '^' {
                r.start = next_char(r.start).unwrap();
                fmt_charclass(w, &r)?;
                write!(w, "^")?;
            } else if let Some(q) = end_hyphen.as_mut() {
                write_escape(w, q.start)?;
                q.start = next_char(q.start).unwrap();
                fmt_charclass(w, &r)?;
            } else {
                // A plain `^` is also accepted here at parse.
                write!(w, "\\^")?;
            }
        }
        for r in rest.chain(end_hyphen) {
            fmt_charclass(w, &r)?;
        }
        write!(w, "]")?;
        Ok(())
    }
}

impl ReprState<'_> {
    pub fn write<W>(&mut self, w: &mut W, node: &Node) -> Result
    where
        W: Write,
    {
        match *node {
            Node::Literal(ref s) => write_literal(w, s),
            Node::Chars(ref chars) => write!(w, "{chars}"),
            Node::List(ref list) => {
                let nested = mem::replace(&mut self.0, true);
                if nested {
                    write!(w, "(")?;
                }
                list.iter().try_for_each(|node| self.write(w, node))?;
                if nested {
                    write!(w, ")")?;
                }
                Ok(())
            }

            Node::Count(ref node, min, max) => {
                self.0 = true;
                self.write(w, node)?;
                w.write_char('{')?;
                // NB. it is legal to have max == 0.
                if min != 0 || max == 0 {
                    write!(w, "{min}")?;
                }
                if max != min {
                    write!(w, ",{max}")?;
                }
                w.write_char('}')
            }

            Node::Generator(ref generator) => {
                w.write_char('{')?;
                self.1.get_generator(generator.name()).unwrap().write_repr(
                    self.1,
                    w,
                    &generator.args(),
                )?;
                w.write_char('}')?;
                Ok(())
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result {
        self.write_repr(f)
    }
}

impl fmt::Display for Chars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result {
        self.write_repr(f)
    }
}

pub enum Escape {
    Hex,
    Str(&'static str),
}

pub fn write_literal<W>(w: &mut W, s: &str) -> Result
where
    W: fmt::Write + ?Sized,
{
    use Escape::*;
    let mut pos = 0;
    for (i, b) in s.bytes().enumerate() {
        let escaped = match b {
            b'\\' => Str("\\\\"),
            b'(' => Str("\\("),
            b')' => Str("\\)"),
            b'[' => Str("\\["),
            b']' => Str("\\]"),
            b'{' => Str("\\{"),
            b'}' => Str("\\}"),
            b'|' => Str("\\|"),
            b'\x00'..b'\x20' | b'\x7f' => Hex,
            _ => continue,
        };
        if pos != i {
            w.write_str(&s[pos..i])?;
        }
        match escaped {
            Str(s) => w.write_str(s),
            Hex => write!(w, "\\x{b:02x}"),
        }?;
        pos = i + 1;
    }
    if pos != s.len() {
        w.write_str(&s[pos..])?;
    }
    Ok(())
}

pub fn write_escape<W>(w: &mut W, c: char) -> Result
where
    W: Write,
{
    match c {
        '\x00'..'\x20' | '\x7f' => write!(w, "\\x{:02x}", c as u8),
        ']' => w.write_str("\\]"),
        '\\' => w.write_str("\\\\"),
        c if c.is_ascii() => w.write_char(c),
        _ => write!(w, "{}", c.escape_debug()),
    }
}

pub fn fmt_charclass<W>(w: &mut W, cr: &CharRange) -> Result
where
    W: Write,
{
    write_escape(w, cr.start)?;
    if cr.end != cr.start {
        if let Some(next) = next_char(cr.start)
            && next != cr.end
        {
            w.write_char('-')?;
        }
        write_escape(w, cr.end)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chars_hyphens() {
        let tests: [(&str, &[(char, char)]); _] = [
            ("[-a]", &[('-', '-'), ('a', 'a')]),
            ("[Z!--]", &[('Z', 'Z'), ('!', '-')]),
            ("[\\\\\\]]", &[('\\', ']')]),
            ("[!-#]", &[('!', '#')]),
            ("[!\"]", &[('!', '"')]),
            ("[-^]", &[('-', '-'), ('^', '^')]),
            ("[a-z^]", &[('^', '^'), ('a', 'z')]),
            ("[_^]", &[('^', '_')]),
            ("[,^-]", &[(',', '-'), ('^', '^')]),
            ("[+^,-]", &[('+', '-'), ('^', '^')]),
            ("[Z[^]", &[('Z', 'Z'), ('[', '['), ('^', '^')]),
            ("[\\^]", &[('^', '^')]),
            ("[_^`]", &[('^', '`')]),
            ("[_-z^]", &[('^', 'z')]),
            ("[_^`]", &[('^', '`')]),
            ("[_-z^!--]", &[('!', '-'), ('^', 'z')]),
        ];
        for (want, cs) in tests {
            let cs = Chars::from_ranges(cs.iter().copied());
            eprintln!("=== {want} {cs:?} ===");
            assert_eq!(want, &format!("{cs}"), "cs={cs:?}");
            let expr = Expr::new(want.parse().unwrap());
            assert_eq!(want, &format!("{expr}"), "{want:?} cs={cs:?}");
        }
    }

    #[test]
    fn test_non_printable() {
        for (want, root) in [
            (
                "[\\x00-\\x7f]",
                Node::Chars(Chars::from_ranges([('\0', '\x7f')])),
            ),
            (
                "[\u{2014}-\u{2026}]",
                Node::Chars(Chars::from_ranges([('—', '…')])),
            ),
            (r#"\x00—\x7f"#, Node::Literal("\0—\x7f".into())),
        ] {
            assert_eq!(want, &format!("{}", Expr::new(root.clone())));
            assert_eq!(root, want.parse().unwrap());
        }
    }

    #[test]
    fn test_literal() {
        assert_eq!(
            r#"\{\}"#,
            &format!("{}", Expr::new(Node::Literal("{}".into())))
        );
    }

    #[test]
    fn test_nested() {
        assert_eq!(
            "([a-z][0-9]){3,6}",
            &format!("{}", Expr::parse("([a-z][0-9]){3,6}").unwrap())
        );
        assert_eq!(
            "[a-z]{,3}",
            &format!("{}", Expr::parse("[a-z]{0,3}").unwrap())
        );
    }
}
