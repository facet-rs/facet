//! Layered configuration parsing.
//!
//! Each layer parses from a different source (CLI, environment, file) and outputs
//! a `LayerOutput` containing `ConfigValue` with provenance tracking.
//!
//! The layers are:
//! - `cli`: Command-line argument parsing
//! - `env`: Environment variable parsing
//! - `file`: Configuration file parsing

pub mod cli;
pub mod env;
pub mod file;
