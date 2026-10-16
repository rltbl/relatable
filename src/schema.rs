//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use std::collections::HashSet;

use crate::{
    self as rltbl,
    column::ColumnBuilder,
    core::{ColumnID, TableID},
};
use rltbl::{
    column::{Column, Columns},
    core::RelatableError,
    datatype::Datatypes,
    structure::Structure,
    table::Table,
};

use anyhow::Result;
use indexmap::{indexmap, IndexMap};
use itertools::Itertools;
use rltbl_db::JsonRow;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Schema {
    pub tables: IndexMap<String, Table>,
    pub columns: Columns,
    pub datatypes: Datatypes,
}

impl Default for Schema {
    fn default() -> Self {
        Self {
            tables: indexmap! {
                "table".to_string() => Table {
                    name: "table".to_string(),
                    id: 1,
                    order: 1000,
                    path: "src/schema/table.tsv".to_string(),
                    view: String::new(),
                    editable: true,
                    has_meta: true
                },
                "column".to_string() => Table {
                    name: "column".to_string(),
                    id: 2,
                    order: 2000,
                    path: "src/schema/column.tsv".to_string(),
                    view: String::new(),
                    editable: true,
                    has_meta: true
                },
                "datatype".to_string() => Table {
                    name: "datatype".to_string(),
                    id: 3,
                    order: 3000,
                    path: "src/schema/datatype.tsv".to_string(),
                    view: String::new(),
                    editable: true,
                    has_meta: true
                },
            },
            columns: vec![
                // table
                ColumnBuilder::new("table", "table")
                    .id(1)
                    .description("the name of this table")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("table", "path")
                    .id(2)
                    .description("the path to the source of this file")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                // column
                ColumnBuilder::new("column", "table")
                    .id(3)
                    .description("the table for this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "column")
                    .id(4)
                    .description("the name of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "label")
                    .id(5)
                    .description("the label of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "nulltype")
                    .id(6)
                    .description("the null type of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "datatype")
                    .id(7)
                    .description("the datatype of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "structure")
                    .id(8)
                    .description("the structure of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "description")
                    .id(9)
                    .description("the description of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                // datatype
                ColumnBuilder::new("datatype", "datatype")
                    .id(10)
                    .description("the name of this datatype")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("datatype", "parent")
                    .id(11)
                    .description("the parent datatype")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("datatype", "condition")
                    .id(12)
                    .description("the validation condition")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("datatype", "sql_type")
                    .id(13)
                    .description("the validation condition")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("datatype", "format")
                    .id(14)
                    .description("the SQL type for this datatype")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("datatype", "description")
                    .id(15)
                    .description("the description of this datatype")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
            ]
            .into(),
            datatypes: Datatypes::default(),
        }
    }
}

impl Schema {
    /// Get a table.
    pub fn table(&self, table_name: &str) -> Result<&Table> {
        self.tables
            .get(table_name)
            .ok_or(RelatableError::MissingError(format!("No such table {table_name}")).into())
    }

    pub fn table_by_id(&self, table_id: TableID) -> Option<&Table> {
        self.tables
            .values()
            .filter(|table| table.id == table_id)
            .nth(0)
    }
    /// Get a column for a table, if it exists.
    pub fn column(&self, table_name: &str, column_name: &str) -> Option<&Column> {
        self.columns
            .iter()
            .filter(|col| &col.table == table_name)
            .filter(|col| &col.column == column_name)
            .nth(0)
    }

    pub fn column_by_id(&self, column_id: ColumnID) -> Option<&Column> {
        self.columns.iter().filter(|col| col.id == column_id).nth(0)
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

    /// Nullify each of the values in the given row.
    pub fn nullify_row(&self, table_name: &str, row: &JsonRow) -> JsonRow {
        let mut nullified_row = JsonRow::new();
        for (column_name, value) in row.iter() {
            nullified_row.insert(
                column_name.to_owned(),
                self.nullify_value(table_name, column_name, value),
            );
        }
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
