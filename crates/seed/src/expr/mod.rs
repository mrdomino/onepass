pub mod chars;
pub mod fmt;
pub mod generator;
pub mod node;
pub mod parse;
mod util;

use std::{
    io::{Result, Write},
    sync::LazyLock,
};

use crypto_bigint::{NonZero, U256};
use zeroize::Zeroizing;

pub use node::{Context, Node};

pub struct Expr<'a> {
    pub root: Node,
    pub context: Option<Context<'a>>,
}

static DEFAULT_CONTEXT: LazyLock<Context> = LazyLock::new(Context::default);

impl Expr<'_> {
    pub fn new(root: Node) -> Self {
        Expr {
            root,
            context: None,
        }
    }
}

impl<'a> Expr<'a> {
    pub fn with_context(root: Node, context: Context<'a>) -> Self {
        Expr {
            root,
            context: Some(context),
        }
    }

    pub fn get_context(&self) -> &Context<'a> {
        self.context.as_ref().unwrap_or(&DEFAULT_CONTEXT)
    }
}

pub trait Eval {
    fn size(&self) -> NonZero<U256>;
    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()>;
}

impl Eval for Expr<'_> {
    fn size(&self) -> NonZero<U256> {
        self.root.size(self.get_context())
    }

    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        self.root.write_to(self.get_context(), w, index)
    }
}

pub trait EvalContext {
    type Context<'a>: ?Sized + 'a;
    fn size(&self, context: &Self::Context<'_>) -> NonZero<U256>;
    fn write_to(
        &self,
        context: &Self::Context<'_>,
        w: &mut dyn Write,
        index: Zeroizing<U256>,
    ) -> Result<()>;
}
