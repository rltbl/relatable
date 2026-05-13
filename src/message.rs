use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{RowID, ID_SQL_TYPE},
    simple_table::SimpleTable,
};
use rltbl_db::{any::AnyPool, db_value::JsonValue};

use serde::{Deserialize, Serialize};

/// Represents a validation message for a cell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CellMessage {
    /// The value referred to by the message
    pub value: JsonValue,
    /// The severity of the message.
    pub level: String,
    /// The rule violation that the message is about.
    pub rule: String,
    /// The contents of the message.
    pub message: String,
}

/// Represents a validation message in the message table.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub message_id: RowID,
    pub added_by: String,
    pub table: String,
    pub row: RowID,
    pub column: String,
    /// The value referred to by the message
    pub value: JsonValue,
    /// The severity of the message.
    pub level: String,
    /// The rule violation that the message is about.
    pub rule: String,
    /// The contents of the message.
    pub message: String,
}

/// Represents the special "message" table.
pub struct MessageTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for MessageTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "message_id")
                .sql_type("SERIAL")
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "added_by")
                .sql_type("TEXT")
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
            ColumnBuilder::new(self.table_name(), "column")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "value")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "level")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "rule")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "message")
                .sql_type("TEXT")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> MessageTable<'a> {
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "message".to_owned(),
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
        let table = MessageTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "message" (
  "message_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "added_by" TEXT,
  "table" TEXT NOT NULL,
  "row" INTEGER NOT NULL,
  "column" TEXT NOT NULL,
  "value" TEXT,
  "level" TEXT,
  "rule" TEXT,
  "message" TEXT
);"#,
            table.ddl()
        )
    }
}
