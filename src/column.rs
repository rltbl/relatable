//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[column](crate::column)).

use std::{
    fmt,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use crate as rltbl;
use rltbl::{
    core::RelatableError,
    datatype::Datatypes,
    sql::{self, DbTransaction},
    structure::Structure,
    table::Table,
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
    Builder, Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord,
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
    pub structure: String,
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
    // TODO: replace this
    /// Get the columns, either from the same or from another table, that depend on this column,
    /// using the given transaction
    pub fn _get_dependent_columns(&self, tx: &mut DbTransaction<'_>) -> Result<Vec<Self>> {
        tracing::trace!("Column::_get_dependent_columns({self:?}, tx)");

        if !Table::_table_exists("column", tx)? {
            tracing::debug!("No column table found");
            return Ok(vec![]);
        }

        tracing::debug!(
            "Looking through column table for dependent columns of '{}.{}'",
            self.table,
            self.column
        );

        let sql = format!(
            r#"SELECT * FROM "column" WHERE "structure" {is_not} NULL"#,
            is_not = sql::is_not_clause(&tx.kind())
        );
        let mut dependent_columns: Vec<Column> = vec![];
        for row in &tx.query(&sql, None)? {
            let dependent_table = Table::_get_table(&row.get_string("table")?, tx)?;
            let Structure::From(structure_table, structure_column) =
                Structure::from_str(&row.get_string("structure")?)?;
            let structure_table = structure_table.unwrap_or(dependent_table.name.to_string());
            if structure_table == self.table && structure_column == self.column {
                let dependent_column = row.get_string("column")?;
                let dependent_column = match dependent_table.columns.get(&dependent_column) {
                    Some(col) => col.clone(),
                    None => {
                        return Err(RelatableError::DataError(format!(
                            "No column found: '{dependent_column}'"
                        ))
                        .into());
                    }
                };
                let mut indirect_deps = dependent_column._get_dependent_columns(tx)?;
                dependent_columns.push(dependent_column);
                dependent_columns.append(&mut indirect_deps);
            }
        }
        tracing::debug!(
            "Column '{}.{}' has the following dependent columns: {dependent_columns:#?}",
            self.table,
            self.column
        );
        Ok(dependent_columns)
    }

    /// Get the SQL type for this column according to its column,
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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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

impl Columns {
    // Returns an [IndexMap] representing all of the built-in columns, indexed by column name
    pub fn builtins() -> Self {
        Columns {
            list: vec![
                ColumnBuilder::default()
                    .table("table")
                    .column("table")
                    .description("the table for this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "column")
                    .description("the name of this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "label")
                    .description("the label of this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "description")
                    .description("the description of this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "nulltype")
                    .description("the null type of this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "datatype")
                    .description("the datatype of this column")
                    .build()
                    .unwrap(),
                ColumnBuilder::new("column", "structure")
                    .description("the structure of this column")
                    .build()
                    .unwrap(),
            ],
        }
    }
}

/// Represents the special "datatype" table.
pub struct ColumnTable<'a> {
    // This table_name is "datatype" by default.
    table_name: String,
    pool: &'a AnyPool,
}

impl<'a> ColumnTable<'a> {
    /// Create a new instance of ColumnTable from an AnyPool.
    pub fn connect<'b>(pool: &'b AnyPool) -> Self
    where
        'b: 'a,
    {
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
            .map(|dt| json!(dt).as_object().unwrap().clone())
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

    /// Get all the columns for this database.
    /// This merges the actual columns with the content of the column table.
    pub async fn get(&self) -> Result<Columns> {
        let sql = match self.pool.kind() {
            rltbl_db::core::DbKind::SQLite => {
                // format!(
                //     r#"
                //     SELECT
                //       main.name AS 'table',
                //       pti.name AS 'column',
                //       col.label AS 'label',
                //       col.description AS 'description',
                //       pti.type AS 'sql_type',
                //       col.nulltype AS 'nulltype',
                //       col.datatype AS 'datatype',
                //       col.structure AS 'structure',
                //       pti.pk AS 'primary_key',
                //       (SELECT name = pti.name FROM pragma_index_info(pil.name)) AS 'unique'
                //     FROM sqlite_master AS main
                //     JOIN pragma_table_info(main.name) AS pti
                //     JOIN pragma_index_list(main.name) AS pil
                //     LEFT JOIN "{}" AS col ON col."table" = main.name AND col."column" = pti.name
                //     WHERE main.type = 'table'
                //     ORDER BY main.name;"#,
                //     self.table_name
                // )
                format!(
                    r#"SELECT "table", "column", "label", "description", "nulltype", "datatype", "structure"
                    FROM "{}"
                    ORDER BY _order;"#,
                    self.table_name
                )
            }
            rltbl_db::core::DbKind::PostgreSQL => {
                todo!()
            }
        };
        let rows = self.pool.query(&sql, ()).await?;
        let list = rows
            .iter()
            // .map(|row| match serde_json::from_value(json!(row)) {
            //     Ok(col) => Some(col),
            //     Err(err) => {
            //         println!("{row:?} {err}");
            //         None
            //     }
            // })
            // .filter_map(|result| result)
            // .filter_map(|row| serde_json::from_value(json!(row)).ok())
            .filter_map(|row: &JsonRow| {
                let row: JsonRow = row
                    .iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                // serde_json::from_value(json!(row)).ok()
                match serde_json::from_value(json!(row)) {
                    Ok(col) => Some(col),
                    Err(err) => {
                        println!("{row:?} {err}");
                        None
                    }
                }
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
        // let pool = AnyPool::connect(":memory:")
        let pool = AnyPool::connect("test_create.db")
            .await
            .expect("connect to SQLite");
        let table = ColumnTable::connect(&pool);
        table.drop().await.expect("delete column table");
        table.create().await.expect("create column table");
        let columns = table.get().await.expect("get columns");
        assert_eq!(columns, Columns::builtins());
    }
}
