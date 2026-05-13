use crate as rltbl;
use rltbl::column::Columns;
use rltbl_db::{any::AnyPool, core::DbQuery};

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait SimpleTable {
    fn table_name(&self) -> &str;

    fn pool(&self) -> &AnyPool;

    fn columns(&self) -> Columns;

    fn ddl(&self) -> String {
        self.columns().ddl(&self.pool().kind(), self.table_name())
    }

    /// Drop the datatype table from the database.
    async fn drop(&self) -> Result<()> {
        self.pool().drop_table(&self.table_name()).await?;
        Ok(())
    }

    /// Create the "datatype" table in the database
    /// and insert the built-in datatypes.
    async fn create(&self) -> Result<()> {
        self.pool().execute(&self.ddl(), ()).await?;
        Ok(())
    }
}
