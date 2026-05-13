use std::{fmt::Display, str::FromStr};

use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{RelatableError, RowID},
    simple_table::SimpleTable,
    user::Cursor,
};
use rltbl_db::{any::AnyPool, db_value::JsonRow};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// Changes and History

/// A set of changes made by a user to a table.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeSet {
    pub action: ChangeAction,
    pub table: String,
    pub user: String,
    pub description: String,
    pub changes: Vec<Change>,
}

impl ChangeSet {
    /// Given a change, returns the a [Cursor] representing where the user's cursor
    /// should be placed in the frontend.
    pub fn to_cursor(&self) -> Result<Cursor> {
        tracing::trace!("ChangeSet::to_cursor()");
        let table = self.table.clone();
        match self.changes.first() {
            Some(change) => match change {
                Change::Update {
                    row,
                    column,
                    before: _,
                    after: _,
                } => Ok(Cursor {
                    table,
                    row: *row,
                    column: column.to_string(),
                }),
                Change::Add { row, after: _ } => Ok(Cursor {
                    table,
                    row: *row,
                    column: "".to_string(),
                }),
                Change::Move {
                    row,
                    from_after: _,
                    to_after: _,
                } => Ok(Cursor {
                    table,
                    row: *row,
                    column: "".to_string(),
                }),
                Change::Delete { row, after: _ } => Ok(Cursor {
                    table,
                    row: *row,
                    column: "".to_string(),
                }),
            },
            None => Err(RelatableError::ChangeError("No changes in set".into()).into()),
        }
    }
}

/// The kind of action that is performed by a change
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
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

/// A change to a table in the database
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Change {
    Update {
        /// The id of the row that was updated
        row: RowID,
        /// The column whose value was updated
        column: String,
        /// The value of the column before the change
        before: JsonValue,
        /// The value of the column after the change
        after: JsonValue,
    },
    Add {
        /// The id of the row that was added
        row: RowID,
        /// The _id of the row whose _order this comes immediately after in the table
        after: RowID,
    },
    Move {
        /// The id of the row that was moved
        row: RowID,
        /// The row that this row came after before the change
        from_after: RowID,
        /// The row that this row came after after the change
        to_after: RowID,
    },
    Delete {
        /// The id of the row that was deleted
        row: RowID,
        /// The _id of the row whose _order this row came immediately after in the table before
        /// being deleted.
        after: RowID,
    },
}

/// Describes a history of changes that have been done and undone.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct History {
    pub changes_done_stack: Vec<JsonRow>,
    pub changes_undone_stack: Vec<JsonRow>,
}

impl Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Change::Update {
                row,
                column,
                before,
                after,
            } => {
                write!(
                    f,
                    "Update '{column}' in row {row} from {before} to {after}",
                    before = rltbl::sql::json_to_string(before),
                    after = rltbl::sql::json_to_string(after)
                )
            }
            Change::Add { row, after } => {
                write!(f, "Add row {row} after row {after}")
            }
            Change::Move {
                row,
                from_after,
                to_after,
            } => {
                write!(
                    f,
                    "Move row {row} from after row {from_after} to after row {to_after}"
                )
            }
            Change::Delete { row, after: _ } => write!(f, "Delete row {row}"),
        }
    }
}

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
                .sql_type("TEXT")
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
    use rltbl_db::any::AnyPool;

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
  "content" TEXT
);"#,
            table.ddl()
        )
    }
}
