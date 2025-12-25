/// Write the fields passed to this macro as tab-separated values with all inner TSV-meaningful
/// characters escaped; see [`format_tsv_args`].
#[macro_export]
macro_rules! write_tsv {
    ($w:expr, $($args:expr),+ $(,)?) => {
        $w.write_fmt($crate::format_tsv_args!($($args),+))
    };
}

/// Format the fields passed to this macro as tab-separated values with all inner TSV-meaningful
/// characters escaped; see [`format_tsv_args`].
#[macro_export]
macro_rules! format_tsv {
    ($($args:expr),+ $(,)?) => {
        std::fmt::format($crate::format_tsv_args!($($args),+))
    };
}

/// Present the fields passed to this macro as a [`core::fmt::Arguments`] that yields tab-separated
/// values.
///
/// Each individual argument is passed through with the exception of the following four characters,
/// which are escaped as their ANSI C escape sequences:
///
/// - `\\`
/// - `\n`
/// - `\r`
/// - `\t`
///
/// Tabs are inserted between fields. No trailing tabs or newlines are included.
///
/// The intent of this transform is to be a simple, canonical, reversible text representation of
/// data for use in derivation paths or salt parameters.
#[macro_export]
macro_rules! format_tsv_args {
    () => {
        compile_error!("need at least one field")
    };
    ($first:expr $(, $rest:expr)* $(,)?) => {
        $crate::format_tsv_args!(
            @build
            "{}",
            (onepass_base::fmt::TsvField($first))
            $(, $rest)*
        )
    };
    (@build $fmt:expr, ($($args:expr),+)) => {
        core::format_args!($fmt, $($args),+)
    };
    (@build $fmt:expr, ($($args:expr),+), $next:expr $(, $rest:expr)*) => {
        $crate::format_tsv_args!(
            @build
            concat!($fmt, "\t{}"),
            ($($args,)+ onepass_base::fmt::TsvField($next))
            $(, $rest)*
        )
    };
}

#[cfg(test)]
mod tests {
    use std::io::{BufWriter, Write};

    #[test]
    fn format_tsv_works() {
        assert_eq!("", &format_tsv!(""));
        assert_eq!("\t", &format_tsv!("", ""));
        assert_eq!("a\tb\tc", &format_tsv!("a", "b", "c"));
        assert_eq!("a\\r\\\\\\t\\n\tb", &format_tsv!("a\r\\\t\n", "b"));
    }

    #[test]
    fn write_tsv_works() {
        let mut buf = BufWriter::new(Vec::new());
        write_tsv!(buf, "a\nb", "c\td").unwrap();
        let s = String::from_utf8(buf.into_inner().unwrap()).unwrap();
        assert_eq!("a\\nb\tc\\td", &s);
    }
}
