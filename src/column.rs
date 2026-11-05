//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[column](crate::column)).

use crate::{
    self as rltbl,
    data_table::{DataTable, DataTableBuilder},
    schema::Schema,
    tsv_table::{read_table, UserRow, UserRows},
};
use rltbl::{
    core::{RowID, RowOrder},
    datatype::Datatypes,
    structure::{Structure, Structures},
    tsv_table::TsvTable,
};
use rltbl_db::{row, to_row, Row as DbRow, Syntax as DbSyntax};

use anyhow::Result;
use derive_builder::Builder;
use indexmap::IndexMap;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    ops::{Deref, DerefMut},
};

#[derive(Clone, Debug, Default)]
pub struct SqlColumn {
    pub column: String,
    pub sql_type: String,
    pub sql_default: String,
    pub primary_key: bool,
    pub unique: bool,
    pub not_null: bool,
    pub references: Vec<String>,
}

impl SqlColumn {
    pub fn ddl(&self, syntax: &dyn DbSyntax) -> String {
        let mut parts = vec![format!(r#""{}""#, self.column)];
        let sql_type = match self.sql_type.to_uppercase().as_str() {
            "SERIAL" => match syntax.name() {
                "sqlite" => "INTEGER".to_string(),
                _ => self.sql_type.to_string(),
            },
            _ => self.sql_type.to_string(),
        };
        parts.push(sql_type);
        if self.primary_key {
            match (syntax.name(), self.sql_type.to_uppercase().as_str()) {
                ("sqlite", "SERIAL") => parts.push("PRIMARY KEY AUTOINCREMENT".to_string()),
                _ => parts.push("PRIMARY KEY".to_string()),
            }
        } else if self.unique {
            parts.push("UNIQUE".to_string());
        }
        if self.not_null {
            parts.push("NOT NULL".to_string());
        }
        if self.references.len() > 0 {
            parts.push(format!(
                r#"REFERENCES "{table}"({columns})"#,
                table = self.references.first().unwrap(),
                columns = self
                    .references
                    .iter()
                    .skip(1)
                    .map(|c| format!(r#""{c}""#))
                    .join(", ")
            ));
        }
        if self.sql_default != "" {
            parts.push(format!("DEFAULT {}", self.sql_default));
        }
        parts.join(" ")
    }
}

/// Represents a column from some table
#[derive(Builder, Clone, Debug, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(default)]
#[builder(default, setter(into))]
pub struct Column {
    #[serde(rename = "_id")]
    pub id: RowID,
    #[serde(rename = "_order")]
    pub order: RowOrder,
    pub table: String,
    pub column: String,
    pub label: String,
    pub description: String,
    pub sql_type: String,
    pub sql_default: String,
    pub nulltype: String,
    pub datatype: String,
    pub structure: Structures,
    pub primary_key: bool,
    pub unique: bool,
    pub not_null: bool,
    pub references: Vec<String>,
}

impl Default for Column {
    fn default() -> Self {
        Self {
            id: Default::default(),
            order: Default::default(),
            table: Default::default(),
            column: Default::default(),
            label: Default::default(),
            description: Default::default(),
            sql_type: Default::default(),
            sql_default: Default::default(),
            nulltype: Default::default(),
            datatype: "text".to_string(),
            structure: Default::default(),
            primary_key: Default::default(),
            unique: Default::default(),
            not_null: Default::default(),
            references: Default::default(),
        }
    }
}

impl Column {
    pub fn id(&self) -> String {
        if self.id != 0 {
            format!("c{}", self.id)
        } else {
            self.column.clone()
        }
    }

    /// True if this is a "meta" column.
    pub fn is_meta(&self) -> bool {
        self.column.starts_with("_")
    }

    /// True if this is a "meta" column.
    pub fn is_data(&self) -> bool {
        !self.is_meta()
    }

    /// True if there are any database constraints on this column.
    pub fn is_constrained(&self) -> bool {
        if self.primary_key == true {
            return true;
        }
        if self.unique == true {
            return true;
        }
        if self.not_null == true {
            return true;
        }
        if self.references.len() > 0 {
            return true;
        }
        false
    }

    /// Get the SQL type for this column according to its datatype,
    /// or the first column ancestor with a sql_type,
    /// or just "text".
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
            None => "text".to_owned(),
        }
    }

    pub fn ddl(&self, syntax: &dyn DbSyntax) -> String {
        let sql_column: SqlColumn = self.clone().into();
        sql_column.ddl(syntax)
    }
}

impl Into<SqlColumn> for Column {
    fn into(self) -> SqlColumn {
        SqlColumn {
            column: self.id(),
            sql_type: self.sql_type.clone(),
            sql_default: self.sql_default.clone(),
            primary_key: self.primary_key.clone(),
            unique: self.unique.clone(),
            not_null: self.not_null.clone(),
            references: self.references.clone(),
        }
    }
}

impl Into<UserRow> for Column {
    fn into(self) -> UserRow {
        let db_row = row! {
            "table" => self.table,
            "column" => self.column,
            "label" => self.label,
            "nulltype" => self.nulltype,
            "datatype" => self.datatype,
            "structure" => self.structure.to_string(),
            "description" => self.description,
        };
        db_row.into()
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

/// A list of columns and their structure.
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

impl From<Vec<Column>> for Columns {
    fn from(value: Vec<Column>) -> Self {
        Columns { list: value }
    }
}

impl Into<IndexMap<String, Column>> for Columns {
    fn into(self) -> IndexMap<String, Column> {
        self.list
            .into_iter()
            .map(|col| (col.column.to_string(), col))
            .collect()
    }
}

impl Columns {
    pub fn names(&self) -> Vec<String> {
        self.list.iter().map(|c| c.column.to_string()).collect()
    }

    pub fn ids(&self) -> Vec<String> {
        self.list.iter().map(|c| c.id().to_string()).collect()
    }

    pub fn ddl(&self, syntax: &dyn DbSyntax, table_name: &str) -> String {
        let cols = self.list.iter().map(|col| col.ddl(syntax)).join(",\n  ");
        format!(
            r#"CREATE TABLE "{table_name}" (
  {cols}
);"#
        )
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

    /// Given a set of Datatypes, update the sql_type of each column, in place.
    pub fn sql_types(&mut self, datatypes: &Datatypes) {
        for column in self.list.iter_mut() {
            column.sql_type = column.sql_type(datatypes);
        }
    }
}

/// Represents the special "column" table.
#[derive(Debug)]
pub struct ColumnTable<'a> {
    table: DataTable<'a>,
}

impl<'a> Deref for ColumnTable<'a> {
    type Target = DataTable<'a>;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl<'a> ColumnTable<'a> {
    pub fn from(table: DataTable<'a>) -> Self {
        Self { table }
    }

    /// Create the default column table.
    pub async fn init(&self) -> Result<()> {
        let path = "src/schema/column.tsv";
        let mut builder = DataTableBuilder::from_path(path);
        builder.table_id(2);
        builder.columns(Schema::default().columns("column"));

        for column in Schema::default().columns.iter() {
            builder.to_row(&column)?;
        }

        builder.create_table(&self.pool).await?;

        Ok(())
    }

    /// Read a TSV file and merge it into this table.
    /// If the table name and column name match, use it, updating if required.
    /// Otherwise create the column.
    pub async fn merge_tsv(&self, path: &str) -> Result<()> {
        let rows = read_table(path)?;
        self.merge_rows(rows).await
    }

    /// Merge a vector of rows into this table.
    /// If the table name and column name match, use it, updating if required.
    /// Otherwise create the column.
    pub async fn merge_rows(&self, rows: UserRows) -> Result<()> {
        let list = self.list().await?;
        for row in rows.rows {
            let table = row.get("table").expect("table column").to_string();
            let column = row.get("column").expect("column column").to_string();
            let mut found = false;
            for col in &list {
                if table == col.table && column == col.column {
                    tracing::debug!("Found matching column: {table} {column}");
                    found = true;
                    let old_row: UserRow = col.clone().into();
                    let update_row = row.without(&old_row);
                    if !update_row.is_empty() {
                        tracing::debug!("Updating column: {update_row:?}");
                        let pair = (col.id, update_row);
                        self.update(&[pair]).await?;
                    }
                }
            }
            if !found {
                tracing::debug!("Creating new column: {table} {column}");
                let rows = UserRows { rows: vec![row] };
                self.append(rows).await?;
            }
        }
        Ok(())
    }

    /// Insert these columns into the "column" table,
    /// returning the results.
    pub async fn add(&self, columns: &[&Column]) -> Result<Vec<Column>> {
        // println!("column add {columns:?}");
        let rows: Vec<DbRow> = columns
            .iter()
            .map(|c| to_row(c))
            .collect::<Result<Vec<DbRow>, _>>()?;
        let rows = rows.into_iter().map(|row| row.into()).collect();
        let rows = UserRows { rows };
        self.append(rows).await?;
        let column_names: BTreeMap<(String, String), String> = columns
            .iter()
            .map(|c| {
                (
                    (c.table.to_string(), c.column.to_string()),
                    c.sql_type.to_string(),
                )
            })
            .collect();
        let table_names: Vec<&str> = column_names.iter().map(|((t, _), _)| t.as_str()).collect();
        let new_columns: Vec<Column> = self
            .get_configured(&table_names)
            .await?
            .iter()
            // .inspect(|c| println!("COL {c:?}"))
            .filter_map(|c| {
                column_names
                    .get(&(c.table.clone(), c.column.clone()))
                    .and_then(|sql_type| {
                        let mut c = c.clone();
                        c.sql_type = sql_type.to_string();
                        Some(c)
                    })
            })
            .collect();
        Ok(new_columns)
    }

    /// Get a SQL string for a query over actual columns,
    /// merged with column configuration.
    fn get_sql(&self, tables: &[&str]) -> Result<String> {
        // TODO: Validate table names.
        match self.pool.syntax().name() {
            "sqlite" => {
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
                // TODO: fix primary_key and unique
                Ok(format!(
                    r#"
                    SELECT DISTINCT
                      main.name AS "table",
                      pti.name AS "column",
                      col.label AS "label",
                      col.description AS "description",
                      LOWER(pti.type) AS "sql_type",
                      col.nulltype AS "nulltype",
                      col.datatype AS "datatype",
                      col.structure AS "structure"
                      --pti.pk AS "primary_key",
                      --(SELECT name = pti.name FROM pragma_index_info(pil.name)) AS "unique"
                    FROM sqlite_master AS main
                    JOIN pragma_table_info(main.name) AS pti
                    LEFT JOIN pragma_index_list(main.name) AS pil
                    LEFT JOIN "{}" AS col ON col."table" = main.name AND col."column" = pti.name
                    WHERE main.type = 'table'
                      AND main.name != 'sqlite_sequence'{filter}
                      AND main.name NOT LIKE '%_alt'
                    ORDER BY main.name;"#,
                    self.name
                ))
            }
            "postgres" => {
                // TODO: Improve this
                let filter = if tables.len() > 0 {
                    format!(
                        "\n  AND main.table_name IN({})",
                        tables
                            .iter()
                            .map(|t| format!("'{t}'"))
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                } else {
                    String::new()
                };
                // TODO: finish primary_key and unique
                Ok(format!(
                    r#"SELECT
                         main.table_name::TEXT AS "table",
                         main.column_name::TEXT AS "column",
                         col.label AS "label",
                         col.description AS "description",
                         LOWER(main.data_type::TEXT) AS "sql_type",
                         col.nulltype AS "nulltype",
                         col.datatype AS "datatype",
                         col.structure AS "structure"
                       FROM "information_schema"."columns" AS main
                       LEFT JOIN "{}" AS col ON col."table" = main.table_name AND col."column" = main.column_name
                       WHERE
                         main.table_schema IN (
                           SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                           FROM "pg_settings"
                           WHERE "name" = 'search_path'
                         ) AND main.table_name NOT LIKE '%_alt'{filter}
                       ORDER BY main.ordinal_position;"#,
                    self.name
                ))
            }
            _ => unimplemented!(),
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
        let list: Vec<Column> = self
            .pool
            .query(&sql, ())
            .await?
            .remove_nulls()
            .try_into_vec()?;
        Ok(Columns { list })
    }

    /// Get columns for these tables from the "column" table,
    /// whether or not they are actually in the database.
    pub async fn get_configured(&self, tables: &[&str]) -> Result<Columns> {
        let sql = format!(
            r#"
              SELECT
                "_id",
                "_order",
                "table",
                "column",
                "label",
                "description",
                "nulltype",
                "datatype",
                "structure"
              FROM "column"
              WHERE "table" IN({})
            "#,
            tables
                .iter()
                .map(|t| format!("'{t}'"))
                .collect::<Vec<String>>()
                .join(", ")
        );
        let list: Vec<Column> = self
            .pool
            .query(&sql, ())
            .await?
            .remove_nulls()
            .try_into_vec()?;
        Ok(Columns { list })
    }

    /// List the columns in the "column" table.
    pub async fn list(&self) -> Result<Vec<Column>> {
        let sql = r#"SELECT * FROM "column_union" ORDER BY _order;"#;
        self.pool()
            .query(sql, ())
            .await?
            .remove_nulls()
            .try_into_vec()
            .map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RelatableBuilder;
    use pretty_assertions::assert_eq;
    use rltbl_db::row;

    #[tokio::test]
    async fn test_init() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_column_init")
            .init(true)
            .await?;

        let count: u64 = rltbl
            .pool
            .query(r#"SELECT count(1) FROM "column""#, ())
            .await?
            .try_into_value()?;
        assert_eq!(count, 15);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_column_merge() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_column_merge")
            .init(true)
            .await?;
        let rows: UserRows = vec![row! {
            "table" => "island",
        }]
        .into();
        rltbl.table_table().await?.append(rows).await?;
        let column_table = rltbl.column_table().await?;

        let rows: UserRows = vec![
            row! {
                "table" => "table",
                "column" => "table",
            },
            row! {
                "table" => "table",
                "column" => "path",
                "nulltype" => "empty",
            },
            row! {
                "table" => "island",
                "column" => "island_id",
            },
            row! {
                "table" => "island",
                "column" => "island",
            },
        ]
        .into();

        let mut expected = column_table.list().await?;
        expected[1].nulltype = "empty".to_string();
        expected.push(Column {
            id: 16,
            order: 16000,
            table: "island".to_string(),
            column: "island_id".to_string(),
            ..Default::default()
        });
        expected.push(Column {
            id: 17,
            order: 17000,
            table: "island".to_string(),
            column: "island".to_string(),
            ..Default::default()
        });

        column_table.merge_rows(rows).await?;
        let actual = column_table.list().await?;

        assert_eq!(expected, actual);

        // Check that columns were configured.
        let expected = vec!["island_id", "island"];
        let columns = column_table.get_configured(&["island"]).await?;
        let actual: Vec<&str> = columns.iter().map(|c| c.column.as_str()).collect();
        assert_eq!(expected, actual);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_sql_type() -> Result<()> {
        let datatypes = Datatypes::builtins();

        let column = ColumnBuilder::new("foo", "bar").build()?;
        assert_eq!(column.sql_type(&datatypes), "TEXT");

        let column = ColumnBuilder::new("foo", "bar")
            .datatype("integer")
            .build()?;
        assert_eq!(column.sql_type(&datatypes), "INTEGER");

        Ok(())
    }

    #[tokio::test]
    async fn test_dependent() -> Result<()> {
        let a = ColumnBuilder::new("foo", "a").build()?;
        // column b depends on column a from the same "foo" table.
        let b = ColumnBuilder::new("foo", "b")
            .structure("from(a)")
            .build()?;
        // column c depends on column a from the "bar" table.
        let c = ColumnBuilder::new("bar", "c")
            .structure("from(foo.b)")
            .build()?;
        let columns = Columns {
            list: vec![a.clone(), b.clone(), c.clone()],
        };
        assert_eq!(columns.direct_dependents(&a), vec![&b]);
        assert_eq!(columns.dependents(&a), HashSet::from([&b, &c]));
        assert_eq!(columns.direct_dependents(&b), vec![&c]);
        assert_eq!(columns.dependents(&b), HashSet::from([&c]));
        assert_eq!(columns.direct_dependents(&c), Vec::<&Column>::new());
        assert_eq!(columns.dependents(&c), HashSet::<&Column>::new());

        Ok(())
    }
}
