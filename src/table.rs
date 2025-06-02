//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use crate::{self as rltbl};

use anyhow::Result;
use indexmap::IndexMap;
use rltbl::{
    core::{Relatable, RelatableError, NEW_ORDER_MULTIPLIER},
    sql::{self, DbKind, DbTransaction, JsonRow, SqlParam},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    /// The name of the view (blank if there is none) associated with the table
    pub view: String,
    /// The id of the most recent change to this table.
    pub change_id: usize,
    // We may eventually want to turn `columns` into a special-purpose struct, but for now a
    // simple IndexMap suffices.
    /// The table's column configuration, implemented as a map from column names to [Column]s.
    pub columns: IndexMap<String, Column>,
    pub editable: bool,
    /// Indicates whether the table has the _id and _order meta columns enabled:
    pub has_meta: bool,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            name: "".into(),
            view: "".into(),
            change_id: 0,
            columns: IndexMap::new(),
            editable: true,
            has_meta: true,
        }
    }
}

impl Table {
    /// Use the given [relatable](crate) instance to ensure that the default view for this
    /// table has been created, and to set the [view name](Table::view) for this table to
    /// TABLENAME_default_view
    pub async fn ensure_default_view_created(&mut self, rltbl: &Relatable) -> Result<Vec<Column>> {
        tracing::trace!("Table::ensure_view_created({rltbl:?})");
        let (columns, meta_columns) = rltbl.fetch_all_columns(&self.name).await?;
        self.view = format!("{}_default_view", self.name);
        tracing::debug!(r#"Creating view "{}" with columns {columns:?}"#, self.view);
        let id_col = match meta_columns.iter().any(|c| c.name == "_id") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_id"#,
        };
        let order_col = match meta_columns.iter().any(|c| c.name == "_order") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_order"#,
        };

        for sql in sql::generate_view_ddl(
            &self.name,
            &self.view,
            id_col,
            order_col,
            &columns,
            &rltbl.connection.kind(),
        ) {
            rltbl.connection.query(&sql, None).await?;
        }
        Ok(columns)
    }

    /// Retrieve the given attribute of the given column from this table's
    /// [column configuration](Table::columns)
    pub fn get_column_attribute(&self, column: &str, attribute: &str) -> Option<String> {
        tracing::trace!("Table::get_column_attribute({column:?}, {attribute:?})");
        self.columns.get(column).and_then(|col| match attribute {
            "table" => Some(col.table.to_string()),
            "column" => Some(col.name.to_string()),
            "label" => match &col.label {
                None => None,
                Some(label) if label == "" => None,
                Some(_) => col.label.clone(),
            },
            "description" => match &col.description {
                None => None,
                Some(description) if description == "" => None,
                Some(_) => col.description.clone(),
            },
            "datatype" => match &col.datatype {
                None => None,
                Some(datatype) if datatype == "" => None,
                Some(_) => col.datatype.clone(),
            },
            "nulltype" => match &col.nulltype {
                None => None,
                Some(nulltype) if nulltype == "" => None,
                Some(_) => col.nulltype.clone(),
            },
            _ => None,
        })
    }
}

/// Represents a column from some table
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Column {
    pub name: String,
    pub table: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub datatype: Option<String>,
    pub nulltype: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
}

/// Represents a row from some table
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub id: usize,
    pub order: usize,
    pub change_id: usize,
    pub cells: IndexMap<String, Cell>,
}

impl Row {
    /// Prepares a new [Row] for insertion to the given [Table], with its [id](Row::id) and
    /// [order](Row::order) fields pre-assigned with their correct next values for this table
    pub fn prepare_new(
        table: &Table,
        json_row: Option<&JsonRow>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<Self> {
        tracing::trace!("Row::prepare_new({table:?}, {json_row:?}, tx)");
        let json_row = match json_row {
            None => {
                let columns = {
                    let columns = sql::get_db_table_columns(&table.name, tx)?;
                    if columns.is_empty() {
                        return Err(RelatableError::DataError(format!(
                            "No defined columns for: {table}",
                            table = table.name
                        ))
                        .into());
                    }
                    columns
                        .iter()
                        .map(|c| c.get_string("name").expect("No 'name' found"))
                        .filter(|n| !n.starts_with("_"))
                        .collect::<Vec<_>>()
                };
                let columns = columns.iter().map(|c| c.as_str()).collect::<Vec<_>>();
                JsonRow::from_strings(&columns)
            }
            Some(json_row) => json_row.clone(),
        };
        let mut row = Row::from(json_row);
        row.id = sql::get_next_id(table.name.as_str(), tx)?;
        row.order = NEW_ORDER_MULTIPLIER * row.id;
        row.change_id = table.change_id;
        tracing::debug!("Prepared a new row: {row:?}");
        Ok(row)
    }

    /// Convert the [text](Cell::text) values of all of the row's [cells](Row::cells) to
    /// strings and return them to the caller as a vector
    pub fn to_strings(&self) -> Vec<String> {
        tracing::trace!("Row::to_strings()");
        self.cells.values().map(|cell| cell.text.clone()).collect()
    }

    /// Generate an insert statement and a [JsonValue] representing an [Array](JsonValue::Array) of
    /// parameters that need to be bound to the statement before it is executed.
    pub fn as_insert(&self, table: &str, db_kind: &DbKind) -> (String, JsonValue) {
        tracing::trace!("Row::as_insert({table:?})");
        let id = self.id;
        let order = self.order;
        let quoted_column_names = self
            .cells
            .keys()
            .map(|k| format!(r#""{k}""#))
            .collect::<Vec<_>>();

        let mut sql_param_gen = SqlParam::new(db_kind);
        let (value_placeholders, params) = {
            let mut params = vec![json!(id), json!(order)];
            let mut value_placeholders = vec![sql_param_gen.next(), sql_param_gen.next()];
            for cell in self.cells.values() {
                if cell.value == JsonValue::Null {
                    value_placeholders.push("NULL".to_string());
                } else {
                    value_placeholders.push(sql_param_gen.next());
                    params.push(cell.value.clone());
                }
            }
            (value_placeholders, params)
        };

        let sql = if quoted_column_names.len() == 0 {
            format!(
                r#"INSERT INTO "{table}"
                   ("_id", "_order")
                   VALUES ({column_values})"#,
                column_values = value_placeholders.join(", ")
            )
        } else {
            format!(
                r#"INSERT INTO "{table}"
                   ("_id", "_order", {quoted_column_names})
                   VALUES ({column_values})"#,
                quoted_column_names = quoted_column_names.join(", "),
                column_values = value_placeholders.join(", "),
            )
        };
        (sql, json!(params))
    }
}

impl From<Row> for Vec<String> {
    /// Wrapper around [Row::to_strings()]
    fn from(row: Row) -> Self {
        tracing::trace!("Row::from({row:?})");
        row.to_strings()
    }
}

impl From<JsonRow> for Row {
    fn from(row: JsonRow) -> Self {
        tracing::trace!("Row::from({row:?})");
        let id = row
            .content
            .get("_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let order = row
            .content
            .get("_order")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let change_id = row
            .content
            .get("_change_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let cells = row
            .content
            .iter()
            // Ignore columns that start with "_"
            .filter(|(k, _)| !k.starts_with("_"))
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();

        Self {
            id,
            order,
            change_id,
            cells,
        }
    }
}

/// Represents a cell from a row in a given table
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub value: JsonValue,
    pub text: String,
    pub messages: Vec<Message>,
}

impl From<&JsonValue> for Cell {
    /// Converts a [JsonValue] to a [Cell]
    fn from(value: &JsonValue) -> Self {
        tracing::trace!("Cell::from({value:?})");
        Self {
            value: value.clone(),
            text: match value {
                JsonValue::String(value) => value.to_string(),
                value => format!("{value}"),
            },
            messages: vec![],
        }
    }
}

/// Represents a validation message
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    /// The severity of the message.
    pub level: String,
    /// The rule violation that the message is about.
    pub rule: String,
    /// The contents of the message.
    pub message: String,
}
