use crate::expr::Expr;

pub struct Site {
    pub url: String,
    pub username: Option<String>,
    pub schema: Expr,
    pub increment: u32,
}
