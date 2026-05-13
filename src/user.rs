use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::RowID,
    simple_table::SimpleTable,
};
use rltbl_db::{any::AnyPool, core::DbQuery, serde::to_db_row};

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::to_value;

// Minimal fields for a user.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub color: String,
}

// TODO: Handle ranges of columns and rows.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor {
    pub table: String,
    pub row: RowID,
    pub column: String,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            table: "table".to_string(),
            row: 1,
            column: "table".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UserCursor {
    pub name: String,
    pub color: String,
    pub cursor: Cursor,
    pub datetime: String,
}

impl UserCursor {
    pub fn new(username: &str) -> Self {
        let color = random_color::RandomColor::new().to_hex();
        Self {
            name: username.to_string(),
            color,
            ..Default::default()
        }
    }
}

impl Into<Account> for UserCursor {
    fn into(self) -> Account {
        Account {
            name: self.name.to_string(),
            color: self.color.to_string(),
        }
    }
}

/// Represents the special "user" table.
pub struct UserTable<'a> {
    // This table_name is "user" by default.
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for UserTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "name")
                .sql_type("TEXT")
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "color")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "cursor")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "datetime")
                .sql_type("TIMESTAMP")
                .sql_default("CURRENT_TIMESTAMP")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> UserTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "user".to_owned(),
            pool,
        }
    }

    /// Get a user by name, or create an entry for that name.
    pub async fn get_or_insert(&self, username: &str) -> UserCursor {
        // TODO: Handle datetime natively?
        let sql = r#"SELECT name, color, cursor, TEXT(datetime) AS datetime FROM "user" WHERE name = $1 LIMIT 1"#;
        let mut users: Vec<UserCursor> = self
            .pool
            .query(sql, &[username])
            .await
            .unwrap_or_default()
            .try_into_vec()
            .unwrap_or_default();
        if users.len() > 0 {
            return users.pop().unwrap();
        }
        let user = UserCursor::new(username);
        let row = to_db_row(&user).unwrap();
        self.pool
            .insert("user", &["name", "color", "cursor"], &[&row])
            .await
            .unwrap_or_default();
        user
    }

    /// Updates the cursor field in the user table for the user associated with the given
    /// changeset.
    pub async fn update_cursor(&self, username: &str, cursor: &Cursor) -> Result<()> {
        let sql = format!(
            r#"UPDATE "user"
               SET "cursor" = $1, "datetime" = CURRENT_TIMESTAMP
               WHERE "name" = $2"#,
        );
        self.pool
            .execute(
                &sql,
                &[&to_value(cursor).unwrap_or_default().to_string(), username],
            )
            .await?;
        Ok(())
    }

    pub async fn list(&self) -> Vec<UserCursor> {
        let sql = r#"SELECT * FROM "user""#;
        self.pool
            .query(sql, ())
            .await
            .unwrap_or_default()
            .try_into_vec()
            .unwrap_or_default()
    }

    pub async fn map(&self) -> IndexMap<String, UserCursor> {
        self.list()
            .await
            .into_iter()
            .map(|user| (user.name.to_string(), user))
            .collect()
    }

    // TODO: upsert
    // TODO: update_cursor(username, cursor)
    // TODO: recent(username) -- recent users excluding this user
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;
    use serde_json;

    #[tokio::test]
    async fn test_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = UserTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "user" (
  "name" TEXT PRIMARY KEY,
  "color" TEXT,
  "cursor" TEXT,
  "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);"#,
            table.ddl()
        )
    }

    // Test inner JSON string.
    #[tokio::test]
    async fn test_user() -> Result<()> {
        let user_cursor = UserCursor {
            name: "john".to_owned(),
            color: "#000000".to_owned(),
            cursor: Cursor {
                table: "foo".to_owned(),
                row: 1,
                column: "bar".to_owned(),
            },
            datetime: "2025-01-01T00:00:00".to_owned(),
        };
        let string = r##"{"name":"john","color":"#000000","cursor":{"table":"foo","row":1,"column":"bar"},"datetime":"2025-01-01T00:00:00"}"##;
        assert_eq!(serde_json::from_str::<UserCursor>(&string)?, user_cursor,);
        assert_eq!(serde_json::to_string(&user_cursor)?, string,);
        Ok(())
    }
}
