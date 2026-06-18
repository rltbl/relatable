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

/// An abstraction over SQL result sets.
pub mod result_set;

/// An abstraction over a website / web application.
pub mod site;

/// Git interface
pub mod git;

/// Trait for simple SQL tables.
pub mod simple_table;

/// Trait for TSV tables.
pub mod tsv_table;

/// User accounts and cursors.
pub mod user;

/// Data editing changes.
pub mod change;

/// Struct for general, editable tables.
pub mod data_table;

/// Data editing history.
pub mod history;

/// Validation messages.
pub mod message;

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

#[macro_export]
macro_rules! user_row {
    ($($key:expr => $value:expr,)+) => {
        crate::tsv_table::UserRow { map: indexmap::indexmap!($($key.to_string() => $value.into()),+) }
    };
    ($($key:expr => $value:expr),*) => {
        {
            const CAP: usize = <[()]>::len(&[$({ stringify!($key); }),*]);
            let mut _map = indexmap::IndexMap::with_capacity(CAP);
            $(
                let _ = _map.insert($key.to_string(), $value.into());
            )*
                UserRow { map: _map }
        }
    };
}
