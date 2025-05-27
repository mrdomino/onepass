use std::env::args;

use anyhow::Result;
use nom::Finish;
use randexp::Expr;

mod randexp;

fn parse_complete(input: &str) -> Result<Expr> {
    let (_, expr) = Expr::parse(input).finish().map_err(|e| {
        anyhow::anyhow!("Parse error at {}: {}", e.input.len(), e.code.description())
    })?;
    Ok(expr)
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
