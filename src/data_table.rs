use crate::{self as rltbl};
use rltbl::{
    column::{Column, ColumnBuilder, Columns},
    core::{Relatable, RelatableError, RowID},
    table::Table,
    tsv_table::{TsvTable, UserRow, UserRows},
};
use rltbl_db::{AnyPool, IntoValues, Row as DbRow, Value as DbValue};

use anyhow::Result;
use indexmap::IndexMap;
use serde::Serialize;
use std::iter::zip;

/// Represents the special "table" table.
#[derive(Clone, Debug)]
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

enum ColumnOrBuilder {
    Column(Column),
    Builder(ColumnBuilder),
}

pub struct DataTableBuilder {
    table: Table,
    table_id: RowID,
    columns: IndexMap<String, ColumnOrBuilder>,
    rows: Vec<UserRow>,
    force: bool,
}

impl DataTableBuilder {
    pub fn from(table: Table) -> Self {
        let table_id = table.id;
        Self {
            table,
            table_id,
            columns: IndexMap::new(),
            rows: vec![],
            force: false,
        }
    }

    pub fn from_path(path: &str) -> Self {
        Self {
            table: Table::from_path(path),
            table_id: 1,
            columns: IndexMap::new(),
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

    pub fn columns(&mut self, columns: IndexMap<&String, &Column>) -> &mut Self {
        self.columns = columns
            .into_iter()
            .map(|(k, v)| (k.clone(), ColumnOrBuilder::Column(v.clone())))
            .collect();
        self
    }

    /// Create a ColumnBuilder for this table.
    pub fn column(&mut self, column_name: &str) -> &mut ColumnBuilder {
        self.columns.insert(
            column_name.to_string(),
            ColumnOrBuilder::Builder(ColumnBuilder::new(&self.table.name, column_name)),
        );
        match self.columns.get_mut(column_name).unwrap() {
            ColumnOrBuilder::Column(_) => unreachable!(),
            ColumnOrBuilder::Builder(column_builder) => column_builder,
        }
    }

    /// Create a row for this table from a list of values that can be turned into DbValues.
    pub fn row(&mut self, values: impl IntoValues) -> Result<&mut UserRow> {
        let values: Vec<DbValue> = values.into_values()?.collect();
        if self.columns.len() != values.len() {
            return Err(RelatableError::DataError(format!(
                "Expected {} values but found {}",
                self.columns.len(),
                values.len()
            ))
            .into());
        }
        let db_row = UserRow {
            map: zip(self.columns.keys(), values)
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        };
        self.rows.push(db_row);
        Ok(self.rows.last_mut().unwrap())
    }

    pub fn to_row(&mut self, row: &impl Serialize) -> Result<&mut UserRow> {
        let row: DbRow = rltbl_db::serde::to_row(row)?;
        let mut map = IndexMap::new();
        for key in self.columns.keys() {
            if let Some(value) = row.get(key) {
                map.insert(key.to_string(), value.clone());
            }
        }
        let db_row = UserRow { map };
        self.rows.push(db_row);
        Ok(self.rows.last_mut().unwrap())
    }

    /// Given a database pool, create a DataTable.
    pub fn connect<'a>(self, pool: &'a AnyPool) -> Result<DataTable<'a>> {
        let mut columns = Vec::new();
        for column in self.columns.values() {
            let column = match column {
                ColumnOrBuilder::Column(column) => column.clone(),
                ColumnOrBuilder::Builder(column_builder) => column_builder.clone().build()?,
            };
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
        let new_table = rltbl.table_table().await?.add(self.table.clone()).await?;
        let mut table = self.build(rltbl).await?;
        table.id = new_table.id;
        let refs: Vec<&Column> = table.columns.iter().collect();
        table.columns = rltbl.column_table().await?.add(&refs).await?.into();
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
    use crate::core::RelatableBuilder;

    use super::*;
    use anyhow::Result;
    use rltbl_db::values;

    #[tokio::test]
    async fn test_data_table_builder() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_data_table_builder")
            .init(true)
            .await?;

        let mut builder = DataTableBuilder::from_path("island.tsv");
        builder.column("island_id").datatype("integer");
        builder.column("island");
        builder.row(values![1, "Torgersen"])?;
        builder.row(["foo", "Biscoe"])?;
        builder.create(&rltbl).await?;

        rltbl.drop_test().await
    }
}
