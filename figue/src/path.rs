//! Standard path representation for navigating schemas and ConfigValue trees.
//!
//! A `Path` is a thin wrapper over `Vec<String>`, where each segment is a name.
//! Indices are stringified numbers.

use std::string::String;
use std::vec::Vec;

/// A path into a schema or ConfigValue tree.
pub type Path = Vec<String>;
