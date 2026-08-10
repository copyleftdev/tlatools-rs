//! Decide whether an implementation's reachable state graph refines a TLA+
//! specification.
//!
//! Safety refinement decomposes into two obligations, and a third closes the
//! hole they leave:
//!
//! - `init` — the graph's root is a legal initial state of the specification;
//! - `refines` — every edge is a step the specification permits;
//! - `coverage` — the outcomes the task requires are actually reachable, since
//!   an implementation that does nothing refines everything.
//!
//! All three are decided by evaluating the specification at states that are
//! already known, so none of them searches a state space.

mod check;
mod schema;

pub use check::{ConstantSpec, Edge, EdgeReport, Job, Report, Stats, Status, check};
pub use schema::{DecodeError, Schema, decode, decode_state};
