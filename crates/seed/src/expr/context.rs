use core::{error, fmt};
use std::{collections::HashMap, iter::once, sync::Arc};

use onepass_base::dict::Dict;

use crate::{dict::EFF_WORDLIST, expr::GeneratorFunc};

/// Context for evaluating an <code>[Expr]</code>.
///
/// An instance of this type is needed to evaluate <code>[Generator]</code> invocations. This
/// context also provides a mapping from dictionary hashes to word lists.
///
/// [Expr]: crate::expr::Expr
/// [Generator]: crate::expr::Generator
#[derive(Clone, Debug)]
pub struct Context {
    generator: Arc<HashMap<&'static str, Arc<dyn GeneratorFunc>>>,

    // TODO(someday): maybe this should be extended into a general-purpose content-addressed
    // context map, i.e. `Map<[u8; 32], Any>`.
    dict: Arc<HashMap<[u8; 32], Arc<dyn Dict>>>,

    pub default_dict: Arc<dyn Dict>,

    pub(super) count_rule: CountRule,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CountRule {
    Legacy,
    #[default]
    Current,
}

/// Error returned on unknown generators or dictionary hashes.
#[derive(Clone, Copy, Debug)]
pub struct NotFound;

impl Context {
    pub fn new(
        generator: impl IntoIterator<Item = Arc<dyn GeneratorFunc>>,
        dict: impl IntoIterator<Item = Arc<dyn Dict>>,
        default_dict: Arc<dyn Dict>,
    ) -> Self {
        let generator = Arc::new(generator.into_iter().map(|g| (g.name(), g)).collect());
        let dict = Arc::new(
            once(default_dict.clone())
                .chain(dict)
                .map(|d| (*d.hash(), d))
                .collect(),
        );
        Context {
            generator,
            dict,
            default_dict,
            count_rule: CountRule::default(),
        }
    }

    /// Returns a context without any generators.
    pub fn empty() -> Self {
        Context {
            generator: Arc::default(),
            dict: Arc::default(),
            default_dict: Arc::new(EFF_WORDLIST),
            count_rule: CountRule::default(),
        }
    }

    /// Returns a context with the specified default [`Dict`].
    ///
    /// The dict is added to the lookup table for the returned context. It may or may not be added
    /// to the table for the original context, depending whether there are other clones of the
    /// context or not (see [`Arc::make_mut`].)
    pub fn with_default_dict(&mut self, default_dict: Arc<dyn Dict>) -> Self {
        Arc::make_mut(&mut self.dict).extend([(*default_dict.hash(), default_dict.clone())]);
        Context {
            generator: self.generator.clone(),
            dict: self.dict.clone(),
            default_dict,
            count_rule: self.count_rule,
        }
    }

    /// Returns a context with the pre-v3.3.0 count rule.
    ///
    /// This should only be used to rotate passwords that were impacted by the v3.3.0 count size
    /// bug. WARNING: because [`EvalContext::size`][super::EvalContext::size] cannot return an
    /// error, the code panics when the pre-v3.3.0 count size would have panicked, i.e. when
    /// [`crate::check_legacy_count`] returns [`WouldPanic`][crate::LegacyCountError::WouldPanic].
    pub fn with_legacy_count_rule(self) -> Self {
        Context {
            count_rule: CountRule::Legacy,
            ..self
        }
    }

    pub fn dict_hash(args: &[&str]) -> Option<[u8; 32]> {
        let mut out = [0u8; 32];
        for &arg in args {
            if hex::decode_to_slice(arg, &mut out).is_ok() {
                return Some(out);
            }
        }
        None
    }

    pub fn get_generator(&self, name: &str) -> Result<Arc<dyn GeneratorFunc>, NotFound> {
        self.generator.get(name).map(Arc::clone).ok_or(NotFound)
    }

    pub fn get_dict(&self, hash: &Option<[u8; 32]>) -> Result<Arc<dyn Dict>, NotFound> {
        let Some(hash) = hash else {
            return Ok(self.default_dict.clone());
        };
        self.dict.get(hash).map(Arc::clone).ok_or(NotFound)
    }
}

impl Extend<Arc<dyn Dict>> for Context {
    fn extend<T: IntoIterator<Item = Arc<dyn Dict>>>(&mut self, iter: T) {
        let dict = Arc::make_mut(&mut self.dict);
        dict.extend(iter.into_iter().map(|d| (*d.hash(), d)));
    }
}

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("item not found")
    }
}

impl error::Error for NotFound {}
