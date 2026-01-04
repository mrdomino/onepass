pub mod chars;
pub mod generator;
pub mod node;
pub mod parse;
mod util;

use std::{
    io::{Result, Write},
    sync::LazyLock,
};

use crypto_bigint::U256;
use zeroize::Zeroizing;

pub use node::{Context, Node};

pub struct Expr {
    pub root: Node,
    pub context: Option<Context>,
}

static DEFAULT_CONTEXT: LazyLock<Context> = LazyLock::new(Context::default);

impl Expr {
    pub fn new(root: Node) -> Self {
        Expr {
            root,
            context: None,
        }
    }

    pub fn with_context(root: Node, context: Context) -> Self {
        Expr {
            root,
            context: Some(context),
        }
    }

    pub fn get_context(&self) -> &Context {
        self.context.as_ref().unwrap_or(&DEFAULT_CONTEXT)
    }
}

pub trait Eval {
    fn size(&self) -> U256;
    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()>;
}

impl Eval for Expr {
    fn size(&self) -> U256 {
        self.root.size(self.get_context())
    }

    fn write_to(&self, w: &mut dyn Write, index: Zeroizing<U256>) -> Result<()> {
        self.root.write_to(self.get_context(), w, index)
    }
}

pub trait EvalContext {
    type Context: ?Sized;
    fn size(&self, context: &Self::Context) -> U256;
    fn write_to(
        &self,
        context: &Self::Context,
        w: &mut dyn Write,
        index: Zeroizing<U256>,
    ) -> Result<()>;
}
