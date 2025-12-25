use core::fmt::{Display, Write};

use argon2::{Algorithm, Argon2, Params, Version};
use blake2::{Blake2b256, Digest};
use onepass_base::fmt::DigestWriter;
use zeroize::Zeroizing;

use crate::{data::Site, write_tsv};

impl Site {
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

struct Derivation<'a>(&'a Site);

impl Display for Derivation<'_> {
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
    use super::*;

    fn test_site() -> Site {
        Site {
            url: "https://google.com/".into(),
            username: None,
            schema: "{placeholder}".into(),
            increment: 0,
        }
    }

    #[test]
    fn derivation_works() {
        assert_eq!(
            "v3\thttps://google.com/\t\t{placeholder}\t0",
            &format!("{}", Derivation(&test_site()))
        );
    }

    #[test]
    fn salt_works() {
        assert_eq!(
            "6305a00d24a5b1551d3ae57054b9346a43399e8419cd8be6e39d59d742a8e193",
            hex::encode(test_site().salt())
        );
        let mut site2 = test_site();
        site2.username = Some("me@example.com".into());
        assert_eq!(
            "0142f0bc29fce5d4d6814c346acc1022f5b02d47528654c18ff80603e7d5776d",
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
    }
}
