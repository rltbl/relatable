//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use crate as rltbl;
use rltbl::{
    column::Column,
    core::{meta_column_ddl, Relatable, RelatableError, RowID},
    sql::{self, SqlParam},
};
use rltbl_db::{
    any::AnyPool,
    core::DbQuery,
    db_kind::DbKind,
    db_value::{DbValue, JsonRow},
};

use indexmap::IndexMap;
use regex::Regex;

use anyhow::Result;
use derive_builder::Builder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ops::{Deref, DerefMut};

pub static COLUMNS: [&str; 6] = [
    "datatype",
    "description",
    "parent",
    "condition",
    "sql_type",
    "format",
];

/// Represents a column's datatype
#[derive(Builder, Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq)]
#[builder(default, setter(into))]
pub struct Datatype {
    pub datatype: String,
    pub description: String,
    pub parent: String,
    pub condition: String,
    pub sql_type: String,
    pub format: String,
}

impl Default for Datatype {
    fn default() -> Self {
        Self {
            datatype: Default::default(),
            description: Default::default(),
            parent: "text".to_owned(),
            condition: Default::default(),
            sql_type: Default::default(),
            format: Default::default(),
        }
    }
}

impl DatatypeBuilder {
    /// Create a new datatype.
    pub fn new(datatype: &str) -> Self {
        let mut new = Self::default();
        new.datatype = Some(datatype.to_owned());
        new
    }
}

impl Datatype {
    // TODO: break into smaller pieces
    /// Validate a column of a database table, optionally only for the given row, using the
    /// given transaction. Returns true whenever messages are inserted to the message table as a
    /// result of validation, and false otherwise.
    pub async fn validate(
        &self,
        column: &Column,
        rows: &[&RowID],
        rltbl: &Relatable,
    ) -> Result<bool> {
        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let mut messages_were_added = false;
        match self.condition.as_str() {
            "" => (),
            condition if condition.starts_with("equals(") => {
                let re = regex::Regex::new(r"^equals\((.+?)\)$")?;
                if let Some(captures) = re.captures(condition) {
                    let condition = &captures[1];
                    let condition = unquoted_re.replace(&condition, "$unquoted");
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
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {casted_column} != {sql_param_5}"#,
                        table_name = column.table,
                        casted_column =
                            sql::cast_column_as_text(&column.column, &rltbl.pool.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        sql_param_5 = sql_param_gen.next(),
                    );
                    let mut params: Vec<DbValue> = vec![
                        column.table.clone(),
                        column.column.clone(),
                        format!("datatype:{}", self.datatype),
                        format!("{} must be a {}", column.column, self.datatype),
                        condition.to_string(),
                    ]
                    .iter()
                    .map(|v| v.into())
                    .collect();
                    if rows.len() > 0 {
                        sql.push_str(&format!(
                            r#" AND "_id" IN({sql_params})"#,
                            sql_params = sql_param_gen.get_as_list(rows.len()),
                        ));
                        params.extend(rows.iter().map(|row| DbValue::from(**row)));
                    }
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    let rows = rltbl.pool.query(&sql, params).await?;
                    messages_were_added = rows.len() > 0;
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
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {casted_column} NOT IN ({sql_param_5})"#,
                        table_name = column.table,
                        casted_column =
                            sql::cast_column_as_text(&column.column, &rltbl.pool.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        sql_param_5 = sql_param_gen.get_as_list(condition_list.len()),
                    );
                    let mut params: Vec<DbValue> = vec![
                        column.table.clone(),
                        column.column.clone(),
                        format!("datatype:{}", self.datatype),
                        format!("{} must be a {}", column.column, self.datatype),
                    ]
                    .iter()
                    .map(|v| v.into())
                    .collect();
                    for item in &condition_list {
                        params.push(item.to_string().into())
                    }
                    if rows.len() > 0 {
                        sql.push_str(&format!(
                            r#" AND "_id" IN({sql_params})"#,
                            sql_params = sql_param_gen.get_as_list(rows.len()),
                        ));
                        params.extend(rows.iter().map(|row| DbValue::from(**row)));
                    }
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    let rows = rltbl.pool.query(&sql, params).await?;
                    messages_were_added = rows.len() > 0;
                }
            }
            condition if condition.starts_with("match(") => {
                let re = regex::Regex::new(r"^match\((.+?)\)$")?;
                if let Some(captures) = re.captures(condition) {
                    let condition = &captures[1];
                    let condition = unquoted_re.replace(&condition, "$unquoted");
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
                             {casted_column} AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE {match_condition}"#,
                        table_name = column.table,
                        casted_column =
                            sql::cast_column_as_text(&column.column, &rltbl.pool.kind()),
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        match_condition = sql::regexp_mismatch(&column.column, &mut sql_param_gen),
                    );
                    let mut params: Vec<DbValue> = vec![
                        column.table.clone(),
                        column.column.clone(),
                        format!("datatype:{}", self.datatype),
                        format!("{} must be a {}", column.column, self.datatype),
                        condition.to_string(),
                    ]
                    .iter()
                    .map(|v| v.into())
                    .collect();
                    if rows.len() > 0 {
                        sql.push_str(&format!(
                            r#" AND "_id" IN({sql_params})"#,
                            sql_params = sql_param_gen.get_as_list(rows.len()),
                        ));
                        params.extend(rows.iter().map(|row| DbValue::from(**row)));
                    }
                    sql.push_str(r#" RETURNING 1 AS "inserted""#);
                    // TODO: re-enable this!
                    // let rows = rltbl.pool.query(&sql, params).await?;
                    // messages_were_added = rows.len() > 0;
                }
            }
            invalid => tracing::warn!("Unrecognized datatype condition '{invalid}'"),
        };

        Ok(messages_were_added)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Datatypes {
    map: IndexMap<String, Datatype>,
}

impl Deref for Datatypes {
    type Target = IndexMap<String, Datatype>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for Datatypes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl Datatypes {
    // Returns an [IndexMap] representing all of the built-in datatypes, indexed by datatype name
    pub fn builtins() -> Self {
        Datatypes {
            map: [
                DatatypeBuilder::new("text")
                    .description("any text")
                    .parent("")
                    .sql_type("text")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("empty")
                    .description("the empty string")
                    .parent("text")
                    .condition("equals('')")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("line")
                    .description("a line of text")
                    .parent("text")
                    .condition(r"match([^\n]+)")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("trimmed_line")
                    .description("a line of text that deos not begin or end with whitespace")
                    .parent("line")
                    .condition(r"match(\S([^\n]*\S)*)")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("nonspace")
                    .description("text without whitespace")
                    .parent("trimmed_line")
                    .condition(r"match([^\s]+)")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("word")
                    .description("a single word: letters, numbers, underscore")
                    .parent("nonspace")
                    .condition(r"match(\w+)")
                    .build()
                    .unwrap(),
                DatatypeBuilder::new("integer")
                    .description("an integer")
                    .parent("nonspace")
                    .sql_type("INTEGER")
                    .condition(r"match(-?\d+)")
                    .build()
                    .unwrap(),
            ]
            .into_iter()
            .map(|dt| (dt.datatype.clone(), dt))
            .collect::<IndexMap<_, _>>(),
        }
    }

    /// Get the sql_type of the given datatype, or its closest ancestor.
    /// The default sql_type is "text".
    pub fn sql_type(&self, datatype: &Datatype) -> String {
        match datatype.sql_type.as_str() {
            "" => match self.parent(datatype) {
                Some(parent) => self.sql_type(&parent),
                None => "text".to_owned(),
            },
            sql_type => sql_type.to_owned(),
        }
    }

    /// Get the parent Datatype from the full list of datatypes.
    pub fn parent(&self, datatype: &Datatype) -> Option<&Datatype> {
        self.get(&datatype.parent)
    }

    /// Extract a vector of ancestors for this datatype,
    /// starting with itself.
    pub fn ancestors<'a>(&'a self, datatype: &'a Datatype) -> Vec<&'a Datatype> {
        let mut ancestors = vec![datatype];
        match self.parent(datatype) {
            Some(parent) => ancestors.extend(self.ancestors(&parent)),
            None => (),
        }
        ancestors
    }
}

/// Represents the special "datatype" table.
pub struct DatatypeTable<'a> {
    // This table_name is "datatype" by default.
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> DatatypeTable<'a> {
    /// Create a new instance of DatatypeTable from an AnyPool.
    pub fn connect<'b>(pool: &'b AnyPool) -> Self
    where
        'b: 'a,
    {
        Self {
            table_name: "datatype".to_owned(),
            pool,
        }
    }

    // TODO: use rltbl_db to sanitize the name
    /// Use this name for the datatype table.
    /// The default is "datatype".
    pub fn name(mut self, table_name: &str) -> Result<Self> {
        let pattern = Regex::new(r"^\w+$").unwrap();
        if !pattern.is_match(table_name) {
            return Err(
                RelatableError::DataError(format!("Not a valid table name: {table_name}")).into(),
            );
        }
        self.table_name = table_name.to_owned();
        Ok(self)
    }

    /// Get the SQL DDL as a string.
    /// Requires the db only to know the SQL flavour to use.
    pub fn ddl(&self) -> String {
        format!(
            r#"CREATE TABLE "{table_name}" (
              {meta_columns},
              "datatype" TEXT,
              "description" TEXT,
              "parent" TEXT,
              "condition" TEXT,
              "sql_type" TEXT,
              "format" TEXT
            )"#,
            table_name = self.table_name,
            meta_columns = meta_column_ddl(&self.pool.kind()),
        )
    }

    // TODO: replace this with self.pool.drop(self.name).
    /// Drop the datatype table from the database.
    pub async fn drop(&self) -> Result<()> {
        let sql = match self.pool.kind() {
            DbKind::SQLite => {
                format!(r#"DROP TABLE IF EXISTS "{}""#, self.table_name)
            }
            DbKind::PostgreSQL => {
                format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, self.table_name)
            }
        };
        self.pool.execute(&sql, ()).await?;
        Ok(())
    }

    /// Create the "datatype" table in the database
    /// and insert the built-in datatypes.
    pub async fn create(&self) -> Result<()> {
        self.pool.execute(&self.ddl(), ()).await?;
        let rows: Vec<JsonRow> = Datatypes::builtins()
            .values()
            .map(|dt| json!(dt).as_object().unwrap().clone())
            .collect();
        self.pool.insert(&self.table_name, &COLUMNS, rows).await?;
        Ok(())
    }

    /// Insert these datatypes into the "datatype" table,
    /// returning the results.
    pub async fn add(&self, datatypes: &[&Datatype]) -> Result<Vec<Datatype>> {
        let rows: Vec<JsonRow> = datatypes
            .iter()
            .map(|dt| json!(dt).as_object().unwrap().clone())
            .collect();
        let db_rows = self
            .pool
            .insert_returning(&self.table_name, &COLUMNS, rows, &[])
            .await?;
        let dts: Vec<Datatype> = db_rows
            .rows
            .into_iter()
            // WARN: This silently ignores invalid datatypes.
            .filter_map(|row| serde_json::from_value::<Datatype>(json!(row)).ok())
            .collect();
        Ok(dts)
    }

    /// Get all the dataypes from the datatype table.
    /// Built-in datatypes override rows found in the table.
    /// If the datatype table does not exist, just return buildins.
    pub async fn get(&self) -> Datatypes {
        let datatypes: Vec<Datatype> = self.pool
            .query(
                &format!(
                    r#"SELECT "datatype", "description", "parent", "condition", "sql_type", "format" FROM "{}""#,
                    self.table_name
                ),
                (),
            )
            .await
            .and_then(|db_rows| db_rows.try_into_vec())
            .unwrap_or_default();
        let mut map = datatypes
            .into_iter()
            .map(|dt: Datatype| (dt.datatype.to_string(), dt))
            .collect::<IndexMap<_, _>>();
        map.extend(Datatypes::builtins().map);
        Datatypes { map }
    }

    // validate the "datatype" table
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_create() -> Result<()> {
        let rltbl = Relatable::test("test_datatype_create", false).await?;

        let table = DatatypeTable::connect(&rltbl.pool);
        table.create().await?;
        let count: u64 = rltbl
            .pool
            .query("SELECT count(1) FROM datatype", ())
            .await?
            .try_into()?;
        assert_eq!(count, Datatypes::builtins().len() as u64);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_add() -> Result<()> {
        let rltbl = Relatable::test("test_datatype_add", false).await?;

        let table = DatatypeTable::connect(&rltbl.pool);
        table.create().await?;
        let test = DatatypeBuilder::new("test")
            .description("test datatype")
            .build()
            .unwrap();
        table.add(&[&test]).await?;

        let count: u64 = rltbl
            .pool
            .query("SELECT count(1) FROM datatype", ())
            .await?
            .try_into()?;
        assert_eq!(count as usize, Datatypes::builtins().len() + 1);
        assert_eq!(&test, table.get().await.get("test").unwrap());

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_priority() -> Result<()> {
        // built-ins take priority over rows from the table
        let rltbl = Relatable::test("test_datatype_priority", false).await?;

        let table = DatatypeTable::connect(&rltbl.pool);
        table.create().await?;
        rltbl
            .pool
            .execute(
                "UPDATE datatype SET description = 'FOO' WHERE datatype = 'text'",
                (),
            )
            .await?;
        assert_eq!(
            Datatypes::builtins().get("text").unwrap(),
            table.get().await.get("text").unwrap()
        );

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_ancestors() -> Result<()> {
        // built-ins take priority over rows from the table
        let datatypes = Datatypes::builtins();
        let integer = datatypes.get("integer").unwrap();
        let ancestors = datatypes.ancestors(integer);
        assert_eq!(
            ancestors,
            vec![
                datatypes.get("integer").unwrap(),
                datatypes.get("nonspace").unwrap(),
                datatypes.get("trimmed_line").unwrap(),
                datatypes.get("line").unwrap(),
                datatypes.get("text").unwrap(),
            ]
        );
        Ok(())
    }
}
