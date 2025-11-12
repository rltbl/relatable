//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use std::collections::HashSet;

use crate as rltbl;
use rltbl::{
    column::{Column, Columns},
    core::RelatableError,
    datatype::Datatypes,
    structure::Structure,
    table::Table,
};

use anyhow::Result;
use indexmap::IndexMap;
use itertools::Itertools;
use rltbl_db::core::JsonRow;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Schema {
    pub tables: IndexMap<String, Table>,
    pub columns: Columns,
    pub datatypes: Datatypes,
}

impl Schema {
    /// Get a table.
    pub fn table(&self, table_name: &str) -> Result<&Table> {
        self.tables
            .get(table_name)
            .ok_or(RelatableError::MissingError(format!("No such table {table_name}")).into())
    }

    /// Get a column for a table, if it exists.
    pub fn column(&self, table_name: &str, column_name: &str) -> Option<&Column> {
        self.columns
            .iter()
            .filter(|col| &col.table == table_name)
            .filter(|col| &col.column == column_name)
            .nth(0)
    }

    /// Get the columns for the given table as a map from column name to column.
    pub fn columns(&self, table_name: &str) -> IndexMap<&String, &Column> {
        self.columns
            .iter()
            .filter(|col| &col.table == table_name)
            .map(|col| (&col.column, col))
            .collect()
    }

    /// Use the columns configuration for the given table to lookup the
    /// nulltype of the given column, and then if the given value matches the
    /// column's nulltype, set it to [Null](JsonValue::Null)
    pub fn nullify_value(
        &self,
        table_name: &str,
        column_name: &str,
        value: &JsonValue,
    ) -> JsonValue {
        match self
            .column(table_name, column_name)
            .and_then(|c| Some(c.nulltype.to_owned()))
            .unwrap_or_default()
            .as_str()
        {
            "" => value.clone(),
            "empty" => match value {
                JsonValue::String(s) if s == "" => JsonValue::Null,
                value => value.clone(),
            },
            nulltype => {
                tracing::warn!("Unsupported nulltype: '{nulltype}'");
                value.clone()
            }
        }
    }

    pub fn nullify_row(&self, table_name: &str, row: &JsonRow) -> JsonRow {
        let mut nullified_row = JsonRow::new();
        for (column_name, value) in row.iter() {
            nullified_row.insert(
                column_name.to_owned(),
                self.nullify_value(table_name, column_name, value),
            );
        }
        tracing::debug!("Nullified row: {row:?} to: {nullified_row:?}");
        nullified_row
    }

    /// Given the full set of tables,
    /// return a list of the tables that this table depends on,
    /// because of `from()` structures in its columns.
    pub fn table_depends_on(&self, table_name: &str) -> HashSet<String> {
        self.columns
            .iter()
            .filter(|col| &col.table == table_name)
            .filter_map(|col| {
                for structure in col.structure.iter() {
                    #[allow(irrefutable_let_patterns)]
                    if let Structure::From(t, _) = structure {
                        return t.clone();
                    }
                }
                None
            })
            .collect()
    }

    /// Given a map of all the tables in the database,
    /// return a list of tables that depend on this table
    /// because of `from()` structures on their columns.
    /// Dependencies are recursive and in order, with no duplicates.
    pub fn dependent_tables(&self, table_name: &str) -> Vec<&Table> {
        let mut dependent_tables = Vec::new();
        for table in self.tables.values() {
            if self.table_depends_on(&table.name).contains(table_name) {
                dependent_tables.push(table);
                // TODO: This is probably not correct.
                // dependent_tables.extend(self.dependent_tables(tables));
            }
        }
        dependent_tables
            .into_iter()
            .unique_by(|table| table.name.clone())
            .collect()
    }
}
