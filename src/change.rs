use std::{collections::HashMap, fmt::Display, str::FromStr};

use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{ColumnID, TableID},
    core::{RelatableError, RowID},
    simple_table::SimpleTable,
    table::Table,
    tsv_table::InnerRows,
};
use rltbl_db::AnyPool;

use anyhow::Result;
use serde::{Deserialize, Serialize};

// Changes and History

/// The kind of action that is performed by a change
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeAction {
    Do,
    Undo,
    Redo,
}

impl FromStr for ChangeAction {
    type Err = anyhow::Error;

    fn from_str(action: &str) -> Result<Self> {
        tracing::trace!("ChangeAction::from_str({action:?})");
        match action.to_lowercase().as_str() {
            "do" => Ok(ChangeAction::Do),
            "undo" => Ok(ChangeAction::Undo),
            "redo" => Ok(ChangeAction::Redo),
            _ => {
                return Err(
                    RelatableError::InputError(format!("Unrecognized action: {action}")).into(),
                );
            }
        }
    }
}

impl Display for ChangeAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeAction::Do => write!(f, "do"),
            ChangeAction::Undo => write!(f, "undo"),
            ChangeAction::Redo => write!(f, "redo"),
        }
    }
}

impl Serialize for ChangeAction {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ChangeAction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// A change to a table in the database
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Change {
    InsertRows {
        table_id: TableID,
        rows: InnerRows,
        after_id: RowID,
    },
    DeleteRows {
        table_id: TableID,
        row_ids: Vec<RowID>,
    },
    DeleteTables {
        table_ids: Vec<TableID>,
        column_ids: Vec<ColumnID>,
    },
    MoveRows {
        table_id: i32,
        row_ids: Vec<RowID>,
        after_id: RowID,
    },
    UpdateRows {
        table_id: TableID,
        rows: InnerRows,
    },
}

impl Change {
    pub fn render(&self, tables: &HashMap<TableID, Table>) -> String {
        let no_change = "No change".to_string();
        match self {
            Change::InsertRows { table_id, rows, .. } => match tables.get(table_id) {
                Some(table) => format!(
                    r#"Insert {} row{} into "{}" table"#,
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" },
                    table.name
                ),
                None => no_change,
            },
            Change::DeleteRows { table_id, row_ids } => match tables.get(table_id) {
                Some(table) => format!(
                    r#"Delete {} row{} from "{}" table"#,
                    row_ids.len(),
                    if row_ids.len() == 1 { "" } else { "s" },
                    table.name
                ),
                None => no_change,
            },
            Change::DeleteTables { table_ids, .. } => match table_ids.first() {
                Some(table_id) => match tables.get(table_id) {
                    Some(table) => format!(r#"Delete "{}" table"#, table.name),
                    None => no_change,
                },
                None => no_change,
            },
            Change::MoveRows {
                table_id, row_ids, ..
            } => match tables.get(table_id) {
                Some(table) => format!(
                    r#"Move {} row{} in "{}" table"#,
                    row_ids.len(),
                    if row_ids.len() == 1 { "" } else { "s" },
                    table.name
                ),
                None => no_change,
            },
            Change::UpdateRows { table_id, rows } => match tables.get(table_id) {
                Some(table) => format!(
                    r#"Update {} row{} in "{}" table"#,
                    rows.len(),
                    if rows.len() == 1 { "" } else { "s" },
                    table.name
                ),
                None => no_change,
            },
        }
    }
}

// fn serialize_inner_rows<S>(x: &TsvRows, s: S) -> Result<S::Ok, S::Error>
// where
//     S: serde::Serializer,
// {
//     let inner = x.inner();
//     let mut ser = s.serialize_seq(Some(inner.len()))?;
//     ser.serialize_entry("$oid", &x.inner())?;
//     ser.end()
// }

/// Represents the special "change" table.
pub struct ChangeTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for ChangeTable<'a> {
    fn table_name(&self) -> &str {
        &self.table_name
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        vec![
            ColumnBuilder::new(self.table_name(), "change_id")
                .sql_type("SERIAL")
                .primary_key(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "datetime")
                .sql_type("TIMESTAMP")
                .sql_default("CURRENT_TIMESTAMP")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "user")
                .sql_type("TEXT")
                .not_null(true)
                .references(vec!["user".to_string(), "name".to_string()])
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "action")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "table")
                .sql_type("TEXT")
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "description")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "content")
                .sql_type("JSON")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> ChangeTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "change".to_owned(),
            pool,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::AnyPool;

    #[tokio::test]
    async fn test_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = ChangeTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "change" (
  "change_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  "user" TEXT NOT NULL REFERENCES "user"("name"),
  "action" TEXT NOT NULL,
  "table" TEXT NOT NULL,
  "description" TEXT,
  "content" JSON
);"#,
            table.ddl()
        )
    }
}
