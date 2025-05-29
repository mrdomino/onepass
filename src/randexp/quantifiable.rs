use anyhow::Result;
use crypto_bigint::U256;
use zeroize::Zeroizing;

pub(crate) trait Quantifiable<Node> {
    fn size(&self, node: &Node) -> U256;
}

pub(crate) trait Enumerable<Node>: Quantifiable<Node> {
    fn gen_at(&self, node: &Node, index: U256) -> Result<Zeroizing<String>>;
}
