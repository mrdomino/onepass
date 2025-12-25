pub mod fmt;

/// Write the fields passed to this macro as tab-separated values with all inner TSV-meaningful
/// characters escaped; see [`format_tsv_args`].
#[macro_export]
macro_rules! write_tsv {
    ($w:expr, $($args:tt)+) => {
        $w.write_fmt($crate::format_tsv_args!($($args)+))
    };
}

/// Format the fields passed to this macro as tab-separated values with all inner TSV-meaningful
/// characters escaped; see [`format_tsv_args`].
#[macro_export]
macro_rules! format_tsv {
    ($($args:tt)+) => {
        std::fmt::format($crate::format_tsv_args!($($args)+))
    };
}

/// Format the fields passed to this macro as a [`core::fmt::Arguments`] that formats the fields
/// with TSV characters escaped.
#[macro_export]
macro_rules! format_tsv_args {
    ($first:expr $(, $($rest:tt)+)?) => {
        $crate::format_tsv_args!(
            @build
            "{}",
            ($crate::fmt::TsvField($first))
            $(, $($rest)+)?
        )
    };
    (@build $fmt:expr, ($($args:tt)+)) => {
        core::format_args!($fmt, $($args)+)
    };
    (@build $fmt:expr, ($($args:tt)+), $next:expr $(, $($rest:tt)+)?) => {
        $crate::format_tsv_args!(
            @build
            concat!($fmt, "\t{}"),
            ($($args)+, $crate::fmt::TsvField($next))
            $(, $($rest)+)?
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
