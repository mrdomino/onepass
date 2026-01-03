pub mod chars;
pub mod generator;
pub mod node;
mod util;

use std::io::{Result, Write};

use crypto_bigint::U256;
use zeroize::Zeroizing;

pub trait Eval {
    fn size(&self) -> U256;
    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()>;
}
