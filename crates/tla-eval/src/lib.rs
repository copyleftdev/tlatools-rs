//! Evaluate TLA+ at concrete states.
//!
//! The question this crate answers is not "what states can this specification
//! reach?" but "does this predicate hold *here*?" — where *here* is a state, or
//! a pair of states for an action. That is enough to decide the obligations a
//! refinement oracle actually has, and unlike reachability it needs no search.
//!
//! ```
//! use std::collections::BTreeMap;
//! use tla_eval::{Evaluator, Spec, Value};
//!
//! let spec = Spec::parse(
//!     "---- MODULE Counter ----
//!      EXTENDS Naturals
//!      CONSTANT Limit
//!      VARIABLE n
//!      Init == n = 0
//!      Next == n < Limit /\\ n' = n + 1
//!      ========================",
//! )?;
//!
//! let constants = BTreeMap::from([("Limit".to_string(), Value::Int(3))]);
//! let eval = Evaluator::new(&spec, constants)?;
//!
//! let at = |n| BTreeMap::from([("n".to_string(), Value::Int(n))]);
//! assert!(eval.holds_at("Init", &at(0))?);
//! assert!(eval.step_allowed("Next", &at(0), &at(1))?);
//! assert!(!eval.step_allowed("Next", &at(0), &at(2))?);
//! # Ok::<(), tla_eval::Error>(())
//! ```

mod builtin;
mod error;
mod eval;
mod value;

pub use error::{Error, Result};
pub use eval::{Evaluator, MAX_ELEMENTS, Spec, State};
pub use value::{Infinite, Value};
