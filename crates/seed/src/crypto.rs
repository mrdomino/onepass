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
        // Write to a fixed-size buffer to avoid reallocations leaking data.
        let mut buf = Zeroizing::new(vec![0u8; 4096]);
        self.schema.write_to(&mut &mut *buf, index)?;
        if let Some(pos) = buf.iter().position(|&b| b == 0) {
            buf.truncate(pos);
        }
        let _ = str::from_utf8(&buf).map_err(Error::other)?;
        let buf = mem::take(&mut *buf);
        let res = unsafe { String::from_utf8_unchecked(buf) };
        Ok(Zeroizing::new(res))
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
            schema: Expr::new("{word}".parse().unwrap()),
            increment: 0,
        }
    }

    #[test]
    fn derivation_works() {
        assert_eq!(
            "v3\thttps://google.com/\t\t{word:323606b363ebdedff9f562cb84c50df1a21cbd4b597ff4566df92bb9f2cefdfd}\t0",
            &format!("{}", Derivation(&test_site()))
        );
    }

    #[test]
    fn salt_works() {
        assert_eq!(
            "d02d5004a00973dcb1e0707e31626014a13beb7f7dcb36d88bebf37101f43342",
            hex::encode(test_site().salt())
        );
        let mut site2 = test_site();
        site2.username = Some("me@example.com".into());
        assert_eq!(
            "35be685c1c577cf6604164666e01ec01db080de08d942c67e5c44ab14d307bf0",
            hex::encode(site2.salt())
        );
    }

    #[test]
    #[ignore] // too slow in debug
    fn secret() {
        assert_eq!(
            "a96d610f969d8befcc5a8f7db635976eeb5c83718a2a0d9974a4bb1b6423fac9",
            hex::encode(test_site().secret("testpass"))
        );
        assert_eq!(
            "cd319a18cd2e86ef74805e91ce0b74b52db8b9b6252bc6dbb38cd9c3fdc07191",
            hex::encode(test_site().secret("testpass2"))
        );
        let mut site2 = test_site();
        site2.increment = 1;
        site2.username = Some("you@example.com".into());
        assert_eq!(
            "dc4354071b6b73bc8021f2b9d190298155fe79e8eff746a7290299110899c8e4",
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
}
