//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[datatype](crate::datatype)).

use crate as rltbl;
use indexmap::IndexMap;
use rltbl::{
    column::Column,
    sql::{self, DbTransaction, SqlParam},
};
use rltbl_db::core::{DbKind, DbQuery, JsonRow};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

pub type DatatypeMap = IndexMap<String, Datatype>;

/// Represents a column's datatype
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd, Ord, Eq)]
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

impl Datatype {
    /// Create a new datatype.
    pub fn new(datatype: &str) -> Self {
        Datatype {
            datatype: datatype.to_owned(),
            ..Default::default()
        }
    }

    /// Rename the datatype
    pub fn name(mut self, datatype: &str) -> Self {
        self.datatype = datatype.to_owned();
        self
    }

    /// Set the description.
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_owned();
        self
    }

    /// Set the parent.
    pub fn parent(mut self, parent: &str) -> Self {
        self.parent = parent.to_owned();
        self
    }

    /// Set the condition.
    pub fn condition(mut self, condition: &str) -> Self {
        self.condition = condition.to_owned();
        self
    }

    /// Set the sql_type.
    pub fn sql_type(mut self, sql_type: &str) -> Self {
        self.sql_type = sql_type.to_owned();
        self
    }

    /// Set the format.
    pub fn format(mut self, format: &str) -> Self {
        self.format = format.to_owned();
        self
    }

    /// Get the sql_type of this datatype, or its closest ancestor.
    /// The default sql_type is "TEXT".
    pub fn get_sql_type(&self, datatypes: &DatatypeMap) -> String {
        match self.sql_type.as_str() {
            "" => match self.get_parent(datatypes) {
                Some(parent) => parent.get_sql_type(datatypes),
                None => "TEXT".to_owned(),
            },
            sql_type => sql_type.to_owned(),
        }
    }

    /// Get the parent Datatype from the full list of datatypes.
    pub fn get_parent<'a>(&self, datatypes: &'a DatatypeMap) -> Option<&'a Datatype> {
        if self.parent != "" {
            datatypes.get(&self.parent)
        } else {
            None
        }
    }

    /// Extract a vector of ancestors for this datatype,
    /// starting with itself.
    pub fn ancestors<'a>(&'a self, datatypes: &'a DatatypeMap) -> Vec<&'a Datatype> {
        let mut result = vec![self];
        match self.get_parent(datatypes) {
            Some(parent) => result.extend(parent.ancestors(datatypes)),
            None => (),
        }
        result
    }

    // TODO: Eliminate this in favour of reading the actual SQL type for the column from the database.
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
            let sql_type = match self.datatype.to_lowercase().as_str() {
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
                datatype if DatatypeTable::builtins().contains_key(datatype) => "TEXT",
                unknown => {
                    tracing::warn!("Cannot infer SQL type for unknown datatype '{unknown}'");
                    "TEXT"
                }
            };
            sql_type.to_string()
        }
    }

    // TODO: break into smaller pieces
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
                                format!("datatype:{}", self.datatype),
                                format!("{} must be a {}", column.name, self.datatype),
                                condition,
                                row
                            ]);
                        }
                        None => {
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.datatype),
                                format!("{} must be a {}", column.name, self.datatype),
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
                        format!("datatype:{}", self.datatype),
                        format!("{} must be a {}", column.name, self.datatype),
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
                                format!("datatype:{}", self.datatype),
                                format!("{} must be a {}", column.name, self.datatype),
                                format!("^{condition}$"),
                                row
                            ]);
                        }
                        None => {
                            params = json!([
                                column.table,
                                column.name,
                                format!("datatype:{}", self.datatype),
                                format!("{} must be a {}", column.name, self.datatype),
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
            self.datatype,
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

/// Represents the special "datatype" table.
pub struct DatatypeTable {}

impl DatatypeTable {
    /// Get the SQL DDL as a string.
    /// Requires the db only to know the SQL flavour to use.
    pub fn ddl(db: &impl DbQuery) -> String {
        let pkey_clause = match db.kind() {
            DbKind::SQLite => "INTEGER PRIMARY KEY AUTOINCREMENT",
            DbKind::PostgreSQL => "SERIAL PRIMARY KEY",
        };

        format!(
            r#"CREATE TABLE "datatype" (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             "datatype" TEXT,
             "description" TEXT,
             "parent" TEXT,
             "condition" TEXT,
             "sql_type" TEXT,
             "format" TEXT
           )"#,
        )
    }

    /// Create the "datatype" table in the database
    /// and insert the built-in datatypes.
    pub async fn create(db: &impl DbQuery) -> Result<()> {
        db.execute(&DatatypeTable::ddl(db), &[]).await?;
        let rows: Vec<JsonRow> = DatatypeTable::builtins()
            .values()
            .map(|dt| json!(dt).as_object().unwrap().clone())
            .collect();
        let refs: Vec<&JsonRow> = rows.iter().collect();
        db.insert("datatype", &refs).await?;
        Ok(())
    }

    // Returns an [IndexMap] representing all of the built-in datatypes, indexed by datatype name
    pub fn builtins() -> IndexMap<String, Datatype> {
        tracing::trace!("Datatype::builtin_datatypes()");
        [
            (
                "text".into(),
                Datatype {
                    datatype: "text".to_owned(),
                    parent: String::new(),
                    description: "any text".to_owned(),
                    sql_type: "TEXT".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "empty".into(),
                Datatype {
                    datatype: "empty".to_owned(),
                    description: "the empty string".to_owned(),
                    parent: "text".to_owned(),
                    condition: r"equals('')".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "line".into(),
                Datatype {
                    datatype: "line".to_owned(),
                    description: "a line of text".to_owned(),
                    parent: "text".to_owned(),
                    condition: r"match([^\n]+)".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "trimmed_line".into(),
                Datatype {
                    datatype: "trimmed_line".to_owned(),
                    description: "a line of text that deos not begin or end with whitespace"
                        .to_owned(),
                    parent: "line".to_owned(),
                    condition: r"match(\S([^\n]*\S)*)".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "nonspace".into(),
                Datatype {
                    datatype: "nonspace".to_owned(),
                    description: "text without whitespace".to_owned(),
                    parent: "trimmed_line".to_owned(),
                    condition: r"match([^\s]+)".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "word".into(),
                Datatype {
                    datatype: "word".to_owned(),
                    description: "a single word: letters, numbers, underscore".to_owned(),
                    parent: "nonspace".to_owned(),
                    condition: r"match(\w+)".to_owned(),
                    ..Default::default()
                },
            ),
            (
                "integer".into(),
                Datatype {
                    datatype: "integer".to_owned(),
                    description: "an integer".to_owned(),
                    parent: "nonspace".to_owned(),
                    sql_type: "INTEGER".to_owned(),
                    condition: r"match(-?\d+)".to_owned(),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect::<IndexMap<_, _>>()
    }

    /// Insert this datatype into the "datatype" table,
    /// returning the result.
    pub async fn add(db: &impl DbQuery, datatype: &Datatype) -> Result<Datatype> {
        let row = json!(datatype);
        let row = row.as_object().unwrap();
        let rows = db.insert("datatype", &[&row]).await?;
        let row = rows.get(0).unwrap();
        let dt: Datatype = serde_json::from_value(json!(row))?;
        Ok(dt)
    }

    /// Get all the dataypes from the "datatype" table.
    /// Built-in datatypes override rows found in the table.
    /// If the "datatype" table does not exist, just return buildins.
    pub async fn get(db: &impl DbQuery) -> DatatypeMap {
        let rows = match db
            .query(
                "SELECT datatype, description, parent, sql_type, condition, format FROM datatype",
                &[],
            )
            .await
        {
            Ok(rows) => rows,
            Err(_) => return DatatypeTable::builtins(),
        };
        let mut map = rows
            .iter()
            .map(|row| serde_json::from_value(json!(row)))
            .filter_map(|result| result.ok())
            .map(|dt: Datatype| (dt.datatype.to_string(), dt))
            .collect::<IndexMap<_, _>>();
        map.extend(DatatypeTable::builtins());
        map
    }

    // validate the "datatype" table
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;

    #[tokio::test]
    async fn test_create() {
        let pool = AnyPool::connect(":memory:")
            .await
            .expect("connect to SQLite");
        DatatypeTable::create(&pool)
            .await
            .expect("create datatype table");
        let count = pool
            .query_u64("SELECT count() FROM datatype", &[])
            .await
            .expect("count rows");
        assert_eq!(count, DatatypeTable::builtins().len() as u64);
    }

    #[tokio::test]
    async fn test_add() {
        let pool = AnyPool::connect(":memory:")
            .await
            .expect("connect to SQLite");
        DatatypeTable::create(&pool)
            .await
            .expect("create datatype table");
        let test = Datatype::new("test").description("test datatype");
        DatatypeTable::add(&pool, &test)
            .await
            .expect("insert test datatype");
        let count = pool
            .query_u64("SELECT count() FROM datatype", &[])
            .await
            .expect("count rows");
        assert_eq!(count as usize, DatatypeTable::builtins().len() + 1);
        assert_eq!(&test, DatatypeTable::get(&pool).await.get("test").unwrap());
    }

    #[tokio::test]
    async fn test_priority() {
        // built-ins take priority over rows from the table
        let pool = AnyPool::connect(":memory:")
            .await
            .expect("connect to SQLite");
        DatatypeTable::create(&pool)
            .await
            .expect("create datatype table");
        pool.execute(
            "UPDATE datatype SET description = 'FOO' WHERE datatype = 'text'",
            &[],
        )
        .await
        .expect("update datatype table");
        assert_eq!(
            DatatypeTable::builtins().get("text").unwrap(),
            DatatypeTable::get(&pool).await.get("text").unwrap()
        );
    }

    #[tokio::test]
    async fn test_ancestors() {
        // built-ins take priority over rows from the table
        let datatypes = DatatypeTable::builtins();
        let dt = datatypes.get("integer").unwrap();
        let ancestors = dt.ancestors(&datatypes);
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
    }
}
