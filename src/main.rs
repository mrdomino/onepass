use anyhow::Result;
use randexp::Expr;

mod randexp;

fn main() -> Result<()> {
    let (_, expr) = Expr::parse("[A-Za-z0-9_-]")?;
    println!("{:?}", expr);
    Ok(())
}
