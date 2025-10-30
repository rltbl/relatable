//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use crate as rltbl;
use rltbl::{
    column::Column,
    core::{Relatable, RelatableError},
    sql::{self, DbTransaction, SqlParam},
    table::Table,
};

use anyhow::Result;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

lazy_static! {
    /// Relatable's core built-in datatypes
    pub static ref BUILTIN_DATATYPES: Vec<&'static str> =
        vec!["text", "empty", "line", "trimmed_line", "nonspace", "word", "integer"];
}

/// Represents a column's datatype
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Datatype {
    pub name: String,
    pub description: String,
    pub parent: String,
    pub condition: String,
    pub sql_type: String,
    pub format: String,
}

impl Datatype {
    /// Return the SQL type corresponding to the given datatype, or to one of its parents if it
    /// has no sql_type.
    pub fn infer_sql_type(&self, dt_hierarchy: &Vec<Datatype>) -> String {
        tracing::trace!("infer_sql_type({self:?}, {dt_hierarchy:?})");
        if self.sql_type != "" {
            self.sql_type.to_string()
        } else if !dt_hierarchy.is_empty() {
            let mut ancestors = dt_hierarchy.clone();
            let parent = dt_hierarchy[0].clone();
            ancestors.remove(0);
            parent.infer_sql_type(&ancestors)
        } else {
            // Handle built-in types:
            let sql_type = match self.name.to_lowercase().as_str() {
                "text" => "TEXT",
                "int" | "integer" | "tinyint" | "smallint" | "mediumint" | "bigint" => "INTEGER",
                "real" | "decimal" | "numeric" => "NUMERIC",
                datatype
                    if (datatype.starts_with("real")
                        || datatype.starts_with("numeric")
                        || datatype.starts_with("decimal")) =>
                {
                    "NUMERIC"
                }
                datatype
                    if (datatype.starts_with("varchar") || datatype.starts_with("character")) =>
                {
                    "TEXT"
                }
                datatype if BUILTIN_DATATYPES.contains(&datatype) => "TEXT",
                unknown => {
                    tracing::warn!("Cannot infer SQL type for unknown datatype '{unknown}'");
                    "TEXT"
                }
            };
            sql_type.to_string()
        }
    }

    /// Return a Datatype struct corresponding to the given built-in datatype
    pub fn builtin_datatype(datatype: &str) -> Result<Self> {
        tracing::trace!("Datatype::builtin_datatype({datatype})");
        let builtins = Datatype::builtin_datatypes();
        let builtin = match datatype {
            "text" => builtins.get("text").expect("Builtin 'text' not found"),
            "empty" => builtins.get("empty").expect("Builtin 'empty' not found"),
            "line" => builtins.get("line").expect("Builtin 'line' not found"),
            "trimmed_line" => builtins
                .get("trimmed_line")
                .expect("Builtin 'trimmed_line' not found"),
            "nonspace" => builtins
                .get("nonspace")
                .expect("Builtin 'nonspace' not found"),
            "word" => builtins.get("word").expect("Builtin 'word' not found"),
            "integer" => builtins
                .get("integer")
                .expect("Builtin 'integer' not found"),
            unrecognized => {
                return Err(RelatableError::InputError(format!(
                    "Unrecognized built-in datatype: '{unrecognized}'"
                ))
                .into())
            }
        };
        Ok(builtin.to_owned())
    }

    // Returns a [HashMap] representing all of the built-in datatypes, indexed by datatype name
    pub fn builtin_datatypes() -> HashMap<String, Self> {
        tracing::trace!("Datatype::builtin_datatypes()");
        [
            (
                "text".into(),
                Datatype {
                    name: "text".to_string(),
                    description: "any text".to_string(),
                    ..Default::default()
                },
            ),
            (
                "empty".into(),
                Datatype {
                    name: "empty".to_string(),
                    description: "the empty string".to_string(),
                    parent: "text".to_string(),
                    condition: r"equals('')".to_string(),
                    ..Default::default()
                },
            ),
            (
                "line".into(),
                Datatype {
                    name: "line".to_string(),
                    description: "a line of text".to_string(),
                    parent: "text".to_string(),
                    condition: r"match([^\n]+)".to_string(),
                    ..Default::default()
                },
            ),
            (
                "trimmed_line".into(),
                Datatype {
                    name: "trimmed_line".to_string(),
                    description: "a line of text that deos not begin or end with whitespace"
                        .to_string(),
                    parent: "line".to_string(),
                    condition: r"match(\S([^\n]*\S)*)".to_string(),
                    ..Default::default()
                },
            ),
            (
                "nonspace".into(),
                Datatype {
                    name: "nonspace".to_string(),
                    description: "text without whitespace".to_string(),
                    parent: "trimmed_line".to_string(),
                    condition: r"match([^\s]+)".to_string(),
                    ..Default::default()
                },
            ),
            (
                "word".into(),
                Datatype {
                    name: "word".to_string(),
                    description: "a single word: letters, numbers, underscore".to_string(),
                    parent: "nonspace".to_string(),
                    condition: r"match(\w+)".to_string(),
                    ..Default::default()
                },
            ),
            (
                "integer".into(),
                Datatype {
                    name: "integer".to_string(),
                    description: "an integer".to_string(),
                    parent: "nonspace".to_string(),
                    sql_type: "INTEGER".to_string(),
                    condition: r"match(-?\d+)".to_string(),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>()
    }

    /// Get all of the datatypes in the database
    pub async fn get_all_datatypes(rltbl: &Relatable) -> Result<HashMap<String, Self>> {
        tracing::trace!("Datatype::get_all_datatypes({rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let datatypes = Datatype::_get_all_datatypes(&mut tx)?;
        tx.commit()?;
        Ok(datatypes)
    }

    /// Get all of the datatypes in the database using the given transaction
    fn _get_all_datatypes(tx: &mut DbTransaction<'_>) -> Result<HashMap<String, Self>> {
        tracing::trace!("Datatype::_get_all_datatypes(tx)");
        let mut datatypes = Datatype::builtin_datatypes();
        if Table::_table_exists("datatype", tx)? {
            let sql = r#"SELECT * FROM "datatype""#;
            let datatype_rows = tx.query(&sql, None)?;
            for dt_row in &datatype_rows {
                let dt_name = dt_row.get_string("datatype")?;
                datatypes.insert(
                    dt_name.to_string(),
                    Datatype {
                        name: dt_name,
                        description: dt_row.get_string("description")?,
                        parent: dt_row.get_string("parent")?,
                        condition: dt_row.get_string("condition")?,
                        sql_type: dt_row.get_string("sql_type")?,
                        format: dt_row.get_string("format")?,
                    },
                );
            }
        }
        Ok(datatypes)
    }

    /// Get the given [Datatype] from the database
    pub async fn get_datatype(datatype: &str, rltbl: &Relatable) -> Result<Option<Self>> {
        tracing::trace!("Datatype::get_datatype({datatype}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let datatype = Datatype::_get_datatype(datatype, &mut tx)?;
        tx.commit()?;
        Ok(datatype)
    }

    pub fn _get_datatype(datatype: &str, tx: &mut DbTransaction<'_>) -> Result<Option<Self>> {
        tracing::trace!("Datatype::_get_datatype({datatype}, tx)");
        let datatypes = Datatype::_get_all_datatypes(tx)?;
        match datatypes.get(datatype) {
            Some(datatype) => Ok(Some(datatype.to_owned())),
            None => {
                tracing::warn!("No datatype '{datatype}' found");
                Ok(None)
            }
        }
    }

    /// Get all of this datatype's ancestors
    pub async fn get_all_ancestors(&self, rltbl: &Relatable) -> Result<Vec<Self>> {
        tracing::trace!("Datatype::get_all_ancestors({self:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let ancestors = self._get_all_ancestors(&mut tx)?;
        tx.commit()?;
        Ok(ancestors)
    }

    /// Get all of this datatype's ancestors using the given transaction.
    pub fn _get_all_ancestors(&self, tx: &mut DbTransaction<'_>) -> Result<Vec<Self>> {
        tracing::trace!("Datatype::_get_all_ancestors({self:?}, tx)");
        let datatypes = {
            let mut datatypes = Datatype::builtin_datatypes()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_owned()))
                .collect::<HashMap<_, _>>();
            if Table::_table_exists("datatype", tx)? {
                let builtin_names = datatypes.keys().cloned().collect::<Vec<_>>();
                let sql = r#"SELECT * from "datatype""#;
                for row in tx.query(sql, None)? {
                    let dt_name = row.get_string("datatype")?;
                    if builtin_names.contains(&dt_name) {
                        tracing::info!("Ignoring redefinition of built-in datatype '{dt_name}'");
                    } else {
                        datatypes.insert(
                            dt_name.to_string(),
                            Datatype {
                                name: dt_name,
                                description: row.get_string("description").unwrap_or_default(),
                                parent: row.get_string("parent").unwrap_or_default(),
                                condition: row.get_string("condition").unwrap_or_default(),
                                sql_type: row.get_string("sql_type").unwrap_or_default(),
                                format: row.get_string("format").unwrap_or_default(),
                            },
                        );
                    }
                }
            }
            datatypes
        };

        fn build_hierarchy(
            dt_map: &HashMap<String, Datatype>,
            start_dt_name: &str,
            dt_name: &str,
        ) -> Result<Vec<Datatype>> {
            tracing::trace!(
                "Datatype::get_all_ancestors()::build_hierarchy({dt_map:?}, {start_dt_name}, \
                 {dt_name})"
            );
            let mut datatypes = vec![];
            if dt_name != "" {
                let datatype = match dt_map.get(dt_name) {
                    Some(datatype) => datatype,
                    None => {
                        tracing::warn!("Undefined datatype '{dt_name}'");
                        return Ok(datatypes);
                    }
                };
                let dt_name = datatype.name.as_str();
                let dt_parent = datatype.parent.as_str();
                if dt_name != start_dt_name {
                    datatypes.push(datatype.clone());
                }
                let mut more_datatypes = build_hierarchy(dt_map, start_dt_name, &dt_parent)?;
                datatypes.append(&mut more_datatypes);
            }
            Ok(datatypes)
        }

        build_hierarchy(&datatypes, &self.name, &self.name)
    }

    /// Validate a column of a database table, optionally only for the given row, using the
    /// given transaction. Returns true whenever messages are inserted to the message table as a
    /// result of validation, and false otherwise.
    pub fn validate(
        &self,
        column: &Column,
        row: Option<&u64>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<bool> {
        tracing::trace!("Datatype::validate({self:?}, {column:?}, {row:?}, tx)");
        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let mut messages_were_added = false;
        match self.condition.as_str() {
            "" => (),
            condition if condition.starts_with("equals(") => {
                let re = regex::Regex::new(r"^equals\((.+?)\)$")?;
                if let Some(captures) = re.captures(condition) {
                    let condition = &captures[1];
                    let condition = unquoted_re.replace(&condition, "$unquoted");
                    let mut sql_param_gen = SqlParam::new(&tx.kind());
                    let mut sql = format!(
                        r#"INSERT INTO "message"
                             ("added_by", "table", "row", "column", "value", "level", "rule",
                              "message")
                           SELECT
                             'rltbl' AS "added_by",
                             {sql_param_1} AS "table",
                             "_id" AS "row",
                             {sql_param_2} AS "column",
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {casted_column} != {sql_param_5}"#,
                        table_name = column.table,
                        casted_column = sql::cast_column_as_text(&column.name, &tx.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        sql_param_5 = sql_param_gen.next(),
                    );
                    let params;
                    match row {
                        Some(row) => {
                            sql.push_str(&format!(
                                r#" AND "_id" = {sql_param}"#,
                                sql_param = sql_param_gen.next()
                            ));
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.name),
                                format!("{} must be a {}", column.name, self.name),
                                condition,
                                row
                            ]);
                        }
                        None => {
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.name),
                                format!("{} must be a {}", column.name, self.name),
                                condition
                            ]);
                        }
                    };
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    if let Some(_) = tx.query_one(&sql, Some(&params))? {
                        messages_were_added = true;
                    }
                }
            }
            condition if condition.starts_with("in(") => {
                let re = regex::Regex::new(r"^in\((.+?)\)$").unwrap();
                if let Some(captures) = re.captures(condition) {
                    let list_separator = regex::Regex::new(r"\s*,\s*").unwrap();
                    let condition_list_str = &captures[1];
                    let condition_list = list_separator
                        .split(condition_list_str)
                        .map(|item| unquoted_re.replace(item, "$unquoted"))
                        .collect::<Vec<_>>();
                    let mut sql_param_gen = SqlParam::new(&tx.kind());
                    let mut sql = format!(
                        r#"INSERT INTO "message"
                             ("added_by", "table", "row", "column", "value", "level", "rule",
                              "message")
                           SELECT
                             'rltbl' AS "added_by",
                             {sql_param_1} AS "table",
                             "_id" AS "row",
                             {sql_param_2} AS "column",
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {casted_column} NOT IN ({sql_param_5})"#,
                        table_name = column.table,
                        casted_column = sql::cast_column_as_text(&column.name, &tx.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        sql_param_5 = sql_param_gen.get_as_list(condition_list.len()),
                    );
                    let mut params = json!([
                        column.table,
                        column.name,
                        format!("datatype:{}", self.name),
                        format!("{} must be a {}", column.name, self.name),
                    ]);
                    for item in &condition_list {
                        if let JsonValue::Array(ref mut v) = params {
                            v.push(json!(item));
                        }
                    }
                    if let Some(row) = row {
                        sql.push_str(&format!(
                            r#" AND "_id" = {sql_param}"#,
                            sql_param = sql_param_gen.next()
                        ));
                        if let JsonValue::Array(ref mut v) = params {
                            v.push(json!(row));
                        }
                    }
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    if let Some(_) = tx.query_one(&sql, Some(&params))? {
                        messages_were_added = true;
                    }
                }
            }
            condition if condition.starts_with("match(") => {
                let re = regex::Regex::new(r"^match\((.+?)\)$")?;
                if let Some(captures) = re.captures(condition) {
                    let condition = &captures[1];
                    let condition = unquoted_re.replace(&condition, "$unquoted");
                    let mut sql_param_gen = SqlParam::new(&tx.kind());
                    let mut sql = format!(
                        r#"INSERT INTO "message"
                             ("added_by", "table", "row", "column", "value", "level", "rule",
                              "message")
                           SELECT
                             'rltbl' AS "added_by",
                             {sql_param_1} AS "table",
                             "_id" AS "row",
                             {sql_param_2} AS "column",
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {match_condition}"#,
                        table_name = column.table,
                        casted_column = sql::cast_column_as_text(&column.name, &tx.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        match_condition = sql::regexp_mismatch(&column.name, &mut sql_param_gen),
                    );
                    let params;
                    match row {
                        Some(row) => {
                            sql.push_str(&format!(
                                r#" AND "_id" = {sql_param}"#,
                                sql_param = sql_param_gen.next()
                            ));
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.name),
                                format!("{} must be a {}", column.name, self.name),
                                format!("^{condition}$"),
                                row
                            ]);
                        }
                        None => {
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.name),
                                format!("{} must be a {}", column.name, self.name),
                                format!("^{condition}$")
                            ]);
                        }
                    };
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    if let Some(_) = tx.query_one(&sql, Some(&params))? {
                        messages_were_added = true;
                    }
                }
            }
            invalid => tracing::warn!("Unrecognized datatype condition '{invalid}'"),
        };

        tracing::debug!(
            "Validated datatype '{}' for column '{}.{}' (row: {:?}) {}",
            self.name,
            column.table,
            column.name,
            row,
            match messages_were_added {
                false => "with messages added.",
                true => "with no messages added.",
            }
        );
        Ok(messages_were_added)
    }
}
