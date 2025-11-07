//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[column](crate::column)).

use std::{
    collections::HashSet,
    fmt,
    ops::{Deref, DerefMut},
};

use crate as rltbl;
use indexmap::IndexMap;
use rltbl::{
    core::RelatableError,
    datatype::Datatypes,
    structure::{Structure, Structures},
};
use rltbl_db::{
    any::AnyPool,
    core::{DbKind, DbQuery, JsonRow},
};

use anyhow::Result;
use derive_builder::Builder;
use regex::Regex;
use serde::{
    de::{self, Visitor},
    Deserialize, Serialize,
};
use serde_json::json;

/// Represents a column from some table
#[derive(
    Builder, Clone, Debug, Default, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(default)]
#[builder(default, setter(into))]
pub struct Column {
    pub table: String,
    pub column: String,
    pub label: String,
    pub description: String,
    pub sql_type: String,
    pub nulltype: String,
    pub datatype: String,
    pub structure: Structures,
    #[serde(deserialize_with = "to_bool")]
    pub primary_key: bool,
    #[serde(deserialize_with = "to_bool")]
    pub unique: bool,
}

// Adapted from serde_with BoolFromInt
// https://tg-rs.github.io/tgbot/serde_with/struct.BoolFromInt.html#impl-DeserializeAs%3C'de,+bool%3E-for-BoolFromInt
/// Deserialize various values to a bool.
/// Required to convert SQLite booleans.
fn to_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct BoolVisitor;
    impl Visitor<'_> for BoolVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an integer")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v)
        }

        fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }

        fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }

        fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }

        fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v != 0)
        }
    }

    deserializer.deserialize_u8(BoolVisitor)
}

impl Column {
    /// True if this is a "meta" column.
    pub fn is_meta(&self) -> bool {
        self.column.starts_with("_")
    }

    /// True if this is a "meta" column.
    pub fn is_data(&self) -> bool {
        !self.is_meta()
    }

    /// Get the SQL type for this column according to its datatype,
    /// or the first column ancestor with a sql_type,
    /// or just "TEXT".
    pub fn sql_type(&self, datatypes: &Datatypes) -> String {
        if self.sql_type != "" {
            return self.sql_type.clone();
        }
        let datatype = datatypes
            .get(&self.datatype)
            .unwrap_or(datatypes.get("text").unwrap());
        let ancestors = datatypes.ancestors(datatype);
        match ancestors.iter().filter(|dt| dt.sql_type != "").nth(0) {
            Some(dt) => dt.sql_type.to_owned(),
            None => "TEXT".to_owned(),
        }
    }
}

impl ColumnBuilder {
    pub fn new(table: &str, column: &str) -> Self {
        let mut new = Self::default();
        new.table = Some(table.to_owned());
        new.column = Some(column.to_owned());
        new
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Columns {
    list: Vec<Column>,
}

impl Deref for Columns {
    type Target = Vec<Column>;

    fn deref(&self) -> &Self::Target {
        &self.list
    }
}

impl DerefMut for Columns {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.list
    }
}

impl Into<Vec<Column>> for Columns {
    fn into(self) -> Vec<Column> {
        self.list
    }
}

impl Into<IndexMap<String, Column>> for Columns {
    fn into(self) -> IndexMap<String, Column> {
        self.list
            .into_iter()
            .map(|col| (col.column.clone(), col.clone()))
            .collect()
    }
}

impl Columns {
    // Returns an [IndexMap] representing all of the built-in columns, indexed by column name
    pub fn builtins() -> Self {
        Columns {
            list: vec![
                ColumnBuilder::default()
                    .table("column")
                    .column("table")
                    .description("the table for this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "column")
                    .description("the name of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "label")
                    .description("the label of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "description")
                    .description("the description of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "nulltype")
                    .description("the null type of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "datatype")
                    .description("the datatype of this column")
                    .datatype("word")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "structure")
                    .description("the structure of this column")
                    .datatype("trimmed_line")
                    .sql_type("TEXT")
                    .build()
                    .unwrap(),
            ],
        }
    }

    /// Return all the "meta" columns, starting with "_".
    pub fn meta(&self) -> Vec<&Column> {
        self.list.iter().filter(|col| col.is_meta()).collect()
    }

    /// Return all the data (non-meta) columns.
    pub fn data(&self) -> Vec<&Column> {
        self.list.iter().filter(|col| col.is_data()).collect()
    }

    /// Return the column for this table name and column name, or None.
    pub fn column(&self, table: &str, column: &str) -> Option<&Column> {
        self.iter()
            .filter(|c| c.table == table && c.column == column)
            .nth(0)
    }

    /// Given a column
    /// return a list of the columns that depend on this column
    /// because of their `from()` structure.
    pub fn direct_dependents(&self, column: &Column) -> Vec<&Column> {
        self.list
            .iter()
            .filter(|col| {
                let mut dependent = false;
                for structure in col.structure.iter() {
                    #[allow(irrefutable_let_patterns)]
                    if let Structure::From(t, c) = structure {
                        // If no table is specified, it means "from the same table".
                        let t = match t {
                            Some(t) => t,
                            None => &col.table,
                        };
                        if t == &column.table && c == &column.column {
                            dependent = true;
                            break;
                        };
                    }
                }
                dependent
            })
            .collect()
    }

    /// Given a column
    /// return a list of the columns that depend on this column
    /// because of their `from()` structure,
    /// and all their dependents recursively.
    pub fn dependents(&self, column: &Column) -> HashSet<&Column> {
        let mut dependents: HashSet<&Column> = HashSet::new();
        let direct_dependents = self.direct_dependents(column);
        for dependent in direct_dependents {
            dependents.insert(dependent);
            dependents.extend(self.dependents(dependent))
        }
        dependents
    }
}

/// Represents the special "column" table.
pub struct ColumnTable<'a> {
    // This table_name is "column" by default.
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> ColumnTable<'a> {
    /// Create a new instance of ColumnTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            table_name: "column".to_owned(),
            pool,
        }
    }

    // TODO: use rltbl_db to sanitize the name
    /// Use this name for the column table.
    /// The default is "column".
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
        let pkey_clause = match self.pool.kind() {
            DbKind::SQLite => "INTEGER PRIMARY KEY AUTOINCREMENT",
            DbKind::PostgreSQL => "SERIAL PRIMARY KEY",
        };

        format!(
            r#"CREATE TABLE "{}" (
              _id {pkey_clause},
              _order INTEGER UNIQUE,
              "table" TEXT,
              "column" TEXT,
              "label" TEXT,
              "description" TEXT,
              "nulltype" TEXT,
              "datatype" TEXT,
              "structure" TEXT
            )"#,
            self.table_name
        )
    }

    // TODO: replace this with self.pool.drop(self.name).
    /// Drop the column table from the database.
    pub async fn drop(&self) -> Result<()> {
        let sql = match self.pool.kind() {
            rltbl_db::core::DbKind::SQLite => {
                format!(r#"DROP TABLE IF EXISTS "{}""#, self.table_name)
            }
            rltbl_db::core::DbKind::PostgreSQL => {
                format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, self.table_name)
            }
        };
        self.pool.execute(&sql, ()).await?;
        Ok(())
    }

    /// Create the column table in the database
    /// and insert the built-in columns.
    pub async fn create(&self) -> Result<()> {
        self.pool.execute(&self.ddl(), ()).await?;
        let rows: Vec<JsonRow> = Columns::builtins()
            .iter()
            .map(|col| json!(col).as_object().unwrap().clone())
            .collect();
        let refs: Vec<&JsonRow> = rows.iter().collect();
        self.pool.insert(&self.table_name, &refs).await?;
        Ok(())
    }

    /// Insert these columns into the column table,
    /// returning the results.
    pub async fn add(&self, columns: &[&Column]) -> Result<Vec<Column>> {
        let rows: Vec<JsonRow> = columns
            .iter()
            .map(|dt| json!(dt).as_object().unwrap().clone())
            .collect();
        let refs: Vec<&JsonRow> = rows.iter().collect();
        let rows = self.pool.insert(&self.table_name, &refs).await?;
        let dts: Vec<Column> = rows
            .into_iter()
            // WARN: This silently ignores invalid columns.
            .filter_map(|row| serde_json::from_value::<Column>(json!(row)).ok())
            .collect();
        Ok(dts)
    }

    /// Get a SQL string for a query over actual columns,
    /// merged with column configuration.
    fn get_sql(&self, tables: &[&str]) -> Result<String> {
        // TODO: Validate table names.
        match self.pool.kind() {
            rltbl_db::core::DbKind::SQLite => {
                // TODO: Improve this
                let filter = if tables.len() > 0 {
                    format!(
                        "\n  AND main.name IN({})",
                        tables
                            .iter()
                            .map(|t| format!("'{t}'"))
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                } else {
                    String::new()
                };
                Ok(format!(
                    r#"
                    SELECT DISTINCT
                      main.name AS 'table',
                      pti.name AS 'column',
                      col.label AS 'label',
                      col.description AS 'description',
                      pti.type AS 'sql_type',
                      col.nulltype AS 'nulltype',
                      col.datatype AS 'datatype',
                      col.structure AS 'structure',
                      pti.pk AS 'primary_key',
                      (SELECT name = pti.name FROM pragma_index_info(pil.name)) AS 'unique'
                    FROM sqlite_master AS main
                    JOIN pragma_table_info(main.name) AS pti
                    LEFT JOIN pragma_index_list(main.name) AS pil
                    LEFT JOIN "{}" AS col ON col."table" = main.name AND col."column" = pti.name
                    WHERE main.type = 'table'
                      AND main.name != 'sqlite_sequence'{filter}
                    ORDER BY main.name;"#,
                    self.table_name
                ))
            }
            rltbl_db::core::DbKind::PostgreSQL => {
                todo!()
            }
        }
    }

    /// Get all the columns for this database.
    /// This merges the actual columns with the content of the column table.
    pub async fn get_all(&self) -> Result<Columns> {
        self.get(&[]).await
    }

    /// Get all the columns for this database.
    /// This merges the actual columns with the content of the column table.
    pub async fn get(&self, tables: &[&str]) -> Result<Columns> {
        let sql = self.get_sql(tables)?;
        let rows = self.pool.query(&sql, ()).await?;
        let list = rows
            .iter()
            .filter_map(|row: &JsonRow| {
                let row: JsonRow = row
                    .iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                serde_json::from_value(json!(row)).ok()
            })
            .collect::<Vec<_>>();
        Ok(Columns { list })
    }

    /// Get columns for these tables from the "column" table,
    /// whether or not they are actually in the database.
    pub async fn get_configured(&self, tables: &[&str]) -> Result<Columns> {
        let sql = format!(
            r#"
              SELECT
                "table",
                "column",
                "label",
                "description",
                "nulltype",
                "datatype",
                "structure"
              FROM "{}"
              WHERE "table" IN({})
            "#,
            self.table_name,
            tables
                .iter()
                .map(|t| format!("'{t}'"))
                .collect::<Vec<String>>()
                .join(", ")
        );
        let rows = self.pool.query(&sql, ()).await?;
        let list = rows
            .iter()
            .filter_map(|row: &JsonRow| {
                let row: JsonRow = row
                    .iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                serde_json::from_value(json!(row)).ok()
            })
            .collect::<Vec<_>>();
        Ok(Columns { list })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;

    #[tokio::test]
    async fn test_sql_type() {
        let datatypes = Datatypes::builtins();

        let column = ColumnBuilder::new("foo", "bar").build().unwrap();
        assert_eq!(column.sql_type(&datatypes), "TEXT");

        let column = ColumnBuilder::new("foo", "bar")
            .datatype("integer")
            .build()
            .unwrap();
        assert_eq!(column.sql_type(&datatypes), "INTEGER");
    }

    #[tokio::test]
    async fn test_create() {
        let pool = AnyPool::connect("build/test_column_create.db")
            .await
            .expect("connect to SQLite");
        let table = ColumnTable::connect(&pool);
        table.drop().await.expect("delete column table");
        table.create().await.expect("create column table");
        let columns = table.get_all().await.expect("get columns");
        assert_eq!(
            columns.data(),
            Columns::builtins().iter().collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_dependent() {
        let a = ColumnBuilder::new("foo", "a").build().unwrap();
        // column b depends on column a from the same "foo" table.
        let b = ColumnBuilder::new("foo", "b")
            .structure("from(a)")
            .build()
            .unwrap();
        // column c depends on column a from the "bar" table.
        let c = ColumnBuilder::new("bar", "c")
            .structure("from(foo.b)")
            .build()
            .unwrap();
        let columns = Columns {
            list: vec![a.clone(), b.clone(), c.clone()],
        };
        assert_eq!(columns.direct_dependents(&a), vec![&b]);
        assert_eq!(columns.dependents(&a), HashSet::from([&b, &c]));
        assert_eq!(columns.direct_dependents(&b), vec![&c]);
        assert_eq!(columns.dependents(&b), HashSet::from([&c]));
        assert_eq!(columns.direct_dependents(&c), Vec::<&Column>::new());
        assert_eq!(columns.dependents(&c), HashSet::<&Column>::new());
    }
}
