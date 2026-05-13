use crate as rltbl;
use rltbl::core::{id_ddl, RowID, ID_SQL_TYPE};
use rltbl_db::{any::AnyPool, core::DbQuery, db_value::JsonValue};

use anyhow::Result;
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

impl<'a> MessageTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "message".to_owned(),
            pool,
        }
    }

    pub fn column_names(&self) -> Vec<String> {
        vec![
            "message_id",
            "added_by",
            "table",
            "row",
            "column",
            "value",
            "level",
            "rule",
            "message",
        ]
        .into_iter()
        .map(|x| x.to_string())
        .collect()
    }

    /// Get the SQL DDL as a string.
    /// Requires the db only to know the SQL flavour to use.
    pub fn ddl(&self) -> String {
        format!(
            r#"CREATE TABLE "{table_name}" (
              "message_id" {id},
              "added_by" TEXT,
              "table" TEXT NOT NULL,
              "row" {ID_SQL_TYPE} NOT NULL,
              "column" TEXT NOT NULL,
              "value" TEXT,
              "level" TEXT,
              "rule" TEXT,
              "message" TEXT
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
