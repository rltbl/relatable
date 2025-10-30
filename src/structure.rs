//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[structure](crate::structure)).

use crate as rltbl;
use rltbl::{
    column::Column,
    core::RelatableError,
    sql::{DbTransaction, SqlParam},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fmt::Display, str::FromStr};

/// Represents a column's structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Structure {
    From(Option<String>, String),
}

impl Structure {
    /// Use this structure condition to validate the given column using the given transaction.
    /// If `row` is specified, then only validate that row.
    pub fn validate(
        &self,
        column: &Column,
        row: Option<&u64>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<bool> {
        tracing::trace!("Structre::validate({self:?}, {column:?}, {row:?}, tx)");
        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let mut messages_were_added = false;
        match self {
            Structure::From(s_table, s_column) => {
                let c_table = &column.table;
                let c_column = &column.name;
                let s_table = match s_table {
                    None => c_table,
                    Some(s_table) => s_table,
                };
                let s_table = unquoted_re.replace(&s_table, "$unquoted").to_string();
                let s_column = unquoted_re.replace(&s_column, "$unquoted").to_string();
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
                             "{c_column}" AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{c_table}"
                           WHERE "{c_column}" NOT IN (
                               SELECT "{s_column}" FROM "{s_table}"
                           )"#,
                    sql_param_1 = sql_param_gen.next(),
                    sql_param_2 = sql_param_gen.next(),
                    sql_param_3 = sql_param_gen.next(),
                    sql_param_4 = sql_param_gen.next(),
                );
                let params;
                match row {
                    Some(row) => {
                        sql.push_str(&format!(
                            r#" AND "_id" = {sql_param}"#,
                            sql_param = sql_param_gen.next()
                        ));
                        params = json!([
                            c_table,
                            c_column,
                            format!("key:foreign"),
                            format!("{c_column} must be in {s_table}.{s_column}"),
                            row
                        ]);
                    }
                    None => {
                        params = json!([
                            c_table,
                            c_column,
                            format!("key:foreign"),
                            format!("{c_column} must be in {s_table}.{s_column}"),
                        ]);
                    }
                };
                sql.push_str(r#" RETURNING 1 AS "inserted""#);
                if let Some(_) = tx.query_one(&sql, Some(&params))? {
                    messages_were_added = true;
                }
            }
        };

        tracing::debug!(
            "Validated structure '{}' for column '{}.{}' (row: {:?}) {}",
            self,
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

impl FromStr for Structure {
    type Err = anyhow::Error;

    fn from_str(structure: &str) -> Result<Self> {
        tracing::trace!("Structure::from_str({structure})");
        if structure.starts_with("from(") {
            let re = regex::Regex::new(r"from\(((.+?)\.)?(.+?)\)")?;
            let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
            match re.captures(structure) {
                Some(captures) => {
                    let table = &captures.get(2).and_then(|t| Some(t.as_str()));
                    let table = match table {
                        Some(table) => Some(unquoted_re.replace(table, "$unquoted").to_string()),
                        None => None,
                    };
                    let column = &captures[3];
                    let column = unquoted_re.replace(column, "$unquoted").to_string();
                    Ok(Structure::From(table, column))
                }
                None => {
                    return Err(RelatableError::InputError(format!(
                        "Invalid from() structure: '{structure}'"
                    ))
                    .into());
                }
            }
        } else {
            return Err(
                RelatableError::InputError(format!("Invalid structure: '{structure}'")).into(),
            );
        }
    }
}

impl Display for Structure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Structure::From(s_table, s_column) => match s_table {
                None => write!(f, "from({s_column})"),
                Some(s_table) => write!(f, "from({s_table}.{s_column})"),
            },
        }
    }
}
