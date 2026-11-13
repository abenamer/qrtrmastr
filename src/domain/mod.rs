//! The domain: types, invariants and rules.
//!
//! Nothing in this module performs I/O, reads the clock, or names a database, file or network
//! type (REAS-ARCH-002, REAS-ARCH-005). A function here takes values and returns values; the
//! application layer loads the data and calls it, and an adapter serializes the result.

pub mod identity;
pub mod ids;
pub mod money;
pub mod receipt;
pub mod recommend;
pub mod states;
