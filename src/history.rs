use crate::{
    self as rltbl,
    change::{ChangeAction, NewChange},
    core::{ColumnID, RowOrder, TableID, ORDER_SQL_TYPE},
};
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{RowID, ID_SQL_TYPE},
    simple_table::SimpleTable,
};
use rltbl_db::{
    any::AnyPool,
    core::{DbError, DbQuery},
    db_value::{DbRow, DbValue},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

// Store either the changes or the undo/redo target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct History {
    pub history_id: RowID,
    pub user: String,
    //datetime
    pub action: ChangeAction,
    pub target: Option<RowID>,
    pub changes: Vec<NewChange>,
}

/// Represents the special "history" table.
pub struct HistoryTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for HistoryTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "history_id")
                .sql_type("SERIAL")
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "user")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "action")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "target")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "changes")
                .sql_type("JSON")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> HistoryTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "history".to_owned(),
            pool,
        }
    }

    pub async fn append(&self, history: History) -> Result<History> {
        let mut row: DbRow = rltbl_db::serde::to_db_row(&history)?;
        row.map.shift_remove("history_id");
        // println!("append history as {row:?}");
        let columns: Vec<&str> = row.keys().map(|c| c.as_str()).collect();
        let rows: Vec<History> = self
            .pool()
            .insert_returning(&self.table_name, &columns, &[&row], &[])
            .await?
            .try_into_vec()?;
        Ok(rows.first().unwrap().clone())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviousValue {
    pub history_id: RowID,
    pub table_id: TableID,
    pub row_id: RowID,
    pub column_id: ColumnID,
    pub value: DbValue,
}

/// Represents the special "history" table.
pub struct PreviousValueTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for PreviousValueTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "history_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "table_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "row_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "column_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "value")
                .sql_type("JSON")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> PreviousValueTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "previous_value".to_owned(),
            pool,
        }
    }

    pub async fn extend(
        &self,
        history_id: RowID,
        table_id: TableID,
        triples: &[(RowID, ColumnID, DbValue)],
    ) -> Result<Vec<PreviousValue>> {
        let rows: Vec<DbRow> = triples
            .iter()
            .cloned()
            .map(|(row_id, column_id, value)| PreviousValue {
                history_id,
                table_id,
                row_id,
                column_id,
                value,
            })
            .map(|po| rltbl_db::serde::to_db_row(&po))
            .collect::<Result<Vec<DbRow>, DbError>>()?;
        let columns = ["history_id", "table_id", "row_id", "column_id", "value"];
        let pos: Vec<PreviousValue> = self
            .pool()
            .insert_returning(&self.table_name, &columns, &rows, &[])
            .await?
            .try_into_vec()?;
        Ok(pos)
    }

    pub async fn get_by_history_id(&self, history_id: RowID) -> Result<Vec<PreviousValue>> {
        let sql = r#"SELECT * FROM "previous_value" WHERE "history_id" = $1;"#;
        let pos: Vec<PreviousValue> = self.pool.query(sql, [history_id]).await?.try_into_vec()?;
        Ok(pos)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreviousOrder {
    pub history_id: RowID,
    pub table_id: TableID,
    pub row_id: RowID,
    pub order: RowOrder,
}

/// Represents the special "history" table.
pub struct PreviousOrderTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for PreviousOrderTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "history_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "table_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "row_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "order")
                .sql_type(ORDER_SQL_TYPE)
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> PreviousOrderTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "previous_order".to_owned(),
            pool,
        }
    }

    pub async fn extend(
        &self,
        history_id: RowID,
        table_id: TableID,
        pairs: &[(RowID, RowOrder)],
    ) -> Result<Vec<PreviousOrder>> {
        let rows: Vec<DbRow> = pairs
            .iter()
            .cloned()
            .map(|(row_id, order)| PreviousOrder {
                history_id,
                table_id,
                row_id,
                order,
            })
            .map(|po| rltbl_db::serde::to_db_row(&po))
            .collect::<Result<Vec<DbRow>, DbError>>()?;
        let columns = ["history_id", "table_id", "row_id", "order"];
        let pos: Vec<PreviousOrder> = self
            .pool()
            .insert_returning(&self.table_name, &columns, &rows, &[])
            .await?
            .try_into_vec()?;
        Ok(pos)
    }

    pub async fn get_by_history_id(&self, history_id: RowID) -> Result<Vec<PreviousOrder>> {
        let sql = r#"SELECT * FROM "previous_order" WHERE "history_id" = $1;"#;
        let pos: Vec<PreviousOrder> = self.pool.query(sql, [history_id]).await?.try_into_vec()?;
        Ok(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;

    #[tokio::test]
    async fn test_history_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = HistoryTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "history" (
  "history_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "user" TEXT NOT NULL,
  "action" TEXT NOT NULL,
  "target" INTEGER,
  "changes" JSON
);"#,
            table.ddl()
        )
    }

    #[tokio::test]
    async fn test_previous_value_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = PreviousValueTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "previous_value" (
  "history_id" INTEGER,
  "table_id" INTEGER,
  "row_id" INTEGER,
  "column_id" INTEGER,
  "value" JSON
);"#,
            table.ddl()
        )
    }

    #[tokio::test]
    async fn test_previous_order_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = PreviousOrderTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "previous_order" (
  "history_id" INTEGER,
  "table_id" INTEGER,
  "row_id" INTEGER,
  "order" BIGINT
);"#,
            table.ddl()
        )
    }
}
