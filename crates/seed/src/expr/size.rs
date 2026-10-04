use crypto_bigint::{CheckedSub, NonZero, One, U64, U256, U512};

use crate::expr::EvalContext;

use super::{Context, Node, util::u256_saturating_pow};

pub(crate) struct Sizes {
    pub(crate) old: NonZero<U256>,
    pub(crate) new: NonZero<U256>,
    pub(crate) changed: bool,
}

/// Returns the current and legacy sizes of this node, or [`None`] if the legacy size would panic.
///
/// [`Sizes::changed`] is true if any internal node has a different size under the legacy behavior
/// compared with the current behavior. Even though the final node may be the same size (e.g.
/// [`U256::MAX`] under both rules, differences in sizes of inner nodes will cause different
/// passwords to be generated.
pub(crate) fn sizes(context: &Context, node: &Node) -> Option<Sizes> {
    match *node {
        Node::Count(ref node, min, max) => {
            let Sizes { old, new, changed } = sizes(context, node)?;
            #[allow(deprecated)]
            let old = count_size_legacy(&old, min, max)?;
            let new = count_size(&new, min, max);
            Some(Sizes {
                old,
                new,
                changed: changed || old != new,
            })
        }

        Node::List(ref nodes) => {
            let parts = nodes
                .iter()
                .map(|node| sizes(context, node))
                .collect::<Option<Vec<_>>>()?;
            let old = list_size(parts.iter().map(|p| p.old));
            let new = list_size(parts.iter().map(|p| p.new));
            let changed = parts.into_iter().any(|p| p.changed);
            Some(Sizes { old, new, changed })
        }

        Node::Literal(_) | Node::Chars(_) | Node::Generator(_) => {
            let size = node.size(context);
            Some(Sizes {
                old: size,
                new: size,
                changed: false,
            })
        }
    }
}

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

/// Compute the size of a count node as onepass pre-v3.3.0 would.
///
/// Returns [`None`] when the old code would have panicked.
#[deprecated = "for computing pre-v3.3.0 passwords only"]
pub(crate) fn count_size_legacy(n: &NonZero<U256>, min: u32, max: u32) -> Option<NonZero<U256>> {
    let (min, max) = (min as u64, max as u64);
    if n.is_one().into() {
        return NonZero::new((max - min + 1).into()).into_option();
    }
    let k = min;
    let l = max;
    let mut x = U256::ZERO;
    u256_saturating_pow(n, l + 1, &mut x);
    let mut y = U256::ZERO;
    u256_saturating_pow(n, k, &mut y);
    if x == U256::MAX && y == U256::MAX {
        return Some(NonZero::MAX);
    }
    x = x.checked_sub(&y).into_option()?;
    let (x, rem) = x.div_rem(&NonZero::new(n.saturating_sub(&U256::ONE)).into_option()?);
    if !bool::from(rem.is_zero()) {
        return None;
    }
    NonZero::new(x).into_option()
}

pub(crate) fn list_size(parts: impl Iterator<Item = NonZero<U256>>) -> NonZero<U256> {
    NonZero::new(parts.fold(U256::ONE, |acc, n| acc.saturating_mul(&n))).unwrap()
}
