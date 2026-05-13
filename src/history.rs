use crate as rltbl;
use rltbl::core::{id_ddl, ID_SQL_TYPE};
use rltbl_db::{any::AnyPool, core::DbQuery};

use anyhow::Result;

/// Represents the special "history" table.
pub struct HistoryTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> HistoryTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "history".to_owned(),
            pool,
        }
    }

    pub fn column_names(&self) -> Vec<String> {
        vec!["history_id", "change_id", "table", "row", "before", "after"]
            .into_iter()
            .map(|x| x.to_string())
            .collect()
    }

    /// Get the SQL DDL as a string.
    /// Requires the db only to know the SQL flavour to use.
    pub fn ddl(&self) -> String {
        format!(
            r#"CREATE TABLE "{table_name}" (
              "history_id" {id},
              "change_id" {ID_SQL_TYPE} NOT NULL REFERENCES "change"("change_id"),
              "table" TEXT NOT NULL,
              "row" {ID_SQL_TYPE} NOT NULL,
              "before" TEXT,
              "after" TEXT
            )"#,
            table_name = self.table_name,
            id = id_ddl(&self.pool.kind())
        )
    }

    /// Drop the datatype table from the database.
    pub async fn drop(&self) -> Result<()> {
        self.pool.drop_table(&self.table_name).await?;
        Ok(())
    }

    /// Create the "datatype" table in the database
    /// and insert the built-in datatypes.
    pub async fn create(&self) -> Result<()> {
        self.pool.execute(&self.ddl(), ()).await?;
        Ok(())
    }
}
