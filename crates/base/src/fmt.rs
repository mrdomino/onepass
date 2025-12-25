use core::fmt::{Display, Formatter, Write};

use digest::Update;

/// Adapter type that allows a hasher to be treated like a [`impl Write`][Write], assuming
/// [`core::fmt::Write`] is imported.
pub struct DigestWriter<T: Update>(pub T);

impl<T: Update> Write for DigestWriter<T> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.update(s.as_bytes());
        Ok(())
    }
}

/// Wrapper type that formats an iterator as newline-separated values. It can be combined with a
/// [`map(TsvField)`][TsvField] on the iterator to ensure that the fields themselves do not contain
/// newlines.
pub struct Lines<I>(pub I);

impl<I> Display for Lines<I>
where
    I: Iterator + Clone,
    I::Item: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0
            .clone()
            .try_fold((), |(), line| write!(f, "{line}\n"))
    }
}

/// Wrapper type that formats a `T` as an escaped tab-separated-values field.
pub struct TsvField<T>(pub T);

impl<T: Display> Display for TsvField<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(TsvEscaper(f), "{}", self.0)
    }
}

/// This type wrapps a [`Formatter`] and implements simple escaping of the semantically meaningful
/// tab-separated-values characters `'\t'`, `'\n'`, `'\r'` and `'\\'`. Writes to instances of this
/// type are forwarded to the underlying `Formatter` except for these characters, which have their
/// ANSI C backslash escaped forms emitted instead.
pub struct TsvEscaper<'a, 'b>(&'a mut Formatter<'b>);

impl TsvEscaper<'_, '_> {
    fn escape(b: u8) -> Option<&'static str> {
        Some(match b {
            b'\\' => r#"\\"#,
            b'\n' => r#"\n"#,
            b'\r' => r#"\r"#,
            b'\t' => r#"\t"#,
            _ => return None,
        })
    }
}

impl Write for TsvEscaper<'_, '_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let mut pos = 0;
        for (i, b) in s.bytes().enumerate() {
            let Some(escaped) = Self::escape(b) else {
                continue;
            };
            if pos < i {
                self.0.write_str(&s[pos..i])?;
            }
            self.0.write_str(escaped)?;
            pos = i + 1;
        }
        if pos < s.len() {
            self.0.write_str(&s[pos..])?;
        }
        Ok(())
    }

    fn write_char(&mut self, c: char) -> std::fmt::Result {
        match u8::try_from(c).ok().and_then(Self::escape) {
            None => self.0.write_char(c),
            Some(s) => self.0.write_str(s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_works() {
        let lines: &[&str] = &["a\nb", "cd", "e", ""];
        assert_eq!("a\nb\ncd\ne\n\n", &format!("{}", Lines(lines.iter())));
        assert_eq!(
            "a\\nb\ncd\ne\n\n",
            &format!("{}", Lines(lines.iter().map(TsvField)))
        );
    }

    #[test]
    fn tsv_field_works() {
        assert_eq!("", &format!("{}", TsvField("")));
        assert_eq!("abcde\\nf", &format!("{}", TsvField("abcde\nf")));
    }
}
