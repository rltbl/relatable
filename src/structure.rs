//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[structure](crate::structure)).

use crate as rltbl;
use rltbl::{
    column::Column,
    core::{Relatable, RelatableError, RowID},
    sql::SqlParam,
};
use rltbl_db::core::{DbQuery, ParamValue};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

/// Represents a column's structure.
#[derive(Clone, Debug, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub enum Structure {
    From(Option<String>, String),
}

impl Default for Structure {
    fn default() -> Self {
        Structure::From(None, String::new())
    }
}

impl Structure {
    /// Use this structure condition to validate the given column using the given transaction.
    /// If `row` is specified, then only validate that row.
    pub async fn validate(
        &self,
        column: &Column,
        rows: &[&RowID],
        rltbl: &Relatable,
    ) -> Result<bool> {
        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let messages_were_added;
        match self {
            Structure::From(s_table, s_column) => {
                let c_table = &column.table;
                let c_column = &column.column;
                let s_table = match s_table {
                    None => c_table,
                    Some(s_table) => s_table,
                };
                let s_table = unquoted_re.replace(&s_table, "$unquoted").to_string();
                let s_column = unquoted_re.replace(&s_column, "$unquoted").to_string();
                let mut sql_param_gen = SqlParam::new(&rltbl.pool.kind());
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
                let mut params: Vec<ParamValue> = vec![
                    c_table.to_owned(),
                    c_column.to_owned(),
                    format!("key:foreign"),
                    format!("{c_column} must be in {s_table}.{s_column}"),
                ]
                .iter()
                .map(|v| v.into())
                .collect();
                if rows.len() > 0 {
                    sql.push_str(&format!(
                        r#" AND "_id" IN({sql_params})"#,
                        sql_params = sql_param_gen.get_as_list(rows.len()),
                    ));
                    params.extend(rows.iter().map(|row| ParamValue::from(**row)));
                }
                sql.push_str(r#" RETURNING 1 AS "inserted""#);
                let rows = rltbl.pool.query(&sql, params).await?;
                messages_were_added = rows.len() > 0;
            }
        };
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

impl TryFrom<&str> for Structure {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<String> for Structure {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for Structure {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Clone, Debug, Default, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "String", into = "String")]
pub struct Structures {
    list: Vec<Structure>,
}

impl Deref for Structures {
    type Target = Vec<Structure>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for Structures {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl Into<Vec<Structure>> for Structures {
    fn into(self) -> Vec<Structure> {
        self.list
    }
}

impl FromStr for Structures {
    type Err = anyhow::Error;

    fn from_str(structure: &str) -> Result<Self> {
        match structure.trim() {
            "" => Ok(Self::default()),
            // TODO: handle multiple structures
            _ => Ok(Structures {
                list: vec![Structure::from_str(structure)?],
            }),
        }
    }
}

impl From<&str> for Structures {
    fn from(value: &str) -> Self {
        match Self::from_str(value) {
            Ok(structure) => structure,
            Err(_) => Self::default(),
        }
    }
}

impl TryFrom<String> for Structures {
    type Error = anyhow::Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl From<Structures> for String {
    fn from(value: Structures) -> Self {
        value.to_string()
    }
}

impl Display for Structures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.list
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .join(" ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use derive_builder::Builder;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn test_structure() {
        let string = "from(tbl.col)";
        let structure = Structure::from_str(&string).expect("get structure");
        assert_eq!(
            structure,
            Structure::From(Some("tbl".to_owned()), "col".to_owned())
        );
        assert_eq!(structure.to_string(), string);
        assert_eq!(json!(structure), json!(string));

        let structure: Structure = string.try_into().expect("valid structure");
        assert_eq!(
            structure,
            Structure::From(Some("tbl".to_owned()), "col".to_owned())
        );
    }

    #[test]
    fn test_structures() {
        let string = "";
        let structures = Structures::from_str(&string).expect("get structure");
        assert_eq!(structures, Structures { list: vec![] });
        assert_eq!(structures.to_string(), string);
        assert_eq!(json!(structures), json!(string));

        let string = "from(tbl.col)";
        let structures = Structures::from_str(&string).expect("get structure");
        assert_eq!(
            structures,
            Structures {
                list: vec![Structure::From(Some("tbl".to_owned()), "col".to_owned())]
            }
        );
        assert_eq!(structures.to_string(), string);
        assert_eq!(json!(structures), json!(string));

        let structures: Structures = string.into();
        assert_eq!(
            structures,
            Structures {
                list: vec![Structure::From(Some("tbl".to_owned()), "col".to_owned())]
            }
        );
    }

    #[derive(Builder, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
    #[builder(default, setter(into))]
    struct Column {
        structure: Structures,
    }

    #[test]
    fn test_column() {
        let column = ColumnBuilder::default()
            .structure("from(tbl.col)")
            .build()
            .unwrap();
        assert_eq!(json!(column), json!({"structure": "from(tbl.col)"}));
    }
}
