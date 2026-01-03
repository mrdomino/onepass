use crypto_bigint::{U256, Word};
use zeroize::Zeroizing;

pub(super) fn u256_to_word(x: &U256) -> Word {
    assert!(x.bits_vartime() <= Word::BITS);
    x.as_words()[0]
}

pub(super) fn u256_saturating_pow(base: &U256, mut n: Word) -> U256 {
    let mut res = U256::ONE;
    if n == 0 {
        return res;
    }
    let mut base = Zeroizing::new(*base);
    while n > 0 {
        if n & 1 == 1 {
            res = res.saturating_mul(&base);
        }
        n >>= 1;
        *base = base.saturating_mul(&base);
    }
    res
}
