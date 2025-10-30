//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[column](crate::column)).

use std::str::FromStr;

use crate as rltbl;
use rltbl::{
    core::RelatableError,
    datatype::Datatype,
    sql::{self, DbTransaction},
    structure::Structure,
    table::Table,
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Represents a column from some table
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub table: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
    pub datatype: Datatype,
    pub datatype_hierarchy: Vec<Datatype>,
    pub nulltype: Option<Datatype>,
    pub structure: Option<Structure>,
}

impl Column {
    /// Get the columns, either from the same or from another table, that depend on this column,
    /// using the given transaction
    pub fn _get_dependent_columns(&self, tx: &mut DbTransaction<'_>) -> Result<Vec<Self>> {
        tracing::trace!("Column::_get_dependent_columns({self:?}, tx)");

        if !Table::_table_exists("column", tx)? {
            tracing::debug!("No column table found");
            return Ok(vec![]);
        }

        tracing::debug!(
            "Looking through column table for dependent columns of '{}.{}'",
            self.table,
            self.name
        );

        let sql = format!(
            r#"SELECT * FROM "column" WHERE "structure" {is_not} NULL"#,
            is_not = sql::is_not_clause(&tx.kind())
        );
        let mut dependent_columns: Vec<Column> = vec![];
        for row in &tx.query(&sql, None)? {
            let dependent_table = Table::_get_table(&row.get_string("table")?, tx)?;
            let Structure::From(structure_table, structure_column) =
                Structure::from_str(&row.get_string("structure")?)?;
            let structure_table = structure_table.unwrap_or(dependent_table.name.to_string());
            if structure_table == self.table && structure_column == self.name {
                let dependent_column = row.get_string("column")?;
                let dependent_column = match dependent_table.columns.get(&dependent_column) {
                    Some(col) => col.clone(),
                    None => {
                        return Err(RelatableError::DataError(format!(
                            "No column found: '{dependent_column}'"
                        ))
                        .into());
                    }
                };
                let mut indirect_deps = dependent_column._get_dependent_columns(tx)?;
                dependent_columns.push(dependent_column);
                dependent_columns.append(&mut indirect_deps);
            }
        }
        tracing::debug!(
            "Column '{}.{}' has the following dependent columns: {dependent_columns:#?}",
            self.table,
            self.name
        );
        Ok(dependent_columns)
    }
}
