//! Adapters: the concrete implementations of the ports, each named for the system it adapts
//! (REAS-ARCH-004), and the inbound adapters that turn external input into use-case calls.

pub mod cli;
pub mod config;
pub mod postgres;
