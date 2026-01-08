use core::fmt::{Display, Write};
use std::{
    cmp::Ordering,
    io::{Error, Result},
    mem,
};

use argon2::{Algorithm, Argon2, Params, Version};
use blake2::{Blake2b256, Digest};
use chacha20::ChaCha20Rng;
use crypto_bigint::{Encoding, NonZero, U256};
use onepass_base::fmt::DigestWriter;
use rand_core::{RngCore, SeedableRng};
use zeroize::Zeroizing;

use crate::{data::Site, expr::Eval, write_tsv};

impl Site<'_> {
    pub fn password(&self, seed_password: &str) -> Result<Zeroizing<String>> {
        let size = self.schema.size();
        let secret = self.secret(seed_password);
        let index = secret_uniform(&secret, &size);
        // Write to a pre-allocated buffer to prevent reallocations leaking sensitive data.
        let mut buf = Zeroizing::new(vec![0u8; 2048]);
        // XXX double borrow to get a `&mut dyn Write`, as
        // `Write` is implemented on `&mut [u8]`, not `[u8]`.
        self.schema.write_to(&mut &mut buf[..], index)?;
        if let Some(pos) = buf.iter().position(|&b| b == 0) {
            buf.truncate(pos);
        }
        let _ = str::from_utf8(&buf).map_err(Error::other)?;
        let buf = mem::take(&mut *buf);
        Ok(Zeroizing::new(unsafe { String::from_utf8_unchecked(buf) }))
    }

    pub fn salt(&self) -> [u8; 32] {
        let mut w = DigestWriter(Blake2b256::new());
        write!(w, "{}", Derivation(self)).unwrap();
        w.0.finalize().into()
    }

    pub fn secret(&self, seed_password: &str) -> Zeroizing<[u8; 32]> {
        let params = Params::new(256 * 1024, 4, 4, None).unwrap();
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let salt = self.salt();
        let mut out = Zeroizing::new([0u8; 32]);
        argon2
            .hash_password_into(seed_password.as_bytes(), &salt, &mut *out)
            .unwrap();
        out
    }
}

pub fn secret_uniform(secret: &[u8; 32], n: &NonZero<U256>) -> Zeroizing<U256> {
    let mut rng = ChaCha20Rng::from_seed(*secret);
    let mut n_bits = n.bits_vartime();
    if n_bits == 1 {
        return U256::ZERO.into();
    }

    // For powers of 2, we do not need rejection-sampling.
    // We can merely generate `n_bits - 1` random bits.
    if n.trailing_zeros_vartime() == n_bits - 1 {
        n_bits -= 1;
    }
    let n_bits = n_bits;

    let mut ret = U256::ZERO.to_le_bytes();
    let n_bytes = n_bits.div_ceil(8) as usize;
    let hi_mask = !0 >> ((8 - (n_bits % 8)) % 8);

    loop {
        rng.fill_bytes(&mut ret[..n_bytes]);
        ret[n_bytes - 1] &= hi_mask;
        let ret = Zeroizing::new(U256::from_le_bytes(ret));
        if ret.cmp(n) == Ordering::Less {
            return ret;
        }
    }
}

pub struct Derivation<'a, 'b>(pub &'a Site<'b>);

impl Display for Derivation<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.0;
        let url = &s.url;
        let username = s.username.as_deref().unwrap_or("");
        let schema = &s.schema;
        let increment = s.increment;
        write_tsv!(f, "v3", url, username, schema, increment)
    }
}

#[cfg(test)]
mod tests {
    use crate::expr::Expr;

    use super::*;

    fn test_site() -> Site<'static> {
        Site {
            url: "https://google.com/".into(),
            username: None,
            schema: Expr::new("{words}".parse().unwrap()),
            increment: 0,
        }
    }

    #[test]
    fn derivation_works() {
        assert_eq!(
            "v3\thttps://google.com/\t\t{words:323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd}\t0",
            &format!("{}", Derivation(&test_site()))
        );
    }

    #[test]
    fn salt_works() {
        assert_eq!(
            "1bbfdc8d16e76a78d65a37402fd4966a367e1f4740787e8126b42b3f6e5fc67a",
            hex::encode(test_site().salt())
        );
        let mut site2 = test_site();
        site2.username = Some("me@example.com".into());
        assert_eq!(
            "9653ee2d8cc225dfc9902d4c967619dcff2aca60b56c75cb3f04f224adeb64fc",
            hex::encode(site2.salt())
        );
    }

    #[test]
    #[ignore] // too slow in debug
    fn secret() {
        assert_eq!(
            "bdff45de9afd8b221ba249dcf12ad2739daa18bf53f7a9ba712f4d6b044c437b",
            hex::encode(test_site().secret("testpass"))
        );
        assert_eq!(
            "28ec03675de3b3501a75ca2bb25f29311eb767e5016e734b77b9af81e36f6d92",
            hex::encode(test_site().secret("testpass2"))
        );
        let mut site2 = test_site();
        site2.increment = 1;
        site2.username = Some("you@example.com".into());
        assert_eq!(
            "27ebb569a8e97fb64aaad70ca1dee5538fd2008a1ed45ab359ea9fcbc12f2736",
            hex::encode(site2.secret("testpass"))
        );
    }

    #[test]
    fn secret_uniform_short() {
        let tests = [(1, 0x3c5), (2, 0xf6a), (3, 0x180), (4, 0x390), (5, 0x19d)];
        for (seed, want) in tests {
            let secret = U256::from_u32(seed).to_le_bytes();
            let n = NonZero::new(U256::from_u32(0x1000)).unwrap();
            assert_eq!(U256::from_u32(want), *secret_uniform(&secret, &n));
        }
    }

    #[test]
    fn secret_uniform_vectors() {
        let tests: [(&str, &str, &str); _] = [
            (
                "0000000000000000000000000000000000000000000000000000000000000000",
                "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
                "C70D778BCCEF36A81AED8DA0B819D2BD28BD8653E56A5D40903DF1A0ADE0B876",
            ),
            (
                "0123456789abcdeffedcba98765432100123456789abcdeffedcba9876543210",
                "0000000000000000000000000000000000000000000000000000000000100000",
                "000000000000000000000000000000000000000000000000000000000005D415",
            ),
            (
                "0123456789abcdeffedcba98765432100123456789abcdeffedcba9876543210",
                "295A7969D28101E13473A8DD15E68D28CCD4F578591D8994008C5D999F85D416",
                "295A7969D28101E13473A8DD15E68D28CCD4F578591D8994008C5D999F85D415",
            ),
            (
                "0123456789abcdeffedcba98765432100123456789abcdeffedcba9876543210",
                "295A7969D28101E13473A8DD15E68D28CCD4F578591D8994008C5D999F85D415",
                "0D313C0A2DDB1AE37A6EF3ECC18F8588FB946C5BE4A31B39784D7C9530E31D51",
            ),
            (
                "0000000000000000000000000000000000000000000000000000000000000000",
                "0000000000000000000000000000000000000000000000000000000000000001",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
            (
                "a96d610f969d8befcc5a8f7db635976eeb5c83718a2a0d9974a4bb1b6423fac9",
                "00000000000000001FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
                "00000000000000000B6CE7C37CBAA4C1133D97B36751CCE9AA56B264F9E8698D",
            ),
        ];
        for (sec, siz, want) in tests {
            let sec = U256::from_be_hex(sec);
            assert_eq!(
                U256::from_be_hex(want),
                *secret_uniform(
                    &sec.to_be_bytes(),
                    &NonZero::new(U256::from_be_hex(siz)).unwrap()
                ),
            );
        }
    }

    #[test]
    #[ignore]
    fn password_e2e() {
        assert_eq!(
            "nature swirl unusable zookeeper wind",
            &*test_site().password("testpass").unwrap()
        );
    }
}
