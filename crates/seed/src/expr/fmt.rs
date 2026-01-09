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

pub fn fmt_literal(f: &mut fmt::Formatter<'_>, s: &str) -> Result {
    let mut pos = 0;
    for (i, b) in s.bytes().enumerate() {
        let escaped = match b {
            b'\\' => "\\\\",
            b'(' => "\\(",
            b')' => "\\)",
            b'[' => "\\[",
            b']' => "\\]",
            b'{' => "{{",
            b'}' => "}}",
            b'|' => "\\|",
            _ => continue,
        };
        if pos != i {
            f.write_str(&s[pos..i])?;
        }
        f.write_str(escaped)?;
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
        for cr in &self.0 {
            fmt_charclass(f, cr)?;
        }
        if let Some(hyphen) = self.0.iter().find(|cr| cr.end == '-' && cr.start != '-') {
            fmt_charclass(f, hyphen)?;
        }
        write!(f, "]")?;
        Ok(())
    }
}

pub fn fmt_escape(f: &mut fmt::Formatter<'_>, c: char) -> Result {
    match c {
        ']' => f.write_str("\\]"),
        '\\' => f.write_str("\\\\"),
        _ => f.write_char(c),
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
