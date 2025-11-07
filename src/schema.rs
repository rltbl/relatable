//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use crate as rltbl;
use rltbl::{column::Columns, datatype::Datatypes, table::Table};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Schema {
    pub tables: IndexMap<String, Table>,
    pub columns: Columns,
    pub datatypes: Datatypes,
}
