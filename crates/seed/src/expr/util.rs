use crypto_bigint::{U256, Word};
use secrecy::{ExposeSecret, ExposeSecretMut, SecretBox};

#[cfg(test)]
use super::EvalContext;

pub(super) fn u256_to_word(x: &U256) -> Word {
    assert!(x.bits_vartime() <= Word::BITS);
    x.as_words()[0]
}

pub(super) fn u256_saturating_pow(base: &U256, mut n: Word, res: &mut U256) {
    *res = U256::ONE;
    if n == 0 {
        return;
    }
    let mut base = SecretBox::new(Box::new(*base));
    while n > 0 {
        if n & 1 == 1 {
            *res = res.saturating_mul(base.expose_secret());
        }
        n >>= 1;
        let base = base.expose_secret_mut();
        *base = base.saturating_mul(base);
    }
}

#[cfg(test)]
pub(super) fn format_at_ctx<E: EvalContext>(e: &E, ctx: &E::Context<'_>, index: U256) -> String {
    use std::io::BufWriter;

    let mut buf = BufWriter::new(Vec::new());
    e.write_to(ctx, &mut buf, &mut SecretBox::new(Box::new(index)))
        .unwrap();
    String::from_utf8(buf.into_inner().unwrap()).unwrap()
}
