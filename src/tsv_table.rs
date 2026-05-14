use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{RowOrder, ID_SQL_TYPE, NEW_ORDER_MULTIPLIER, ORDER_SQL_TYPE},
};
use rltbl_db::{
    any::AnyPool,
    core::DbQuery,
    db_value::{DbRows, IntoDbRows},
};

use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait TsvTable {
    fn name(&self) -> &str;

    fn id(&self) -> &str;

    fn pool(&self) -> &AnyPool;

    // Does not include meta columns: _id, _order, _deleted.
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

    /// Return the specified number of new RowOrders for a table.
    async fn new_orders(&self, count: usize) -> Result<Vec<RowOrder>> {
        // TODO: Account for history/change table:
        // UNION ALL
        // SELECT MAX(previous_order) AS _order FROM history
        //   WHERE "table" = "{id}"
        let sql = format!(
            r#"SELECT _order FROM (
                SELECT MAX(_order) AS _order FROM "{id}"
                UNION ALL
                SELECT MAX(_order) AS _order FROM "{id}_alt"
                ORDER BY _order DESC
            )
            LIMIT 1"#,
            id = self.id()
        );
        let max_order: RowOrder = self
            .pool()
            .query(&sql, ())
            .await?
            .try_into()
            .unwrap_or_default();
        let new_orders = (1..=count)
            .into_iter()
            .map(|i| max_order + (i as RowOrder * NEW_ORDER_MULTIPLIER))
            .collect();
        Ok(new_orders)
    }

    /// Insert rows to the regular table.
    async fn insert_regular(&self, rows: impl IntoDbRows + Send) -> Result<DbRows> {
        let mut column_names = self.columns().names();
        column_names.insert(0, "_order".to_string());
        // TODO: nullify rows
        let refs: Vec<&str> = column_names.iter().map(|x| x.as_str()).collect();
        let new_rows = self
            .pool()
            .insert_returning(self.id(), &refs, rows, &[])
            .await?;
        Ok(new_rows)
    }
}
