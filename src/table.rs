//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use crate as rltbl;

use rltbl::{
    column::Column,
    core::{Relatable, RowID},
    sql::{self},
};
use rltbl_db::{core::DbQuery, db_kind::DbKind};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Table {
    /// The name of the table
    pub name: String,
    /// The optional path for loading and saving the table
    pub path: String,
    /// The name of the view (blank if there is none) to be used when querying the table
    pub view: String,
    /// The id of the most recent change to this table.
    pub editable: bool,
    /// Indicates whether the table has the _id and _order meta columns enabled:
    pub has_meta: bool,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            name: Default::default(),
            path: Default::default(),
            view: Default::default(),
            editable: true,
            has_meta: true,
        }
    }
}

impl Table {
    /// Drop the given table in the database
    pub async fn drop_table(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::drop_data_tables({self:?}, {rltbl:?})");
        let sql = match rltbl.pool.kind() {
            DbKind::SQLite => {
                format!(r#"DROP TABLE IF EXISTS "{}""#, self.name)
            }
            DbKind::PostgreSQL => {
                format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, self.name)
            }
        };
        tracing::info!("Dropped table '{}'", self.name);
        rltbl.pool.execute(&sql, ()).await?;
        Ok(())
    }

    /// Query the database through the given [Relatable] instance to determine whether the given
    /// table exists.
    pub async fn table_exists(table_name: &str, rltbl: &Relatable) -> Result<bool> {
        let (sql, params) = match rltbl.pool.kind() {
            DbKind::SQLite => (
                r#"SELECT 1 FROM "sqlite_master"
                       WHERE "type" = $1 AND name = $2 LIMIT 1"#,
                ["table", table_name],
            ),
            DbKind::PostgreSQL => (
                r#"SELECT 1 FROM "information_schema"."tables"
                       WHERE "table_type" LIKE $1
                         AND "table_name" = $2
                         AND "table_schema" IN (
                           SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                           FROM "pg_settings"
                           WHERE "name" = 'search_path'
                         )"#,
                ["%TABLE", table_name],
            ),
        };
        let rows = rltbl.pool.query(&sql, params).await?;
        if rows.len() == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
    }

    /// Determine whether a view of the given type exists for the table in the database.
    pub async fn view_exists(table: &str, view_type: &str, rltbl: &Relatable) -> Result<bool> {
        let (statement, params) = match rltbl.pool.kind() {
            DbKind::SQLite => (
                format!(
                    r#"SELECT 1
                           FROM sqlite_master
                           WHERE type = 'view' AND name = $1"#
                ),
                vec![format!("{table}_{view_type}_view")],
            ),
            DbKind::PostgreSQL => (
                format!(
                    r#"SELECT 1
                           FROM "information_schema"."tables"
                           WHERE "table_name" = $1
                           AND "table_type" = $2
                           AND "table_schema" IN (
                               SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                               FROM "pg_settings"
                               WHERE "name" = 'search_path'
                           )"#,
                ),
                vec![format!("{table}_{view_type}_view"), "VIEW".to_owned()],
            ),
        };
        match rltbl.pool.query(&statement, params).await?.len() {
            1 => Ok(true),
            _ => Ok(false),
        }
    }

    /// Set the view for the table to the given view type (accepted types are "default" and "text"),
    /// after first ensuring that a view of the given type exists, creating it if necessary.
    pub async fn set_view(&mut self, rltbl: &Relatable, view_type: &str) -> Result<&Self> {
        match view_type {
            "text" => self.ensure_text_view_created(rltbl).await?,
            "default" => self.ensure_default_view_created(rltbl).await?,
            unsupported => {
                tracing::warn!(
                    "Unsupported view name: '{}'. Not changing view '{}' for table '{}",
                    unsupported,
                    self.view,
                    self.name
                );
            }
        };
        Ok(self)
    }

    fn get_id_order_columns(&self, meta_columns: &Vec<Column>) -> (&str, &str) {
        match self.name.as_str() {
            "message" => ("message_id", "message_id"),
            "change" => ("change_id", "change_id"),
            "history" => ("history_id", "history_id"),
            _ => {
                let id_col = match meta_columns.iter().any(|c| c.column == "_id") {
                    false => r#"rowid"#, // This *must* be lowercase.
                    true => r#"_id"#,
                };
                let order_col = match meta_columns.iter().any(|c| c.column == "_order") {
                    false => r#"rowid"#, // This *must* be lowercase.
                    true => r#"_order"#,
                };
                (id_col, order_col)
            }
        }
    }

    /// Use the given [relatable](crate) instance to ensure that the default view for this
    /// table has been created, and then set the view for this table to it.
    pub async fn ensure_default_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        let columns = rltbl.column_table().get(&[&self.name]).await?;
        let view_name = format!("{}_default_view", self.name);

        let (id_col, order_col) = self.get_id_order_columns(&columns);

        for sql in sql::generate_default_view_ddl(
            &self.name,
            id_col,
            order_col,
            &columns.data(),
            &rltbl.pool.kind(),
        ) {
            rltbl.pool.execute(&sql, ()).await?;
        }

        // Set the table's view name to the default view:
        self.view = view_name;

        Ok(())
    }

    /// Use the given [relatable](crate) instance to ensure that the text view for this
    /// table has been created, and then set the view for this table to it.
    pub async fn ensure_text_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::ensure_text_view_created({self:?}, {rltbl:?})");

        // The default view needs to be created first:
        self.ensure_default_view_created(rltbl).await?;

        // Create the text view:
        let view_name = format!("{}_text_view", self.name);

        let columns = rltbl.column_table().get(&[&self.name]).await?;
        let (id_col, order_col) = self.get_id_order_columns(&columns);

        let datatypes = rltbl.datatypes().await;
        for sql in sql::generate_text_view_ddl(
            &datatypes,
            &self.name,
            id_col,
            order_col,
            &columns.data(),
            &rltbl.pool.kind(),
        ) {
            rltbl.pool.execute(&sql, ()).await?;
        }

        // Set the table's view name to the text view:
        self.view = view_name;

        Ok(())
    }

    /// Returns the row id that comes before the given row in the given table, using the given
    /// transaction.
    pub async fn get_previous_row_id(
        table: &str,
        row_id: RowID,
        rltbl: &Relatable,
    ) -> Result<RowID> {
        let sql = format!(
            r#"SELECT "_id" FROM "{table}" WHERE "_order" < (SELECT _order FROM "{table}" WHERE _id = $1)
               ORDER BY "_order" DESC LIMIT 1"#,
        );
        match rltbl.pool.query(&sql, [row_id]).await?.value() {
            Ok(value) => Ok(value.as_i32().unwrap_or_default()),
            Err(_) => Ok(0),
        }
    }
}
