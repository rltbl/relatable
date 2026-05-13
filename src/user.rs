use crate as rltbl;
use indexmap::IndexMap;
use rltbl::core::{RelatableError, RowID};
use rltbl_db::{any::AnyPool, core::DbQuery, serde::to_db_row};

use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::to_value;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub color: String,
}

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

impl<'a> UserTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "datatype".to_owned(),
            pool,
        }
    }

    // TODO: use rltbl_db to sanitize the name
    /// Use this name for the datatype table.
    /// The default is "datatype".
    pub fn name(mut self, table_name: &str) -> Result<Self> {
        let pattern = Regex::new(r"^\w+$").unwrap();
        if !pattern.is_match(table_name) {
            return Err(
                RelatableError::DataError(format!("Not a valid table name: {table_name}")).into(),
            );
        }
        self.table_name = table_name.to_owned();
        Ok(self)
    }

    pub fn column_names(&self) -> Vec<String> {
        vec!["name", "color", "cursor", "datetime"]
            .into_iter()
            .map(|x| x.to_string())
            .collect()
    }

    /// Get the SQL DDL as a string.
    /// Requires the db only to know the SQL flavour to use.
    pub fn ddl(&self) -> String {
        format!(
            r#"CREATE TABLE "{table_name}" (
              "name" TEXT PRIMARY KEY,
              "color" TEXT,
              "cursor" TEXT,
              "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )"#,
            table_name = self.table_name,
        )
    }

    // TODO: replace this with self.pool.drop(self.name).
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
    use serde_json;

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
