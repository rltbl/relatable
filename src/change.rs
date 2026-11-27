use std::{collections::HashMap, fmt::Display, num::ParseIntError, str::FromStr};

use crate::{self as rltbl, core::ID_SQL_TYPE, history::History, user::Cursor};
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{ColumnID, TableID},
    core::{RelatableError, RowID},
    simple_table::SimpleTable,
    table::Table,
    tsv_table::InnerRows,
};
use rltbl_db::{row, AnyPool, Row as DbRow, Value as DbValue};

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

impl Into<Cursor> for Change {
    fn into(self) -> Cursor {
        match self {
            Change::InsertRows { table_id, rows, .. } => Cursor {
                table: format!("t{}", table_id),
                row: rows
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .get("_id")
                    .and_then(|v| v.as_i32())
                    .unwrap_or_default(),
                ..Default::default()
            },
            Change::DeleteRows { table_id, row_ids } => Cursor {
                table: format!("t{}", table_id),
                row: row_ids.first().cloned().unwrap_or_default(),
                ..Default::default()
            },
            Change::DeleteTables { table_ids, .. } => Cursor {
                table: "t1".to_string(), // Always the table table?
                row: table_ids.first().cloned().unwrap_or_default(),
                ..Default::default()
            },
            Change::MoveRows {
                table_id, row_ids, ..
            } => Cursor {
                table: format!("t{}", table_id),
                row: row_ids.first().cloned().unwrap_or_default(),
                ..Default::default()
            },
            Change::UpdateRows { table_id, rows } => Cursor {
                table: format!("t{}", table_id),
                row: rows
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .get("_id")
                    .and_then(|v| v.as_i32())
                    .unwrap_or_default(),
                column: rows
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .into_keys()
                    .filter(|k| k.starts_with("c"))
                    .nth(0)
                    .unwrap_or_default(),
            },
        }
    }
}

/// Represents the special "change" table.
pub struct RowChangeTable<'a> {
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> SimpleTable for RowChangeTable<'a> {
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
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "table_id")
                .sql_type(ID_SQL_TYPE)
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "row_id")
                .sql_type(ID_SQL_TYPE)
                .not_null(true)
                .build()
                .unwrap(),
            ColumnBuilder::new(self.table_name(), "column_id")
                .sql_type(ID_SQL_TYPE)
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> RowChangeTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "change".to_owned(),
            pool,
        }
    }

    pub async fn record(&self, history: &History) -> Result<()> {
        let history_id = history.history_id;
        let mut records: Vec<DbRow> = Vec::new();
        for change in &history.changes {
            match change {
                Change::InsertRows { table_id, rows, .. } => {
                    for row in rows {
                        records.push(row! {
                            "history_id" => history_id,
                            "table_id" => *table_id,
                            "row_id" => row.get("_id").and_then(|v| v.as_i32()).unwrap_or_default(),
                            "column_id" => DbValue::Null,
                        });
                    }
                }
                Change::DeleteRows { table_id, row_ids } => {
                    for row_id in row_ids {
                        records.push(row! {
                            "history_id" => history_id,
                            "table_id" => *table_id,
                            "row_id" => *row_id,
                            "column_id" => DbValue::Null,
                        });
                    }
                }
                Change::DeleteTables { table_ids, .. } => {
                    for table_id in table_ids {
                        records.push(row! {
                            "history_id" => history_id,
                            "table_id" => 1,
                            "row_id" => *table_id,
                            "column_id" => DbValue::Null,
                        });
                    }
                }
                Change::MoveRows {
                    table_id, row_ids, ..
                } => {
                    for row_id in row_ids {
                        records.push(row! {
                            "history_id" => history_id,
                            "table_id" => *table_id,
                            "row_id" => *row_id,
                            "column_id" => DbValue::Null,
                        });
                    }
                }
                Change::UpdateRows { table_id, rows } => {
                    for row in rows {
                        let row_id = row.get("_id").and_then(|v| v.as_i32()).unwrap_or_default();
                        for column_name in row.keys() {
                            let column_id: Result<ColumnID, ParseIntError> =
                                column_name.replace("c", "").parse();
                            if let Ok(column_id) = column_id {
                                records.push(row! {
                                    "history_id" => history_id,
                                    "table_id" => *table_id,
                                    "row_id" => row_id,
                                    "column_id" => column_id,
                                });
                            }
                        }
                    }
                }
            }
        }
        self.pool
            .insert(
                self.table_name(),
                &["history_id", "table_id", "row_id", "column_id"],
                &records,
            )
            .await?;
        Ok(())
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
        let table = RowChangeTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "change" (
  "history_id" INTEGER NOT NULL,
  "table_id" INTEGER NOT NULL,
  "row_id" INTEGER NOT NULL,
  "column_id" INTEGER
);"#,
            table.ddl()
        )
    }
}
