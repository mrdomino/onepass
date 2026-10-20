use crypto_bigint::{U256, Word};
#[cfg(test)]
use secrecy::SecretBox;

#[cfg(test)]
use super::EvalContext;

pub(super) fn u256_to_word(x: &U256) -> Word {
    assert!(x.bits_vartime() <= Word::BITS);
    x.as_words()[0]
}

#[cfg(test)]
pub(super) fn format_at_ctx<E: EvalContext>(e: &E, ctx: &E::Context, index: U256) -> String {
    use std::io::BufWriter;

    let mut buf = BufWriter::new(Vec::new());
    e.write_to(ctx, &mut buf, &mut SecretBox::new(Box::new(index)))
        .unwrap();
    String::from_utf8(buf.into_inner().unwrap()).unwrap()
}
