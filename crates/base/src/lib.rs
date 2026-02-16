//! Basic text formatting and hashing routines for use in [onepass].
//!
//! This crate mainly exists to provide routines to the `onepass-seed` crate that are used at both
//! build time and runtime by the latter. The provided routines are used to instantiate a
//! compile-time default word list that ships with onepass.
//!
//! [onepass]: https://github.com/mrdomino/onepass

pub mod dict;
pub mod fmt;
