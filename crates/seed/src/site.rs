use crate::expr::Expr;

pub struct Site<'a> {
    pub url: String,
    pub username: Option<String>,
    pub schema: Expr<'a>,
    pub increment: u32,
}
