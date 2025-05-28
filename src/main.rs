mod randexp;

use anyhow::Result;
use randexp::Expr;

fn main() -> Result<()> {
    let _ = Expr::parse("[:word:]")?;
    Ok(())
}
