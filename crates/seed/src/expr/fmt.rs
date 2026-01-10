use core::fmt;
use std::fmt::{Result, Write};

use crate::expr::{
    Context, Expr, Node,
    chars::{CharRange, Chars},
};

struct FmtState<'a, 'b>(bool, &'a Context<'b>);

impl FmtState<'_, '_> {
    pub fn fmt(&mut self, f: &mut fmt::Formatter<'_>, node: &Node) -> Result {
        match *node {
            Node::Literal(ref s) => fmt_literal(f, s),
            Node::Chars(ref chars) => write!(f, "{chars}"),
            Node::List(ref list) => {
                let nested;
                (nested, self.0) = (self.0, true);
                if nested {
                    write!(f, "(")?;
                }
                list.iter().try_for_each(|node| self.fmt(f, node))?;
                if nested {
                    write!(f, ")")?;
                }
                Ok(())
            }
            Node::Count(ref node, min, max) => {
                self.fmt(f, node)?;
                f.write_char('{')?;
                write!(f, "{min}")?;
                if max != min {
                    write!(f, ",{max}")?;
                }
                f.write_char('}')
            }
            Node::Generator(ref generator) => {
                f.write_char('{')?;
                self.1
                    .get(generator.name())
                    .unwrap()
                    .fmt(f, &generator.args())?;
                f.write_char('}')?;
                Ok(())
            }
        }
    }
}

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result {
        FmtState(false, self.get_context()).fmt(f, &self.root)
    }
}

pub enum Escape {
    Hex,
    Str(&'static str),
}

pub fn fmt_literal(f: &mut fmt::Formatter<'_>, s: &str) -> Result {
    use Escape::*;
    let mut pos = 0;
    for (i, b) in s.bytes().enumerate() {
        let escaped = match b {
            b'\\' => Str("\\\\"),
            b'(' => Str("\\("),
            b')' => Str("\\)"),
            b'[' => Str("\\["),
            b']' => Str("\\]"),
            b'{' => Str("{{"),
            b'}' => Str("}}"),
            b'|' => Str("\\|"),
            b'\x20'..=b'\x7e' => continue,
            _ => Hex,
        };
        if pos != i {
            f.write_str(&s[pos..i])?;
        }
        match escaped {
            Str(s) => f.write_str(s),
            Hex => write!(f, "\\x{b:02x}"),
        }?;
        pos = i + 1;
    }
    if pos != s.len() {
        f.write_str(&s[pos..])?;
    }
    Ok(())
}

impl fmt::Display for Chars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result {
        write!(f, "[")?;
        if let Some(hyphen) = self.0.iter().find(|cr| cr.start == '-') {
            fmt_charclass(f, hyphen)?;
        }
        self.0
            .iter()
            .filter(|&cr| cr.start != '-' && cr.end != '-')
            .try_fold((), |(), cr| fmt_charclass(f, cr))?;
        if let Some(hyphen) = self.0.iter().find(|cr| cr.end == '-' && cr.start != '-') {
            fmt_charclass(f, hyphen)?;
        }
        write!(f, "]")?;
        Ok(())
    }
}

pub fn fmt_escape(f: &mut fmt::Formatter<'_>, c: char) -> Result {
    match c {
        '\x00'..'\x20' | '\x7f' => write!(f, "\\x{:02x}", c as u8),
        ']' => f.write_str("\\]"),
        _ => write!(f, "{}", c.escape_debug()),
    }
}

pub fn fmt_charclass(f: &mut fmt::Formatter<'_>, cr: &CharRange) -> Result {
    fmt_escape(f, cr.start)?;
    if cr.end != cr.start {
        write!(f, "-")?;
        fmt_escape(f, cr.end)?;
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
            // XXX we do UTF-8 byte-oriented printing on literals atm
            (r#"\x00\xe2\x80\x94"#, Node::Literal("\0—".into())),
        ] {
            assert_eq!(want, &format!("{}", Expr::new(root.clone())));
            assert_eq!(root, want.parse().unwrap());
        }
    }
}
