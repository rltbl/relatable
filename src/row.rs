//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[row](crate::row)).

use std::iter::zip;

use crate as rltbl;
use rltbl::{
    column::Column,
    core::{Relatable, RelatableError, RowID, RowOrder},
    datatype::Datatypes,
    message::CellMessage,
    schema::Schema,
};
use rltbl_db::{
    db_row,
    db_value::{DbRow, DbRows, DbValue, IntoDbRow, IntoDbRows, JsonRow},
};

use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Represents a row from some table
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: RowID,
    pub order: RowOrder,
    pub deleted: bool,
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

    // TODO: handle validation
    pub fn regular(&self) -> Option<DbRow> {
        let mut db_row = db_row! {
            "_id" => self.id,
            "_order" => self.order,
        };
        for (column, cell) in self.cells.clone() {
            db_row.insert(column, cell.value.into());
        }
        Some(db_row)
    }
}

impl From<Row> for Vec<String> {
    /// Wrapper around [Row::to_strings()]
    fn from(row: Row) -> Self {
        tracing::trace!("Row::from({row:?})");
        row.to_strings()
    }
}

impl From<DbRow> for Row {
    fn from(db_row: DbRow) -> Self {
        // TODO: This could be more efficient.
        let json_row: JsonRow = db_row
            .into_iter()
            .map(|(key, val)| (key, val.into()))
            .collect();
        json_row.into()
    }
}

impl From<&DbRow> for Row {
    fn from(db_row: &DbRow) -> Self {
        db_row.clone().into()
    }
}

impl From<JsonRow> for Row {
    fn from(row: JsonRow) -> Self {
        let id = row.get("_id").and_then(|i| i.as_u64()).unwrap_or_default() as RowID;
        let order = row
            .get("_order")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as RowOrder;
        let deleted = row
            .get("_deleted")
            .and_then(|b| match b {
                // TODO: Fix this hack!
                JsonValue::Null => Some(false),
                JsonValue::Bool(bool) => Some(*bool),
                JsonValue::Number(number) => {
                    if number.as_i64() == Some(0) || number.as_f64() == Some(0.0) {
                        Some(false)
                    } else {
                        Some(true)
                    }
                }
                _ => b.as_bool(),
            })
            .unwrap_or_default();
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
                    let message: CellMessage = match serde_json::from_value(message.to_owned()) {
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
                        // TODO: Be more careful converting JSON to string.
                        new_cell.text = message
                            .value
                            .clone()
                            .as_str()
                            .unwrap_or_default()
                            .to_string();
                        new_cell.messages.push(message);
                        cells.insert(column.to_string(), new_cell);
                    }
                }
            }
        }

        Self {
            id,
            order,
            deleted,
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
    pub messages: Vec<CellMessage>,
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
            cell.messages.push(CellMessage {
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

#[derive(Clone, Debug)]
pub enum TypedValue {
    Regular(DbValue),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct TypedRow {
    pub map: IndexMap<String, TypedValue>,
}

#[derive(Clone, Debug)]
pub struct TypedRows {
    pub rows: Vec<TypedRow>,
}

#[derive(Clone, Debug)]
pub struct ConstrainedValue {
    pub actual: TypedValue,
    pub placeholder: Option<DbValue>,
}

#[derive(Clone, Debug)]
pub struct ConstrainedRow {
    pub map: IndexMap<String, ConstrainedValue>,
}

impl ConstrainedRow {
    pub fn get(&self, k: &str) -> Option<&DbValue> {
        match self.map.get(k) {
            Some(value) => match &value.actual {
                TypedValue::Regular(db_value) => Some(db_value),
                TypedValue::Text(_) => None,
            },
            None => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConstrainedRows {
    pub rows: Vec<ConstrainedRow>,
}

#[derive(Clone, Debug)]
pub struct OrderedRow {
    pub order: RowOrder,
    pub map: IndexMap<String, ConstrainedValue>,
}

impl OrderedRow {
    /// Return version of this row that definitely can be inserted into the regular table.
    /// May use placeholder values.
    pub fn insertable(&self) -> DbRow {
        let mut db_row = DbRow::new();
        db_row.insert("_order".to_string(), DbValue::from(self.order));
        for (key, value) in self.map.iter() {
            match &value.placeholder {
                Some(placeholder) => db_row.insert(key.to_string(), placeholder.clone()),
                None => match value.actual.clone() {
                    TypedValue::Regular(db_value) => db_row.insert(key.to_string(), db_value),
                    TypedValue::Text(_) => db_row.insert(key.to_string(), DbValue::Null),
                },
            }
        }
        db_row
    }
}

#[derive(Clone, Debug)]
pub struct OrderedRows {
    pub rows: Vec<OrderedRow>,
}

impl OrderedRows {
    pub fn order(orders: &Vec<RowOrder>, constrained_rows: ConstrainedRows) -> Self {
        let mut ordered_rows = Vec::new();
        for (order, constrained_row) in zip(orders, constrained_rows.rows) {
            let ordered_row = OrderedRow {
                order: *order,
                map: constrained_row.map,
            };
            ordered_rows.push(ordered_row);
        }
        Self { rows: ordered_rows }
    }

    pub fn insertable(&self) -> Vec<DbRow> {
        self.rows.iter().map(|row| row.insertable()).collect()
    }
}

#[derive(Clone, Debug)]
pub struct InsertedRow {
    pub id: RowID,
    pub order: RowOrder,
    pub deleted: bool,
    pub map: IndexMap<String, ConstrainedValue>,
}

impl InsertedRow {
    pub fn is_regular(&self) -> bool {
        for value in self.map.values() {
            match value.actual {
                TypedValue::Text(_) => return false,
                _ => (),
            };
        }
        true
    }

    /// A DbRow that can be inserted into the regular table,
    /// if this row satisfies all the SQL types and constraints.
    pub fn regular(&self) -> Option<DbRow> {
        let mut db_row = DbRow::new();
        db_row.insert("_id".to_string(), DbValue::from(self.id));
        db_row.insert("_order".to_string(), DbValue::from(self.order));
        for (key, value) in self.map.iter() {
            match value.actual.clone() {
                TypedValue::Regular(db_value) => db_row.insert(key.to_string(), db_value),
                TypedValue::Text(_) => return None,
            }
        }
        Some(db_row)
    }

    /// A DbRow that can be inserted into the alternate table,
    pub fn alternate(&self) -> Option<DbRow> {
        if self.is_regular() {
            return None;
        }
        let mut db_row = DbRow::new();
        db_row.insert("_id".to_string(), DbValue::from(self.id));
        db_row.insert("_order".to_string(), DbValue::from(self.order));
        for (key, value) in self.map.iter() {
            match value.actual.clone() {
                TypedValue::Regular(db_value) => db_row.insert(key.to_string(), db_value),
                TypedValue::Text(text) => db_row.insert(format!("{key}_text"), DbValue::Text(text)),
            }
        }
        Some(db_row)
    }
}

#[derive(Clone, Debug)]
pub struct InsertedRows {
    pub rows: Vec<InsertedRow>,
}

impl InsertedRows {
    pub fn identify(ordered_rows: OrderedRows, db_rows: DbRows) -> Self {
        let mut inserted_rows = Vec::new();
        for db_row in db_rows.rows.iter() {
            for ordered_row in ordered_rows.rows.iter() {
                if db_row
                    .get("_order")
                    .expect("inserted row must have an order")
                    .as_i64()
                    .expect("order should be an i64")
                    == ordered_row.order
                {
                    inserted_rows.push(InsertedRow {
                        id: db_row
                            .get("_id")
                            .expect("inserted row must have an id")
                            .as_i32()
                            .expect("id should be an i32"),
                        order: ordered_row.order,
                        deleted: db_row
                            .get("_deleted")
                            .and_then(|v| v.as_bool())
                            .unwrap_or_default(),
                        map: ordered_row.map.clone(),
                    })
                }
            }
        }

        Self {
            rows: inserted_rows,
        }
    }

    pub fn regular(&self) -> Vec<DbRow> {
        self.rows.iter().filter_map(|row| row.regular()).collect()
    }

    pub fn alternate(&self) -> Vec<DbRow> {
        self.rows.iter().filter_map(|row| row.alternate()).collect()
    }
}

/// Represents a list of rows
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Rows {
    pub schema: Schema,
    pub table_name: String,
    pub rows: Vec<Row>,
}

impl Rows {
    pub fn new(schema: Schema, table: &str, rows: impl IntoDbRows + Send) -> Result<Rows> {
        // TODO: check schema for table
        let mut new_rows = Rows {
            schema,
            table_name: table.to_string(),
            ..Default::default()
        };
        let rows = rows.into_db_rows();
        for row in rows.iter() {
            new_rows.add(row)?;
        }
        Ok(new_rows)
    }

    pub fn add(&mut self, row: impl IntoDbRow + Send) -> Result<&Row> {
        let db_row = row.into_db_row();
        // TODO: Check columns, corerce types
        // for key in db_row.keys() {
        // }
        let row = db_row.into();
        self.rows.push(row);
        Ok(self.rows.last().unwrap())
    }

    pub fn regular(&self) -> Vec<DbRow> {
        self.rows.iter().filter_map(|row| row.regular()).collect()
    }
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
                deleted: false,
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
                messages: vec![CellMessage {
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
                deleted: false,
                change_id: 0,
                cells
            }
        )
    }
}
