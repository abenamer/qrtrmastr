//! Quartermaster: a grocery order history built from receipt emails, and a recommended next
//! cart. The binary `qm` (`src/main.rs`) is the composition root; this crate holds the three
//! layers it wires together.
//!
//! * [`domain`]: types, invariants and rules. Pure: no I/O, no clock, no database types.
//! * [`application`]: use cases and the ports they need from the outside world.
//! * [`adapters`]: the concrete implementations of those ports (PostgreSQL, the mbox file,
//!   the command line), named for the system each one adapts.
//!
//! The project follows the REAS standard vendored at `standards/reas/`.
#![forbid(unsafe_code)]

pub mod adapters;
pub mod application;
pub mod domain;
