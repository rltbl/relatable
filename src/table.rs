//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use std::{collections::BTreeSet, ops::Deref, path::Path};

use crate::{
    self as rltbl,
    data_table::DataTableBuilder,
    schema::Schema,
    tsv_table::{UserRow, UserRows},
};
use rltbl::{
    column::Columns,
    core::{Relatable, RowID, RowOrder},
    data_table::DataTable,
    tsv_table::TsvTable,
};
use rltbl_db::{to_row, values, AnyPool, Row as DbRow};

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

// impl TryInto<UserRows> for Vec<Table> {
//     type Error = RelatableError;

//     fn try_into(self) -> std::result::Result<UserRows, Self::Error> {
//         let rows = self
//             .iter()
//             .map(|t| rltbl_db::serde::to_db_row(t))
//             .collect::<Result<Vec<DbRow>, DbError>>()
//             .map_err(|e| {
//                 RelatableError::DataError(format!(
//                     "Vec<Table> struct could not be converted into UserRows: {e}"
//                 ))
//             })?;
//         Ok(UserRows {
//             rows: rows.into_iter().map(|r| UserRow { map: r.map }).collect(),
//         })
//     }
// }

impl From<Vec<Table>> for UserRows {
    fn from(value: Vec<Table>) -> Self {
        let rows: Vec<UserRow> = value
            .iter()
            .map(|t| to_row(t).expect("convert Table to Row"))
            .map(|r| UserRow { map: r.map })
            .collect();
        UserRows { rows }
    }
}

/// Represents the special "table" table.
pub struct TableTable<'a> {
    table: DataTable<'a>,
}

impl<'a> Deref for TableTable<'a> {
    type Target = DataTable<'a>;

    fn deref(&self) -> &Self::Target {
        &self.table
    }
}

impl<'a> TableTable<'a> {
    pub fn from(table: DataTable<'a>) -> Self {
        Self { table }
    }

    /// Create the default table table.
    pub async fn init(&self) -> Result<()> {
        let path = "src/schema/table.tsv";
        let mut builder = DataTableBuilder::from_path(path);
        builder.table_id(1);
        builder.columns(Schema::default().columns("table"));
        builder.row(values!["table", "src/schema/table.tsv"])?;
        builder.row(values!["column", "src/schema/column.tsv"])?;
        builder.row(values!["datatype", "src/schema/datatype.tsv"])?;
        builder.create_table(&self.pool).await?;
        Ok(())
    }

    async fn create_tables(&self, tables: Vec<Table>) -> Result<Vec<Table>> {
        for table in &tables {
            // println!("inserting table {table:?}");
            DataTableBuilder::from(table.clone())
                .create_table(self.pool)
                .await?;
        }
        Ok(tables)
    }

    /// Insert UserRows or Tables into this table, returning valid tables inserted.
    pub async fn insert(&self, after_id: RowID, rows: impl Into<UserRows>) -> Result<Vec<Table>> {
        let rows: UserRows = rows.into();
        let tsv_rows = self.table.insert(after_id, rows).await?;
        let tables = self.select_by_id(&tsv_rows.ids()).await?;
        self.create_tables(tables).await
    }

    /// Append UserRows or Tables into this table, returning valid tables inserted.
    pub async fn append(&self, rows: impl Into<UserRows>) -> Result<Vec<Table>> {
        let rows: UserRows = rows.into();
        let tsv_rows = self.table.append(rows).await?;
        let tables = self.select_by_id(&tsv_rows.ids()).await?;
        self.create_tables(tables).await
    }

    pub async fn select_by_id(&self, ids: &[RowID]) -> Result<Vec<Table>> {
        let ids = ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let sql = format!(r#"SELECT * FROM "table_union" WHERE _id IN ({ids}) ORDER BY _order;"#);
        self.pool()
            .query(&sql, ())
            .await?
            .remove_nulls()
            .try_into_vec()
            .map_err(|e| e.into())
    }

    /// Insert these tables into the "table" table,
    /// returning the results.
    pub async fn extend(&self, tables: &[&Table]) -> Result<Vec<Table>> {
        let rows = tables
            .iter()
            .map(|t| to_row(t))
            .collect::<Result<Vec<DbRow>, _>>()?;
        let rows = rows.into_iter().map(|row| row.into()).collect();
        let rows = UserRows { rows };
        self.table.append(rows).await?;
        let table_names: BTreeSet<String> = tables.iter().map(|d| d.name.clone()).collect();
        let new_tables: Vec<Table> = self
            .list()
            .await?
            .into_iter()
            .filter(|t| table_names.contains(&t.name))
            .collect();
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
        let sql = r#"SELECT * FROM "table_union" ORDER BY _order;"#;
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
    use pretty_assertions::assert_eq;

    #[tokio::test]
    async fn test_init() -> Result<()> {
        let rltbl = Relatable::test("test_table_init", false).await?;

        let count: u64 = rltbl
            .pool
            .query(r#"SELECT count(1) FROM "table""#, ())
            .await?
            .try_into_value()?;
        assert_eq!(count, 3);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_table_insert() -> Result<()> {
        let rltbl = Relatable::test("test_table_insert", false).await?;
        let table_table = rltbl.table_table().await?;

        let table = Table::from_path("foo/bar.tsv");
        let result = table_table.insert(0, vec![table]).await?;
        assert_eq!(
            vec![Table {
                name: "bar".to_string(),
                id: 4,
                order: 500,
                path: "foo/bar.tsv".to_string(),
                view: String::new(),
                editable: true,
                has_meta: true
            }],
            result
        );

        assert!(rltbl.get_table("bar").await.is_ok(), "created bar table");

        // assert!(false, "DONE");
        rltbl.drop_test().await
    }
}
