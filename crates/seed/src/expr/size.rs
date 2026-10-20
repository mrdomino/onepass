use crypto_bigint::{NonZero, One, U64, U256, U512};

/// Compute the size of a count node.
pub(crate) fn count_size(n: &NonZero<U256>, min: u32, max: u32) -> NonZero<U256> {
    assert!(min <= max);
    if bool::from(n.is_one()) {
        return NonZero::new(U256::from(max - min) + U256::ONE).unwrap();
    }
    // Closed form of n^k + … + n^l
    //              = n^k (1 + … + n^(l-k))
    //              = n^k (n^(l-k+1) - 1) / (n - 1)
    //              = (n^(l+1) - n^k) / (n - 1)
    let (k, l) = (U64::from(min), U64::from(max));
    let n: U512 = n.resize();
    // If n^(l+1) overflows U512, then either n or n^l must overflow U256.
    let Some(x) = n.checked_pow_vartime(&(l + U64::ONE)).into_option() else {
        return NonZero::MAX;
    };
    let y = n.checked_pow_vartime(&k).unwrap();
    let (q, rem) = (x - y).div_rem(&NonZero::new(n - U512::ONE).unwrap());
    assert!(bool::from(rem.is_zero()));
    if q.bits_vartime() > U256::BITS {
        return NonZero::MAX;
    }
    NonZero::new(q.resize()).unwrap()
}

pub(crate) fn list_size(mut parts: impl Iterator<Item = NonZero<U256>>) -> NonZero<U256> {
    NonZero::new(
        parts
            .try_fold(U256::ONE, |acc, n| acc.checked_mul(&n).into_option())
            .unwrap_or(U256::MAX),
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto_bigint::U256;
    use serde::Deserialize;

    use crate::testing::{COUNT_VECTORS, u256_from_be_hex};

    #[derive(Deserialize)]
    struct Count {
        name: String,
        why: String,
        #[serde(deserialize_with = "u256_from_be_hex")]
        n: Option<NonZero<U256>>,
        min: u32,
        max: u32,
        #[serde(deserialize_with = "u256_from_be_hex")]
        current: Option<NonZero<U256>>,
    }

    #[derive(Deserialize)]
    struct Counts {
        count: Vec<Count>,
    }

    #[test]
    fn test_vectors() {
        for count in toml::from_str::<Counts>(COUNT_VECTORS).unwrap().count {
            assert_eq!(
                count.current.unwrap(),
                count_size(&count.n.unwrap(), count.min, count.max),
                "{}/{}",
                count.name,
                count.why
            );
        }
    }
}
