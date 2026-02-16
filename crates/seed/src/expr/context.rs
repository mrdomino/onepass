use core::{error, fmt};
use std::{collections::HashMap, iter::once, sync::Arc};

use onepass_base::dict::Dict;

use crate::{dict::EFF_WORDLIST, expr::GeneratorFunc};

#[derive(Clone, Debug)]
pub struct Context<'a> {
    generator: Arc<HashMap<&'static str, Arc<dyn GeneratorFunc>>>,

    // TODO(someday): maybe this should be extended into a general-purpose content-addressed
    // context map, i.e. `Map<[u8; 32], Any>`.
    dict: Arc<HashMap<[u8; 32], Arc<dyn Dict + 'a>>>,

    pub default_dict: Arc<dyn Dict + 'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct NotFound;

impl<'a> Context<'a> {
    pub fn new(
        generator: impl IntoIterator<Item = Arc<dyn GeneratorFunc>>,
        dict: impl IntoIterator<Item = Arc<dyn Dict + 'a>>,
        default_dict: Arc<dyn Dict + 'a>,
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
        }
    }

    pub fn empty() -> Self {
        Context {
            generator: Arc::default(),
            dict: Arc::default(),
            default_dict: Arc::new(EFF_WORDLIST),
        }
    }

    pub fn with_default_dict(&self, default_dict: Arc<dyn Dict + 'a>) -> Self {
        let mut dict = self.dict.clone();
        Arc::make_mut(&mut dict).extend([(*default_dict.hash(), default_dict.clone())]);
        Context {
            generator: self.generator.clone(),
            dict,
            default_dict,
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

    pub fn get_dict(&self, hash: &Option<[u8; 32]>) -> Result<Arc<dyn Dict + 'a>, NotFound> {
        eprintln!("get_dict({:?})", hash);
        let Some(hash) = hash else {
            return Ok(self.default_dict.clone());
        };
        self.dict.get(hash).map(Arc::clone).ok_or(NotFound)
    }
}

impl<'a> Extend<Arc<dyn Dict + 'a>> for Context<'a> {
    fn extend<T: IntoIterator<Item = Arc<dyn Dict + 'a>>>(&mut self, iter: T) {
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
