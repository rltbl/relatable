use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::ID_SQL_TYPE,
    simple_table::SimpleTable,
};
use rltbl_db::any::AnyPool;

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
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "after")
                .sql_type("TEXT")
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
  "before" TEXT,
  "after" TEXT
);"#,
            table.ddl()
        )
    }
}
