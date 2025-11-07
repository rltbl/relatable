//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl).

///////////////////////////////////////////////////////////////////////////////
// Sub-modules
///////////////////////////////////////////////////////////////////////////////

/// An abstraction over SQL engines
pub mod sql;

/// An abstraction over SQL Select statements
pub mod select;

/// Git interface
pub mod git;

/// Structs for column datatypes
pub mod datatype;

/// Structs for column structures
pub mod structure;

/// Structs for table columns
pub mod column;

/// Structs for table rows
pub mod row;

/// Structs for the overall schema
pub mod schema;

/// Structs for representing tables, contents, changes, results
pub mod table;

/// Core functionality
pub mod core;

/// Command line interface
pub mod cli;

/// Web server
pub mod web;

/// Demonstrations
pub mod demo;

///////////////////////////////////////////////////////////////////////////////
// Global constants and other lookups
///////////////////////////////////////////////////////////////////////////////
