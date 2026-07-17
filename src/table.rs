//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use std::path::Path;

use crate as rltbl;
use rltbl::{
    column::{ColumnBuilder, Columns},
    core::{Relatable, RowID, RowOrder},
    data_table::DataTable,
    tsv_table::TsvTable,
};
use rltbl_db::{any::AnyPool, core::DbQuery, serde::to_db_row};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Table {
    /// The name of the table
    #[serde(rename = "table")]
    pub name: String,
    #[serde(rename = "_id")]
    pub id: RowID,
    #[serde(rename = "_order")]
    pub order: RowOrder,
    /// The optional path for loading and saving the table
    pub path: String,
    /// The name of the view (blank if there is none) to be used when querying the table
    pub view: String,
    /// The id of the most recent change to this table.
    pub editable: bool,
    /// Indicates whether the table has the _id and _order meta columns enabled:
    pub has_meta: bool,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            name: Default::default(),
            id: Default::default(),
            order: Default::default(),
            path: Default::default(),
            view: Default::default(),
            editable: true,
            has_meta: true,
        }
    }
}

impl Table {
    pub fn from_path(path: &str) -> Self {
        if !path.ends_with(".tsv") {
            todo!("Only TSV is currently supported");
        }
        let name = Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches(".tsv")
            .to_string();
        let path = path.to_string();
        Self {
            name,
            path,
            ..Default::default()
        }
    }

    /// Returns the row id that comes before the given row in the given table, using the given
    /// transaction.
    pub async fn get_previous_row_id(
        table: &str,
        row_id: RowID,
        rltbl: &Relatable,
    ) -> Result<RowID> {
        let sql = format!(
            r#"SELECT "_id" FROM "{table}_union" WHERE "_order" < (SELECT _order FROM "{table}_union" WHERE _id = $1)
               ORDER BY "_order" DESC LIMIT 1"#,
        );
        match rltbl.pool.query(&sql, [row_id]).await?.value() {
            Ok(value) => Ok(value.as_i32().unwrap_or_default()),
            Err(_) => Ok(0),
        }
    }

    pub fn connect<'a>(&self, pool: &'a AnyPool) -> DataTable<'a> {
        DataTable {
            name: self.name.clone(),
            id: self.id.clone(),
            pool,
            columns: Columns::default(),
        }
    }
}

/// Represents the special "table" table.
pub struct TableTable<'a> {
    name: String,
    pool: &'a AnyPool,
}

impl<'a> TsvTable for TableTable<'a> {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn id(&self) -> String {
        self.name.clone()
    }

    fn pool(&self) -> &AnyPool {
        self.pool
    }

    fn columns(&self) -> Columns {
        // TODO: both should be unique
        vec![
            ColumnBuilder::new(&self.name, "table")
                .sql_type("TEXT")
                .build()
                .unwrap(),
            ColumnBuilder::new(&self.name, "path")
                .sql_type("TEXT")
                .build()
                .unwrap(),
        ]
        .into()
    }
}

impl<'a> TableTable<'a> {
    /// Create a new instance of UserTable from an AnyPool.
    pub fn connect(pool: &'a AnyPool) -> Self {
        Self {
            name: "table".to_owned(),
            pool,
        }
    }

    /// Create tables and fill with default rows.
    pub async fn init(&self) -> Result<()> {
        self.create().await?;
        let tables = vec![
            Table {
                name: self.name().to_string(),
                path: "src/schema/table.tsv".to_string(),
                ..Default::default()
            },
            Table {
                name: "column".to_string(),
                path: "src/schema/column.tsv".to_string(),
                ..Default::default()
            },
            Table {
                name: "datatype".to_string(),
                path: "src/schema/datatype.tsv".to_string(),
                ..Default::default()
            },
        ];
        let refs: Vec<&Table> = tables.iter().collect();
        self.extend(&refs).await?;
        Ok(())
    }

    /// Insert these tables into the "table" table,
    /// returning the results.
    pub async fn extend(&self, tables: &[&Table]) -> Result<Vec<Table>> {
        let rows = tables
            .iter()
            .map(|table| to_db_row(table))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = self.append_regular(rows).await?;
        let new_tables: Vec<Table> = rows.remove_nulls().try_into_vec()?;
        Ok(new_tables)
    }

    /// Insert one tables into the "table" table,
    /// returning the result.
    pub async fn add(&self, table: Table) -> Result<Table> {
        let mut tables = self.extend(&[&table]).await?;
        let table = tables.pop().expect("Added one table");
        Ok(table)
    }

    pub async fn list(&self) -> Result<Vec<Table>> {
        // TODO: Handle the alternate table.
        let sql = r#"SELECT * FROM "table" ORDER BY _order;"#;
        self.pool()
            .query(sql, ())
            .await?
            .try_into_vec()
            .map_err(|e| e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use rltbl_db::any::AnyPool;

    #[tokio::test]
    async fn test_ddl() {
        let pool = AnyPool::connect(":memory:").await.unwrap();
        let table = TableTable::connect(&pool);
        assert_eq!(
            r#"CREATE TABLE "table" (
  "_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "_order" BIGINT UNIQUE,
  "table" TEXT,
  "path" TEXT
);
CREATE TABLE "table_alt" (
  "_id" INTEGER PRIMARY KEY,
  "_order" BIGINT UNIQUE,
  "_deleted" BOOL,
  "table" TEXT,
  "path" TEXT
);
CREATE VIEW "table_union" AS
  SELECT "_id", "_order", FALSE AS "_deleted", "table", "path" FROM "table"
  UNION ALL
  SELECT "_id", "_order", "_deleted", "table", "path" FROM "table_alt";
CREATE VIEW "table_text" AS
  SELECT "_id", "_order", "_deleted", "table", "path"
  FROM "table_union"
  WHERE "_deleted" = FALSE;"#,
            table.ddl()
        )
    }

    #[tokio::test]
    async fn test_init() -> Result<()> {
        let rltbl = Relatable::test("test_table_init", false).await?;

        let count: u64 = rltbl
            .pool
            .query(r#"SELECT count(1) FROM "table""#, ())
            .await?
            .try_into()?;
        assert_eq!(count, 3);

        rltbl.drop_test().await
    }
}
