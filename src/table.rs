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
        tracing::trace!("Table::ensure_default_view_created({rltbl:?})");
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

        for sql in sql::generate_default_view_ddl(
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

    /// TODO: Add docstring
    pub async fn ensure_text_view_created(&mut self, rltbl: &Relatable) -> Result<Vec<Column>> {
        tracing::trace!("Table::ensure_text_view_created({rltbl:?})");
        self.ensure_default_view_created(rltbl).await?;
        let (columns, meta_columns) = rltbl.fetch_all_columns(&self.name).await?;
        self.view = format!("{}_text_view", self.name);
        tracing::debug!(r#"Creating view "{}" with columns {columns:?}"#, self.view);
        let id_col = match meta_columns.iter().any(|c| c.name == "_id") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_id"#,
        };
        let order_col = match meta_columns.iter().any(|c| c.name == "_order") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_order"#,
        };

        for sql in sql::generate_text_view_ddl(
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

    /// TODO: Add docstring
    pub fn get_column(&self, column: &str) -> Column {
        match self.columns.get(column) {
            Some(column) => column.clone(),
            None => {
                tracing::debug!("TODO: Do or say something here");
                Column::default()
            }
        }
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

    /// TODO: Add docstring
    pub fn validate(&mut self, table: &Table, tx: &mut DbTransaction<'_>) -> Result<&Self> {
        for (column, cell) in self.cells.iter_mut() {
            let column_details = table.get_column(column);
            let datatype = match column_details.datatype {
                None => "text".to_string(),
                Some(ref dt) if dt == "" => "text".to_string(),
                Some(ref dt) => dt.to_string(),
            };
            cell.validate(&column_details)?;
            if cell.error_level() >= 2 {
                let mut sql_param_gen = SqlParam::new(&tx.kind());
                let sql = format!(
                    r#"INSERT INTO "message"
                       ("added_by", "table", "row", "column", "value", "level", "rule", "message")
                       VALUES ({p1}, {p2}, {p3}, {p4}, {p5}, {p6}, {p7}, {p8})"#,
                    p1 = sql_param_gen.next(),
                    p2 = sql_param_gen.next(),
                    p3 = sql_param_gen.next(),
                    p4 = sql_param_gen.next(),
                    p5 = sql_param_gen.next(),
                    p6 = sql_param_gen.next(),
                    p7 = sql_param_gen.next(),
                    p8 = sql_param_gen.next(),
                );
                let params = json!([
                    "Valve",
                    table.name,
                    self.id,
                    column,
                    cell.value,
                    "error",
                    format!("datatype:{datatype}"),
                    "incorrect datatype"
                ]);
                tx.query(&sql, Some(&params))?;
            }
        }

        Ok(self)
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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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

impl Cell {
    /// TODO: Add docstring
    pub fn validate(&mut self, column: &Column) -> Result<&Self> {
        fn invalidate(cell: &mut Cell, column: &Column) {
            cell.messages.push(Message {
                level: "error".to_string(),
                rule: format!(
                    "datatype:{}",
                    match &column.datatype {
                        None => "text",
                        Some(datatype) => datatype,
                    }
                ),
                message: "incorrect datatype".to_string(),
            });
        }

        match sql::get_sql_type(&column.datatype)?.to_lowercase().as_str() {
            "integer" => match &mut self.value {
                // TODO: It seems inefficient to first convert to a string in order to determine
                // whether the number is an integer.
                JsonValue::Number(number) => match number.to_string().parse::<isize>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, column),
            },
            "text" | "" => (),
            unsupported => {
                return Err(RelatableError::InputError(format!(
                    "Unsupported datatype: '{unsupported}'"
                ))
                .into())
            }
        };

        Ok(self)
    }

    pub fn error_level(&self) -> usize {
        let mut level = 0;
        for message in &self.messages {
            match message.level.as_str() {
                "info" => (),
                "warn" => {
                    if level < 1 {
                        level = 1;
                    }
                }
                "error" => {
                    level = 2;
                    break;
                }
                unsupported => {
                    tracing::warn!("Unsupported message level '{unsupported}'");
                }
            };
        }
        level
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
