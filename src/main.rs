use std::env::args;

use anyhow::Result;
use randexp::Expr;

mod randexp;

fn parse_complete(input: &str) -> Result<Expr> {
    match Expr::parse(input) {
        Ok(("", expr)) => Ok(expr),
        Ok((rem, _)) => anyhow::bail!("remaining input: {rem}"),
        Err(e) => anyhow::bail!("parse error: {e}"),
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = args().skip(1).collect();
    for arg in &args {
        let expr = parse_complete(arg)?;
        let size = expr.size(7776);
        let bits = size.bits();
        println!("{expr:?}\t{size}\t{bits}");
    }
    Ok(())
}
