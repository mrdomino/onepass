use core::fmt;
use std::fmt::{Result, Write};

use crate::expr::{
    Context, Expr, Node,
    chars::{CharRange, Chars},
};

struct ReprState<'a, 'b>(bool, &'a Context<'b>);

impl Expr<'_> {
    /// Write the canonical serialization of this expression. This function implements this type’s
    /// [`fmt::Display`].
    pub fn write_repr<W>(&self, w: &mut W) -> Result
    where
        W: Write,
    {
        ReprState(false, self.get_context()).write(w, &self.root)
    }
}

impl Chars {
    pub fn write_repr<W>(&self, w: &mut W) -> Result
    where
        W: Write,
    {
        write!(w, "[")?;
        if let Some(hyphen) = self.0.iter().find(|cr| cr.start == '-') {
            fmt_charclass(w, hyphen)?;
        }
        self.0
            .iter()
            .filter(|&cr| cr.start != '-' && cr.end != '-')
            .try_fold((), |(), cr| fmt_charclass(w, cr))?;
        if let Some(hyphen) = self.0.iter().find(|cr| cr.end == '-' && cr.start != '-') {
            fmt_charclass(w, hyphen)?;
        }
        write!(w, "]")?;
        Ok(())
    }
}

impl ReprState<'_, '_> {
    pub fn write<W>(&mut self, w: &mut W, node: &Node) -> Result
    where
        W: Write,
    {
        match *node {
            Node::Literal(ref s) => write_literal(w, s),
            Node::Chars(ref chars) => write!(w, "{chars}"),
            Node::List(ref list) => {
                let nested;
                (nested, self.0) = (self.0, true);
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
                self.write(w, node)?;
                w.write_char('{')?;
                write!(w, "{min}")?;
                if max != min {
                    write!(w, ",{max}")?;
                }
                w.write_char('}')
            }
            Node::Generator(ref generator) => {
                w.write_char('{')?;
                self.1
                    .get(generator.name())
                    .unwrap()
                    .write_repr(w, &generator.args())?;
                w.write_char('}')?;
                Ok(())
            }
        }
    }
}

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result {
        self.write_repr(f)
    }
}

impl fmt::Display for Chars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result {
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
        _ => write!(w, "{}", c.escape_debug()),
    }
}

pub fn fmt_charclass<W>(w: &mut W, cr: &CharRange) -> Result
where
    W: Write,
{
    write_escape(w, cr.start)?;
    if cr.end != cr.start {
        write!(w, "-")?;
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
            ("[\\\\-\\]]", &[('\\', ']')]),
        ];
        for (want, cs) in tests {
            let cs = Chars::from_ranges(cs.iter().copied());
            eprintln!("want=\"{want}\" cs={cs:?}");
            assert_eq!(want, &format!("{cs}"));
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
}
