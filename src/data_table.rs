use crate::{self as rltbl};
use rltbl::{
    column::{Column, ColumnBuilder, Columns},
    core::{Relatable, RelatableError, RowID},
    table::Table,
    tsv_table::{TsvTable, UserRow, UserRows},
};
use rltbl_db::{any::AnyPool, db_value::IntoDbParams, serde::to_db_row};

use anyhow::Result;
use indexmap::IndexMap;
use serde::Serialize;
use std::iter::zip;

/// Represents the special "table" table.
#[derive(Debug)]
pub struct DataTable<'a> {
    pub name: String,
    pub id: RowID,
    pub pool: &'a AnyPool,
    pub columns: Columns,
}

impl<'a> TsvTable for DataTable<'a> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn id(&self) -> String {
        format!("t{}", self.id)
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        self.columns.clone()
    }
}

pub struct DataTableBuilder {
    table: Table,
    table_id: RowID,
    first_column_id: RowID,
    column_builders: IndexMap<String, ColumnBuilder>,
    rows: Vec<UserRow>,
    force: bool,
}

impl DataTableBuilder {
    pub fn from_path(path: &str) -> Self {
        Self {
            table: Table::from_path(path),
            table_id: 1,
            first_column_id: 1,
            column_builders: IndexMap::new(),
            rows: vec![],
            force: false,
        }
    }

    /// When true (default false), drop this table before creating.
    pub fn force(&mut self, force: bool) -> &mut Self {
        self.force = force;
        self
    }

    pub fn table_id(&mut self, id: RowID) -> &mut Self {
        self.table_id = id;
        self
    }

    pub fn first_column_id(&mut self, id: RowID) -> &mut Self {
        self.first_column_id = id;
        self
    }

    /// Create a ColumnBuilder for this table.
    pub fn column(&mut self, column_name: &str) -> &mut ColumnBuilder {
        self.column_builders.insert(
            column_name.to_string(),
            ColumnBuilder::new(&self.table.name, column_name),
        );
        self.column_builders.get_mut(column_name).unwrap()
    }

    /// Create a row for this table from a list of values that can be turned into DbValues.
    pub fn row(&mut self, values: impl IntoDbParams) -> Result<&mut UserRow> {
        let values = match values.into_db_params() {
            rltbl_db::db_value::DbParams::None => {
                return Err(RelatableError::DataError(format!("Row requires values")).into());
            }
            rltbl_db::db_value::DbParams::Positional(db_values) => db_values,
        };
        if self.column_builders.len() != values.len() {
            return Err(RelatableError::DataError(format!(
                "Expected {} values but found {}",
                self.column_builders.len(),
                values.len()
            ))
            .into());
        }
        let db_row = UserRow {
            map: zip(self.column_builders.keys(), values)
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        };
        self.rows.push(db_row);
        Ok(self.rows.last_mut().unwrap())
    }

    pub fn to_row(&mut self, row: &impl Serialize) -> Result<&mut UserRow> {
        let row = to_db_row(row)?;
        let mut map = IndexMap::new();
        for key in self.column_builders.keys() {
            if let Some(value) = row.get(key) {
                map.insert(key.to_string(), value);
            }
        }
        let db_row = UserRow { map };
        self.rows.push(db_row);
        Ok(self.rows.last_mut().unwrap())
    }

    /// Given a database pool, create a DataTable.
    pub fn connect<'a>(self, pool: &'a AnyPool) -> Result<DataTable<'a>> {
        let mut columns = Vec::new();
        for (i, column_builder) in self.column_builders.values().enumerate() {
            let id = self.first_column_id + i as RowID;
            let column = column_builder.clone().id(id).build()?;
            columns.push(column);
        }
        for row in self.rows {
            println!("ROW {row:?}");
        }
        Ok(DataTable {
            name: self.table.name.clone(),
            id: self.table.id.clone(),
            pool,
            columns: columns.into(),
        })
    }

    /// Build this table by connecting and updating the column SQL types.
    /// Does not actually create the table.
    pub async fn build<'a>(self, rltbl: &'a Relatable) -> Result<DataTable<'a>> {
        let mut data_table = self.connect(&rltbl.pool)?;
        let datatypes = rltbl.datatypes().await;
        data_table.columns.sql_types(&datatypes);
        Ok(data_table)
    }

    /// Create this table by:
    /// adding it to the table table and getting the new ID,
    /// adding its columns to the column table and getting their IDs,
    /// creating tables and views for this table,
    /// appending any rows to the new table.
    pub async fn create<'a>(mut self, rltbl: &'a Relatable) -> Result<DataTable<'a>> {
        let force = self.force;
        let rows = UserRows { rows: self.rows };
        self.rows = Vec::new(); // move rows
        let new_table = rltbl.table_table().add(self.table.clone()).await?;
        let mut table = self.build(rltbl).await?;
        table.id = new_table.id;
        let refs: Vec<&Column> = table.columns.iter().collect();
        table.columns = rltbl.column_table().add(&refs).await?.into();
        if force {
            table.drop().await?;
        }
        table.create().await?;
        table.append(rows).await?;
        Ok(table)
    }

    // Create just this table, without changing the schema tables.
    pub async fn create_table<'a>(mut self, pool: &'a AnyPool) -> Result<()> {
        let force = self.force;
        let rows = UserRows { rows: self.rows };
        let id = self.table_id;
        self.rows = Vec::new();
        let mut table = self.connect(pool)?;
        table.id = id;
        if force {
            table.drop().await?;
        }
        table.create().await?;
        if rows.rows.len() > 0 {
            table.append(rows).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::core::Relatable;

    use super::*;
    use anyhow::Result;
    use rltbl_db::params;

    #[tokio::test]
    async fn test_data_table_builder() -> Result<()> {
        let rltbl = Relatable::test("test_data_table_builder", false).await?;

        let mut builder = DataTableBuilder::from_path("island.tsv");
        builder.column("island_id").datatype("integer");
        builder.column("island");
        builder.row(params![1, "Torgersen"])?;
        builder.row(["foo", "Biscoe"])?;
        builder.create(&rltbl).await?;

        rltbl.drop_test().await
    }
}
