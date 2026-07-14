use crate::{self as rltbl};
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{RowID, ID_SQL_TYPE},
    simple_table::SimpleTable,
};
use rltbl_db::{
    any::AnyPool,
    core::DbQuery,
    db_row,
    db_value::{DbRow, DbValue},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct History {
    pub history_id: RowID,
    pub change_id: RowID,
    pub table: String,
    pub row: RowID,
    pub before: JsonValue,
    pub after: JsonValue,
}

impl Into<DbRow> for History {
    fn into(self) -> DbRow {
        db_row! {
            "history_id" => self.history_id,
            "change_id" => self.change_id,
            "table" => self.table,
            "row" => self.row,
            "before" => DbValue::Json(self.before),
            "after" => DbValue::Json(self.after),
        }
    }
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
            ColumnBuilder::new(self.table_name(), "change_id")
                .sql_type(ID_SQL_TYPE)
                .not_null(true)
                .references(vec!["change".to_string(), "change_id".to_string()])
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "table")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "row")
                .sql_type(ID_SQL_TYPE)
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "before")
                .sql_type("JSON")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "after")
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

    pub async fn histories(&self, change_id: RowID) -> Result<Vec<History>> {
        let sql = r#"SELECT * FROM history WHERE change_id = $1;"#;
        let rows: Vec<History> = self.pool().query(&sql, [change_id]).await?.try_into_vec()?;
        Ok(rows)
    }

    pub async fn append(&self, history: History) -> Result<History> {
        let mut row: DbRow = history.into();
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;

    #[tokio::test]
    async fn test_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = HistoryTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "history" (
  "history_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "change_id" INTEGER NOT NULL REFERENCES "change"("change_id"),
  "table" TEXT NOT NULL,
  "row" INTEGER NOT NULL,
  "before" JSON,
  "after" JSON
);"#,
            table.ddl()
        )
    }
}
