//! Use cases and the ports they need (REAS-ARCH-001).
//!
//! A use case takes its adapters as type parameters bound by a port trait (REAS-ARCH-006), so
//! the concrete adapter is visible where it is constructed, in `src/main.rs`.

pub mod migrate;
pub mod ports;
