//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[row](crate::row)).

use crate as rltbl;
use rltbl::{
    column::Column,
    core::{Relatable, RelatableError, RowID, RowOrder},
    datatype::Datatypes,
    schema::Schema,
};
use rltbl_db::core::JsonRow;

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Represents a row from some table
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: RowID,
    pub order: RowOrder,
    pub change_id: RowID,
    pub cells: IndexMap<String, Cell>,
}

impl Row {
    /// Convert the [text](Cell::text) values of all of the row's [cells](Row::cells) to
    /// strings and return them to the caller as a vector
    pub fn to_strings(&self) -> Vec<String> {
        tracing::trace!("Row::to_strings()");
        self.cells.values().map(|cell| cell.text.clone()).collect()
    }

    /// Validate this row, which belongs to the given [Table],
    /// and add any resulting validation [messages](Message) to the message table
    pub async fn validate_sql_types(
        &mut self,
        schema: &Schema,
        table_name: &str,
        rltbl: &Relatable,
    ) -> Result<&Self> {
        for (column_name, cell) in self.cells.iter_mut() {
            let column = schema
                .column(table_name, column_name.as_str())
                .cloned()
                .unwrap_or_default();
            cell.validate_sql_type(&schema.datatypes, &column)?;
            for message in cell.messages.iter() {
                let (msg_id, msg) = rltbl
                    .add_message(
                        "rltbl",
                        table_name,
                        self.id,
                        column_name,
                        &cell.value,
                        &message.level,
                        &message.rule,
                        &message.message,
                    )
                    .await?;
                tracing::debug!("Added message (ID {msg_id}): {msg:?}");
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
        let id = row.get("_id").and_then(|i| i.as_u64()).unwrap_or_default() as RowID;
        let order = row
            .get("_order")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as RowOrder;
        let change_id = row
            .get("_change_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as RowID;
        let mut cells: IndexMap<String, Cell> = row
            .iter()
            // Ignore columns that start with "_"
            .filter(|(k, _)| !k.starts_with("_"))
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();
        let messages = row.get("_message");
        if let Some(m) = messages {
            let mut messages = m.clone();
            // WARN: Converting _message string to JSON.
            match m {
                JsonValue::String(m) => messages = serde_json::from_str(&m).unwrap(),
                _ => (),
            }
            if let Some(messages) = messages.as_array() {
                for message in messages.iter() {
                    let column = message
                        .as_object()
                        .unwrap()
                        .get("column")
                        .unwrap()
                        .as_str()
                        .unwrap();
                    let message: Message = match serde_json::from_value(message.to_owned()) {
                        Ok(message) => message,
                        Err(err) => {
                            tracing::warn!(
                                "Unable to parse message '{message}' due to error '{err}'"
                            );
                            continue;
                        }
                    };
                    if let Some(cell) = cells.get(column) {
                        let mut new_cell = cell.clone();
                        new_cell.value = message.value.clone();
                        new_cell.text = rltbl_db::core::json_value_to_string(&new_cell.value);
                        new_cell.messages.push(message);
                        cells.insert(column.to_string(), new_cell);
                    }
                }
            }
        }

        Self {
            id,
            order,
            change_id,
            cells,
        }
    }
}

/// Represents a cell from a row in a given table
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
                JsonValue::Null => String::new(),
                value => format!("{value}"),
            },
            messages: vec![],
        }
    }
}

impl Cell {
    /// Validate this cell, which belongs to the given [Column], adding any validation
    /// [messages](Message) to the cell's [messages](Cell::messages) field.
    pub fn validate_sql_type(&mut self, datatypes: &Datatypes, column: &Column) -> Result<&Self> {
        let sql_type = column.sql_type(datatypes).to_lowercase();

        fn invalidate(cell: &mut Cell, sql_type: &str, column: &Column) {
            cell.messages.push(Message {
                value: cell.value.clone(),
                level: "error".to_string(),
                rule: format!("sql_type:{sql_type}"),
                message: format!(
                    "{column} must be of type {sql_type}",
                    column = column.column
                ),
            });
        }

        match sql_type.as_str() {
            "integer" | "bigint" => match &mut self.value {
                JsonValue::Number(number) => match number.to_string().parse::<i64>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, &sql_type, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, &sql_type, column),
            },
            "real" | "numeric" => match &mut self.value {
                JsonValue::Number(number) => match number.to_string().parse::<f64>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, &sql_type, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, &sql_type, column),
            },
            "text" => (),
            unsupported => {
                return Err(RelatableError::InputError(format!(
                    "Unsupported SQL type: '{unsupported}'"
                ))
                .into())
            }
        };

        Ok(self)
    }

    /// Report the maximum [error level](Message::level) associated with this cell's
    /// [messages](Cell::messages), where 0 represents no error, 1 represents the presence of
    /// at least one warning message, and 2 represents the presence of at least one error message.
    pub fn message_level(&self) -> usize {
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

    /// Determine whether this cell contains a SQL type error.
    pub fn has_sql_type_error(&self) -> bool {
        self.messages
            .iter()
            .filter(|m| m.level == "error" && m.rule.starts_with("sql_type:"))
            .collect::<Vec<_>>()
            .len()
            > 0
    }
}

/// Represents a validation message
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The value referred to by the message
    pub value: JsonValue,
    /// The severity of the message.
    pub level: String,
    /// The rule violation that the message is about.
    pub rule: String,
    /// The contents of the message.
    pub message: String,
}

// Tests

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_json_to_row() -> Result<()> {
        let json_blob = json!({
            "_id": 1,
            "_order": 1000,
            "_change_id": 0,
            "foo": "FOO",
        });
        let json_row: JsonRow = serde_json::from_value(json_blob)?;
        let row: Row = json_row.into();
        let mut cells = IndexMap::new();
        cells.insert(
            "foo".to_string(),
            Cell {
                value: json!("FOO"),
                text: "FOO".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            row,
            Row {
                id: 1,
                order: 1000,
                change_id: 0,
                cells
            }
        );
        Ok(())
    }

    #[test]
    fn test_json_to_row_messages() {
        let json_blob = json!({
            "_id": 1,
            "_order": 1000,
            "_change_id": 0,
            "_message": [{
                "column": "foo",
                "value": "FOO",
                "level": "error",
                "rule": "test rule",
                "message": "Test message 'FOO'"
            }],
            "foo": "FOO",
        });
        let json_row: JsonRow = serde_json::from_value(json_blob).unwrap();
        let row: Row = json_row.into();
        let mut cells = IndexMap::new();
        cells.insert(
            "foo".to_string(),
            Cell {
                value: json!("FOO"),
                text: "FOO".to_string(),
                messages: vec![Message {
                    value: json!("FOO"),
                    level: "error".to_string(),
                    rule: "test rule".to_string(),
                    message: "Test message 'FOO'".to_string(),
                }],
            },
        );
        assert_eq!(
            row,
            Row {
                id: 1,
                order: 1000,
                change_id: 0,
                cells
            }
        )
    }
}
