use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{ID_SQL_TYPE, ORDER_SQL_TYPE},
};
use rltbl_db::{any::AnyPool, core::DbQuery};

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait TsvTable {
    fn name(&self) -> &str;

    fn id(&self) -> &str;

    fn pool(&self) -> &AnyPool;

    fn columns(&self) -> Columns;

    fn ddl(&self) -> String {
        let mut ddls = vec![];

        let content_columns = self.columns();

        // The regular table includes _id and _order columns.
        let mut regular_columns: Columns = vec![
            ColumnBuilder::new(self.name(), "_id")
                .sql_type("SERIAL")
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.name(), "_order")
                .sql_type(ORDER_SQL_TYPE)
                .unique(true)
                .build()
                .unwrap(),
        ]
        .into();
        regular_columns.extend(content_columns.iter().cloned());
        ddls.push(regular_columns.ddl(&self.pool().kind(), self.id()));

        // The alternate table includes _id, _order, and _deleted columns,
        // plus a _text column for each column that is not already TEXT.
        let mut alternate_columns: Columns = vec![
            ColumnBuilder::new(self.name(), "_id")
                .sql_type(ID_SQL_TYPE)
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.name(), "_order")
                .sql_type(ORDER_SQL_TYPE)
                .unique(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.name(), "_deleted")
                .sql_type("BOOL")
                .build()
                .unwrap(),
        ]
        .into();
        for column in content_columns.iter() {
            alternate_columns.push(column.clone());
            if column.sql_type.to_uppercase() != "TEXT" {
                let mut alt = column.clone();
                alt.column = format!("{}_text", alt.column);
                alt.sql_type = "TEXT".to_string();
                alternate_columns.push(alt);
            }
        }
        let table_name = format!("{}_alt", self.id());
        ddls.push(alternate_columns.ddl(&self.pool().kind(), &table_name));

        ddls.join("\n")
    }

    /// Create all SQL tables and views for this TsvTable.
    async fn create(&self) -> Result<()> {
        self.pool().execute_batch(&self.ddl()).await?;
        Ok(())
    }

    /// Drop all tables and views for this TsvTable.
    async fn drop(&self) -> Result<()> {
        let mut table_names = vec![self.id().to_string(), format!("{}_alt", self.id())];
        table_names.reverse();
        for table_name in table_names {
            self.pool().drop_table(&table_name).await?;
        }
        Ok(())
    }
}
