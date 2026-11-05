use crate::{self as rltbl, column::Column, core::ColumnID};
use csv::ReaderBuilder;
use indexmap::IndexMap;
use rltbl::{
    column::{Columns, SqlColumn},
    core::{RowID, RowOrder, ID_SQL_TYPE, NEW_ORDER_MULTIPLIER, ORDER_SQL_TYPE},
};
use rltbl_db::{row as db_row, AnyPool, Row as DbRow, Rows as DbRows, Value as DbValue};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs::File,
    iter::zip,
    ops::{Deref, DerefMut},
};

pub type ColumnName = String;

/// A row as the user sees it, using column names and values with mixed types.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserRow {
    pub map: IndexMap<ColumnName, DbValue>,
}

impl UserRow {
    /// Given a target UserRow,
    /// return a copy of this UserRow
    /// without any key-value pairs that match the target.
    /// This is useful for making minimal updates to a database row.
    pub fn without(&self, without: &UserRow) -> UserRow {
        let mut new_row = UserRow::default();
        for (key, value) in &self.map {
            if without.contains_key(key) && without.get(key).unwrap() == value {
                // ignore this key-value pair
            } else {
                new_row.insert(key.clone(), value.clone());
            }
        }
        new_row
    }
}

impl Deref for UserRow {
    type Target = IndexMap<ColumnName, DbValue>;

    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl DerefMut for UserRow {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

impl Into<DbRow> for UserRow {
    fn into(self) -> DbRow {
        DbRow { map: self.map }
    }
}

impl From<DbRow> for UserRow {
    fn from(value: DbRow) -> Self {
        Self { map: value.map }
    }
}

#[derive(Clone, Debug)]
pub struct UserRows {
    pub rows: Vec<UserRow>,
}

impl From<Vec<DbRow>> for UserRows {
    fn from(value: Vec<DbRow>) -> Self {
        Self {
            rows: value.into_iter().map(|r| r.into()).collect(),
        }
    }
}

/// A row as Relatable sees it, using column ID strings, e.g. "c14".
pub type InnerRow = IndexMap<String, DbValue>;
pub type InnerRows = Vec<InnerRow>;

/// Read a path into a UserRows.
pub fn read_table(path: &str) -> Result<UserRows> {
    // Read the records from the given TSV file:
    let mut rdr = ReaderBuilder::new()
        .has_headers(true)
        .delimiter(b'\t')
        .from_reader(File::open(path).expect(&format!("Unable to open '{path}'")));

    let headers: Vec<String> = rdr.headers()?.into_iter().map(|s| s.to_string()).collect();
    let records = rdr.records();
    let rows: Vec<UserRow> = records
        .into_iter()
        .map(|record| {
            let map: IndexMap<ColumnName, DbValue> = zip(headers.iter(), record?.iter())
                .map(|(k, v)| (k.to_string(), DbValue::from(v)))
                .collect();
            Ok(UserRow { map })
        })
        .collect::<Result<Vec<UserRow>>>()?;
    Ok(UserRows { rows })
}

#[async_trait]
pub trait TsvTable {
    fn name(&self) -> String;

    fn id(&self) -> String;

    fn pool(&self) -> &AnyPool;

    // Does not include meta columns: _id, _order, _deleted.
    fn columns(&self) -> Columns;

    fn column_names(&self) -> Vec<String> {
        self.columns()
            .iter()
            .map(|c| c.column.to_string())
            .collect()
    }

    // The regular table includes _id and _order columns.
    fn regular_table_columns(&self) -> Vec<SqlColumn> {
        let mut columns = vec![
            SqlColumn {
                column: "_id".to_string(),
                sql_type: "SERIAL".to_string(),
                primary_key: true,
                ..Default::default()
            },
            SqlColumn {
                column: "_order".to_string(),
                sql_type: ORDER_SQL_TYPE.to_string(),
                unique: true,
                ..Default::default()
            },
        ];
        columns.extend(self.columns().iter().map(|c| c.clone().into()));
        columns
    }

    // The alternate table includes _id, _order, and _deleted columns,
    // plus a _text column for each column that is not already TEXT.
    fn alternate_table_columns(&self) -> Vec<SqlColumn> {
        let mut columns = vec![
            SqlColumn {
                column: "_id".to_string(),
                sql_type: ID_SQL_TYPE.to_string(),
                primary_key: true,
                ..Default::default()
            },
            SqlColumn {
                column: "_order".to_string(),
                sql_type: ORDER_SQL_TYPE.to_string(),
                unique: true,
                ..Default::default()
            },
            SqlColumn {
                column: "_deleted".to_string(),
                sql_type: "BOOL".to_string(),
                ..Default::default()
            },
        ];
        for column in self.columns().iter() {
            columns.push(column.clone().into());
            if column.sql_type.to_uppercase() != "TEXT" {
                let alt = SqlColumn {
                    column: format!("{}_text", column.id()),
                    sql_type: "TEXT".to_string(),
                    ..Default::default()
                };
                columns.push(alt);
            }
        }
        columns
    }

    /// Create a DDL for the regular table,
    /// which includes "_id" and "_order" columns,
    /// and uses the table ID and column IDs.
    fn regular_table_ddl(&self) -> String {
        let table_name = self.id();
        let cols = self
            .regular_table_columns()
            .iter()
            .map(|col| col.ddl(self.pool().syntax()))
            .collect::<Vec<String>>()
            .join(",\n  ");
        format!(
            r#"CREATE TABLE "{table_name}" (
  {cols}
);"#
        )
    }

    /// Create a DDL for the alternate table,
    /// which includes "_id", "_order", and "_deleted" columns,
    /// uses the table ID and column IDs,
    /// and includes "_text" columns for each non-TEXT column.
    fn alternate_table_ddl(&self) -> String {
        let table_name = self.id();
        let cols = self
            .alternate_table_columns()
            .iter()
            .map(|col| col.ddl(self.pool().syntax()))
            .collect::<Vec<String>>()
            .join(",\n  ");
        format!(
            r#"CREATE TABLE "{table_name}_alt" (
  {cols}
);"#
        )
    }

    fn regular_view_columns(&self) -> Vec<String> {
        let mut columns: Vec<String> = ["_id", "_order"]
            .iter()
            .map(|s| format!(r#""{s}""#))
            .collect();
        columns.extend(self.columns().iter().map(|column| {
            format!(
                r#""{id}" AS "{name}""#,
                id = column.id(),
                name = column.column
            )
        }));
        columns
    }

    fn regular_view_ddl(&self) -> String {
        let view_name = self.name();
        let table_id = self.id();
        let columns = self.regular_view_columns().join(", ");
        format!(r#"CREATE VIEW "{view_name}" AS SELECT {columns} FROM "{table_id}";"#,)
    }

    fn alternate_column_mapping(&self) -> IndexMap<String, String> {
        let mut columns: IndexMap<String, String> = ["_id", "_order", "_deleted"]
            .iter()
            .map(|s| (s.to_string(), s.to_string()))
            .collect();
        for column in self.columns().iter() {
            columns.insert(column.id(), column.column.to_string());
            if column.sql_type.to_uppercase() != "TEXT" {
                columns.insert(
                    format!("{}_text", column.id()),
                    format!("{}_text", column.column.to_string()),
                );
            }
        }
        columns
    }

    fn alternate_view_columns(&self) -> Vec<String> {
        self.alternate_column_mapping()
            .iter()
            .map(|(k, v)| {
                if k == v {
                    format!(r#""{v}""#)
                } else {
                    format!(r#""{k}" AS "{v}""#,)
                }
            })
            .collect()
    }

    fn alternate_view_ddl(&self) -> String {
        let view_name = self.name();
        let table_id = self.id();
        let columns = self.alternate_view_columns().join(", ");
        format!(r#"CREATE VIEW "{view_name}_alt" AS SELECT {columns} FROM "{table_id}_alt";"#,)
    }

    fn union_view_ddl(&self) -> String {
        let view_name = self.name();
        let table_id = self.id();
        let regular_columns = self
            .alternate_column_mapping()
            .iter()
            .map(|(k, v)| {
                if k == "_deleted" {
                    r#"FALSE AS "_deleted""#.to_string()
                } else if k == v {
                    format!(r#""{v}""#)
                } else if k.ends_with("_text") {
                    format!(r#"NULL AS "{v}""#)
                } else {
                    format!(r#""{k}" AS "{v}""#)
                }
            })
            .collect::<Vec<String>>()
            .join(", ");
        let alternate_columns = self.alternate_view_columns().join(", ");
        format!(
            r#"CREATE VIEW "{view_name}_union" AS
  SELECT {regular_columns} FROM "{table_id}"
  UNION ALL
  SELECT {alternate_columns} FROM "{table_id}_alt";"#,
        )
    }

    fn text_view_ddl(&self) -> String {
        let table_name = self.name();
        let mut columns: Vec<String> = self
            .columns()
            .iter()
            .map(|c| {
                let name = c.column.to_string();
                if c.sql_type.to_uppercase() != "TEXT" {
                    format!(r#"COALESCE(CAST("{name}" AS TEXT), "{name}_text") AS "{name}""#)
                } else {
                    format!(r#""{name}""#)
                }
            })
            .collect();
        columns.insert(0, r#""_id""#.to_string());
        columns.insert(1, r#""_order""#.to_string());
        let columns = columns.join(", ");
        let message = match self.pool().syntax().name() {
            "sqlite" => format!(
                r#"(
    SELECT
      NULLIF(
        JSON_GROUP_ARRAY(
          JSON_OBJECT(
            'column', "column",
            'value', "value",
            'level', "level",
            'rule', "rule",
            'message', "message"
          )
        ),
        '[]'
      ) AS "_message"
      FROM "message"
      WHERE "table" = '{table_name}'
      AND "row" = "_id"
      ORDER BY "column", "message_id"
    ) AS "_message""#
            ),
            // TODO: Shouldn't have to cast to TEXT.
            "postgres" => format!(
                r#"(
    SELECT json_agg(m.*)::TEXT AS json_agg FROM (
      SELECT "message"."column",
              "message"."value",
              "message"."level",
              "message"."rule",
              "message"."message"
      FROM "message"
      WHERE "message"."table" = '{table_name}' AND "message"."row" = "_id"
      ORDER BY "message"."column", "message"."message_id") m
    ) AS "_message""#
            ),
            _ => unimplemented!(),
        };
        format!(
            r#"CREATE VIEW "{table_name}_text" AS
  SELECT {columns}, {message}
  FROM "{table_name}_union"
  WHERE "_deleted" = FALSE;"#
        )
    }

    /// Generate SQL DDLs for this table, including alternate table and maybe views.
    fn ddl(&self) -> String {
        let mut ddls = vec![self.regular_table_ddl(), self.alternate_table_ddl()];

        // Create regular and alternate views.
        if self.name() != self.id() {
            ddls.push(self.regular_view_ddl());
            ddls.push(self.alternate_view_ddl());
        }

        // Create union and text views.
        ddls.push(self.union_view_ddl());
        ddls.push(self.text_view_ddl());

        ddls.join("\n")
    }

    /// Create all SQL tables and views for this TsvTable.
    async fn exists(&self) -> Result<bool> {
        let result = self.pool().table_exists(&self.name()).await?;
        Ok(result)
    }

    /// Create all SQL tables and views for this TsvTable.
    async fn create(&self) -> Result<()> {
        self.pool().execute_batch(&self.ddl()).await?;
        Ok(())
    }

    /// Create SQL views for this TsvTable.
    async fn create_views(&self) -> Result<()> {
        println!("create views");
        let mut ddls = Vec::new();

        // Create regular and alternate views.
        if self.name() != self.id() {
            ddls.push(self.regular_view_ddl());
            ddls.push(self.alternate_view_ddl());
        }

        // Create union and text views.
        ddls.push(self.union_view_ddl());
        ddls.push(self.text_view_ddl());

        self.pool().execute_batch(&ddls.join("\n")).await?;
        Ok(())
    }

    /// Drop all tables and views for this TsvTable.
    async fn drop(&self) -> Result<()> {
        self.drop_views().await?;

        let table_names = vec![format!("{}_alt", self.id()), self.id()];
        for table_name in table_names {
            self.pool().drop_table(&table_name).await?;
        }
        Ok(())
    }

    async fn drop_views(&self) -> Result<()> {
        let mut view_names = vec![
            format!("{}_text", self.name()),
            format!("{}_union", self.name()),
        ];
        if self.id() != self.name() {
            view_names.extend(vec![
                format!("{}_alt", self.name()),
                self.name().to_string(),
            ]);
        }
        for view_name in view_names {
            self.pool().drop_view(&view_name).await?;
        }
        Ok(())
    }

    async fn add_columns(&self, columns: &[&Column]) -> Result<()> {
        let id = self.id();
        let mut regular = Vec::new();
        let mut alternate = Vec::new();
        for column in columns {
            let ddl = column.ddl(self.pool().syntax());
            let line = format!(r#"ALTER TABLE "{id}" ADD COLUMN {ddl}"#);
            regular.push(line);
            let line = format!(r#"ALTER TABLE "{id}_alt" ADD COLUMN {ddl}"#);
            alternate.push(line);
            if column.sql_type.to_uppercase() != "TEXT" {
                let alt = format!(
                    r#"ALTER TABLE "{id}_alt" ADD COLUMN "c{}_text" TEXT"#,
                    column.id
                );
                alternate.push(alt);
            }
        }

        // Regular table
        let sql = regular.join(";\n");
        self.pool().execute_batch(&sql).await?;

        // Alternate table
        let sql = alternate.join(";\n");
        self.pool().execute_batch(&sql).await?;

        self.drop_views().await?;
        self.create_views().await?;
        Ok(())
    }

    async fn get_row(&self, id: RowID) -> Result<DbRow> {
        let table_name = self.name();
        let sql = format!(r#"SELECT * FROM "{table_name}_union" WHERE "_id" = $1;"#);
        let rows = self.pool().query(&sql, [id]).await?;
        Ok(rows.row()?.clone())
    }

    async fn get_orders(&self, row_ids: &[RowID]) -> Result<Vec<(RowID, RowOrder)>> {
        let table_name = self.name();
        let ids = row_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let sql =
            format!(r#"SELECT "_id", "_order" FROM "{table_name}_union" WHERE "_id" IN({ids});"#);
        let rows = self.pool().query(&sql, ()).await?;
        let rows = rows
            .rows
            .iter()
            .map(|row| {
                (
                    row.get("_id")
                        .expect("a row ID")
                        .as_i32()
                        .expect("valid row ID"),
                    row.get("_order")
                        .expect("a row order")
                        .as_i64()
                        .expect("value row order"),
                )
            })
            .collect();
        Ok(rows)
    }

    /// Return UserRows with only the columns for this table,
    /// and none of the meta columns.
    fn sanitize_rows(&self, rows: UserRows) -> DbRows {
        DbRows {
            rows: rows
                .rows
                .into_iter()
                .map(|row| self.sanitize_row(row))
                .collect(),
        }
    }

    /// Return a UserRow with only the columns for this table,
    /// and none of the meta columns.
    fn sanitize_row(&self, row: UserRow) -> DbRow {
        let column_names = self.column_names();
        DbRow {
            map: row
                .map
                .into_iter()
                .filter(|(k, _)| column_names.contains(k))
                .collect(),
        }
    }

    /// Convert DbRows into TsvRows.
    fn type_rows(&self, rows: DbRows) -> TsvRows {
        rows.iter()
            .map(|db_row| self.type_row(db_row))
            .collect::<Vec<TsvRow>>()
            .into()
    }

    /// Convert a DbRow into a TsvRow.
    fn type_row(&self, row: &DbRow) -> TsvRow {
        let syntax = self.pool().syntax();
        let id = row.get("_id").and_then(|i| i.as_i32()).unwrap_or_default();
        let order = row
            .get("_order")
            .and_then(|o| o.as_i64())
            .unwrap_or_default();
        let deleted = row
            .get("_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or_default();
        let mut map = IndexMap::new();
        for column in self.columns().iter() {
            if let Some(db_value) = row.get(&column.column) {
                // TODO: Handle more nulltypes
                if column.nulltype == "empty" && db_value.to_string() == "" {
                    map.insert(column.id, TsvValue::Regular(DbValue::Null));
                } else {
                    let db_type = syntax.sql_type(&column.sql_type).unwrap_or_default();
                    let value = match db_type.convert(&db_value) {
                        Ok(db_value) => TsvValue::Regular(db_value),
                        Err(_) => TsvValue::NotType(db_value.to_string()),
                    };
                    map.insert(column.id, value);
                }
            }
        }
        // println!("{row:?} {map:?}");
        TsvRow {
            id,
            order,
            deleted,
            map,
        }
    }

    /// Given TsvRows, check that they conform to all database constraints,
    /// including uniqueness, not null, and foreign keys.
    async fn constrain(&self, _rows: &mut TsvRows) -> Result<()> {
        for column in self.columns().iter() {
            if column.is_constrained() {
                todo!("Check column constraints");
            }
        }
        Ok(())
    }

    /// Given DbRows, return constrained TsvRows.
    async fn rows(&self, rows: DbRows) -> Result<TsvRows> {
        let mut rows = self.type_rows(rows);
        self.constrain(&mut rows).await?;
        Ok(rows)
    }

    /// Convert TsvRows to DbRows that can be inserted into the regular table,
    /// which may require using placeholder values.
    async fn insertable(&self, rows: &TsvRows) -> Result<DbRows> {
        for column in self.columns().iter() {
            if column.is_constrained() {
                todo!("Create placeholders for constrained columns");
            }
        }
        let mut db_rows = vec![];
        for row in rows.rows.iter() {
            let mut db_row = row.base_row();
            for (key, value) in row.map.iter() {
                if value.is_regular() {
                    db_row.insert(format!("c{key}"), value.typed_value().unwrap().clone());
                }
            }
            db_rows.push(db_row);
        }
        Ok(DbRows { rows: db_rows })
    }

    /// Return the specified number of new RowOrders
    /// at the end of a table (appending).
    async fn new_orders(&self, count: usize) -> Result<Vec<RowOrder>> {
        // TODO: Account for history/change table:
        // UNION ALL
        // SELECT MAX(previous_order) AS _order FROM history
        //   WHERE "table" = "{id}"
        let sql = format!(
            r#"SELECT MAX(_order) FROM (
                SELECT MAX(_order) AS _order FROM "{id}"
                UNION ALL
                SELECT MAX(_order) AS _order FROM "{id}_alt"
            )
            LIMIT 1"#,
            id = self.id()
        );
        let max_order: RowOrder = self
            .pool()
            .query(&sql, ())
            .await?
            .try_into_value()
            .unwrap_or_default();
        let new_orders = (1..=count)
            .into_iter()
            .map(|i| max_order + (i as RowOrder * NEW_ORDER_MULTIPLIER))
            .collect();
        Ok(new_orders)
    }

    /// Return the specified number of new RowOrders
    /// after a given RowID.
    async fn new_orders_after(&self, count: usize, after_id: RowID) -> Result<Vec<RowOrder>> {
        // TODO: Account for history/change table:
        let table_name = self.name();
        let sql = format!(
            r#"SELECT _order
            FROM "{table_name}_union"
            WHERE _order >= (
                SELECT _order
                FROM "{table_name}_union"
                WHERE _id = $1
            )
            ORDER BY _order
            LIMIT 2"#
        );
        let mut orders: Vec<RowOrder> = self
            .pool()
            .query(&sql, &[after_id])
            .await?
            .try_into_values()?;

        // If we didn't find an order, insert at the beginning.
        if orders.len() == 0 {
            let sql = format!(r#"SELECT MIN(_order) FROM "{table_name}_union" LIMIT 1"#,);
            let min_order: RowOrder = self
                .pool()
                .query(&sql, ())
                .await?
                .try_into_value()
                .unwrap_or_default();
            orders = vec![0, min_order];
        }

        // If we found only one order, append too end.
        if orders.len() == 1 {
            let max_order = orders.first().unwrap();
            let new_orders = (1..=count)
                .into_iter()
                .map(|i| max_order + (i as RowOrder * NEW_ORDER_MULTIPLIER))
                .collect();
            return Ok(new_orders);
        }

        let next_order = orders.pop().unwrap();
        let previous_order = orders.pop().unwrap();

        let gap = if previous_order + count as RowOrder >= next_order {
            self.reorder(previous_order, count).await?
        } else {
            (next_order - previous_order) as usize / (count + 1)
        };

        let new_orders = (1..=count)
            .into_iter()
            .map(|i| previous_order + (i * gap) as RowOrder)
            .collect();
        Ok(new_orders)
    }

    async fn reorder(&self, previous_order: RowOrder, count: usize) -> Result<usize> {
        println!("REORDER: {previous_order} {count}");
        todo!("implement reorder");
    }

    /// Append a set of DbRows to the table, returning the new rows.
    async fn append(&self, rows: UserRows) -> Result<TsvRows> {
        let rows = self.sanitize_rows(rows);
        let mut rows = self.rows(rows).await?;
        let new_orders = self.new_orders(rows.rows.len()).await?;
        for (order, row) in zip(new_orders, rows.rows.iter_mut()) {
            row.order = order;
        }
        self.insert_ordered(&mut rows).await?;
        Ok(rows)
    }

    async fn insert(&self, after_id: RowID, rows: UserRows) -> Result<TsvRows> {
        println!("UserRows {rows:?}");
        let rows = self.sanitize_rows(rows);
        println!("sanitized {rows:?}");
        let mut rows = self.rows(rows).await?;
        println!("tsv rows {rows:?}");
        let new_orders = self.new_orders_after(rows.rows.len(), after_id).await?;
        println!("new orders {new_orders:?}");
        for (order, row) in zip(new_orders, rows.rows.iter_mut()) {
            row.order = order;
        }
        println!("updated rows {rows:?}");
        self.insert_ordered(&mut rows).await?;
        Ok(rows)
    }

    /// Insert ordered rows,
    /// first using "insertable" rows (maybe with placeholder) to generate IDs,
    /// then moving placeholder and alternate rows.
    async fn insert_ordered(&self, rows: &mut TsvRows) -> Result<()> {
        // insert the rows and get IDs
        let db_rows = self.insertable(&rows).await?;
        let new_rows = self.insert_regular(db_rows).await?;

        // Collect the ids
        for (row, new_row) in zip(rows.rows.iter_mut(), new_rows.rows) {
            row.id = new_row
                .get("_id")
                .and_then(|i| i.as_i32())
                .unwrap_or_default();
        }

        // If a row is regular, leave it in place.
        // If a row is alternate, delete from the regular table
        //  and insert into the alternate table.
        let alternate_rows = rows.alternate();
        if alternate_rows.len() > 0 {
            let alternate_ids: Vec<RowID> = alternate_rows
                .iter()
                .map(|r| r.get("_id").unwrap().as_i32().unwrap())
                .collect();
            let kind = self.pool().syntax();
            let prefix = kind.param_prefix();
            let params = alternate_ids
                .iter()
                .enumerate()
                .map(|(i, _)| format!("{prefix}{}", i + 1))
                .collect::<Vec<String>>()
                .join(", ");
            let sql = format!(
                r#"DELETE FROM "{table_id}" WHERE "_id" IN ({params})"#,
                table_id = self.id(),
            );
            let column_names = self
                .alternate_column_mapping()
                .keys()
                .cloned()
                .collect::<Vec<String>>();
            let refs: Vec<&str> = column_names.iter().map(|s| s.as_str()).collect();
            self.pool().execute(&sql, alternate_ids).await?;
            self.pool()
                .insert(&format!("{}_alt", self.id()), &refs, alternate_rows.iter())
                .await?;
        }

        Ok(())
    }

    async fn delete(&self, row_ids: &[RowID]) -> Result<()> {
        let id = self.id();
        let columns = self
            .regular_table_columns()
            .iter()
            .map(|c| c.column.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let ids = row_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let mut sql = format!(
            r#"INSERT INTO "{id}_alt"({columns}) SELECT {columns} FROM "{id}" WHERE _id IN ({ids});"#
        );
        sql += &format!(r#"DELETE FROM "{id}" WHERE _id IN ({ids});"#);
        sql += &format!(r#"UPDATE "{id}_alt" SET _deleted = TRUE WHERE _id IN ({ids});"#);
        self.pool().execute_batch(&sql).await?;
        Ok(())
    }

    async fn undelete(&self, row_ids: &[RowID]) -> Result<()> {
        let table_name = self.name();
        let id = self.id();
        let ids = row_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let sql = format!(r#"SELECT * FROM "{table_name}_union" WHERE _id IN ({ids});"#);
        let db_rows = self.pool().query(&sql, ()).await?;
        let rows = self.rows(db_rows).await?;
        let regular_ids = rows
            .rows
            .iter()
            .filter(|row| row.is_regular())
            .map(|row| row.id.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let columns = self
            .regular_table_columns()
            .iter()
            .map(|c| c.column.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let mut sql = format!(r#"UPDATE "{id}_alt" SET _deleted = FALSE WHERE _id IN ({ids});"#);
        if !regular_ids.is_empty() {
            sql += &format!(
                r#"INSERT INTO "{id}"({columns}) SELECT {columns} FROM "{id}_alt" WHERE _id IN ({regular_ids});"#
            );
            sql += &format!(r#"DELETE FROM "{id}_alt" WHERE _id IN ({regular_ids});"#);
        }
        self.pool().execute_batch(&sql).await?;
        Ok(())
    }

    fn column(&self, name: &str) -> Option<Column> {
        self.columns()
            .iter()
            .filter(|c| c.column == name)
            .nth(0)
            .cloned()
    }

    async fn update(&self, pairs: &[(RowID, UserRow)]) -> Result<()> {
        let row_ids: BTreeSet<RowID> = pairs.iter().map(|(id, _)| *id).collect();
        let table_name = self.name();
        let id = self.id();
        let ids = row_ids
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let sql = format!(r#"SELECT * FROM "{table_name}_union" WHERE _id IN ({ids});"#);
        let db_rows = self.pool().query(&sql, ()).await?;
        let mut rows = self.rows(db_rows).await?;

        // Update rows from pairs.
        let syntax = self.pool().syntax();
        for (id, user_row) in pairs {
            let user_row = self.sanitize_row(user_row.clone());
            for row in rows.rows.iter_mut() {
                if row.id == *id {
                    for (col, db_value) in user_row.iter() {
                        if let Some(column) = self.column(col) {
                            if column.nulltype == "empty" && db_value.to_string() == "" {
                                row.map.insert(column.id, TsvValue::Regular(DbValue::Null));
                            } else {
                                let db_type = syntax.sql_type(&column.sql_type).unwrap_or_default();
                                let value = match db_type.convert(db_value) {
                                    Ok(db_value) => TsvValue::Regular(db_value),
                                    Err(_) => TsvValue::NotType(db_value.to_string()),
                                };
                                row.map.entry(column.id).insert_entry(value);
                            }
                        }
                    }
                }
            }
        }

        let regular_ids = rows
            .rows
            .iter()
            .filter(|row| row.is_regular())
            .map(|row| row.id.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        let alternate_ids = rows
            .rows
            .iter()
            .filter(|row| !row.is_regular())
            .map(|row| row.id.to_string())
            .collect::<Vec<String>>()
            .join(", ");

        // upsert regular table
        let column_names: Vec<String> = self
            .regular_table_columns()
            .iter()
            .map(|c| c.column.to_string())
            .collect();
        let refs: Vec<&str> = column_names.iter().map(|x| x.as_str()).collect();
        let regular_rows = rows.regular();
        // println!("regular {column_names:?} {regular_rows:?}");
        self.pool().upsert(&self.id(), &refs, &regular_rows).await?;

        // upsert alternate table
        let column_names: Vec<String> = self
            .alternate_table_columns()
            .iter()
            .map(|c| c.column.to_string())
            .collect();
        let refs: Vec<&str> = column_names.iter().map(|x| x.as_str()).collect();
        let alternate_rows = rows.alternate();
        // println!("alternate {alternate_rows:?}");
        self.pool()
            .upsert(&format!("{id}_alt"), &refs, &alternate_rows)
            .await?;

        let mut sqls = vec![];
        // Delete alternate rows from regular table.
        if !alternate_ids.is_empty() {
            sqls.push(format!(
                r#"DELETE FROM "{id}" WHERE _id IN ({alternate_ids});"#
            ));
        }
        // Delete regular rows from alternate table.
        if !regular_ids.is_empty() {
            sqls.push(format!(
                r#"DELETE FROM "{id}_alt" WHERE _id IN ({regular_ids});"#
            ));
        }
        self.pool().execute_batch(&sqls.join("\n")).await?;

        Ok(())
    }

    async fn move_after(&self, after_id: RowID, ids: &[RowID]) -> Result<Vec<(RowID, RowOrder)>> {
        let new_orders = self.new_orders_after(ids.len(), after_id).await?;
        let pairs: Vec<(RowID, RowOrder)> = zip(ids, new_orders)
            .map(|(id, order)| (*id, order))
            .collect();
        self.move_to(&pairs).await?;
        Ok(pairs)
    }

    async fn move_to(&self, pairs: &[(RowID, RowOrder)]) -> Result<()> {
        // TODO: record existing orders in history table?
        let rows: Vec<DbRow> = pairs
            .iter()
            .map(|(id, order)| db_row! { "_id" => *id, "_order" => *order})
            .collect();
        let id = self.id();
        self.pool().update(&id, &["_id", "_order"], &rows).await?;
        self.pool()
            .update(&format!("{id}_alt"), &["_id", "_order"], &rows)
            .await?;
        Ok(())
    }

    /// Append rows to the regular table.
    async fn append_regular(&self, rows: DbRows) -> Result<DbRows> {
        let mut rows = rows.clone();
        let new_orders = self.new_orders(rows.len()).await?;
        for (row, order) in zip(rows.iter_mut(), new_orders) {
            row.insert("_order".to_string(), order.into());
        }
        self.insert_regular(rows).await
    }

    /// Insert rows to the regular table.
    async fn insert_regular(&self, rows: DbRows) -> Result<DbRows> {
        let mut column_names = self.columns().ids();
        column_names.insert(0, "_order".to_string());
        // TODO: resolve column IDs
        // TODO: nullify rows
        let refs: Vec<&str> = column_names.iter().map(|x| x.as_str()).collect();
        let new_rows = self
            .pool()
            .insert_returning(&self.id(), &refs, rows.iter(), &[])
            .await?;
        Ok(new_rows)
    }
}

/// Represent a value in a TSV file with information about whether and how we can insert it into the database.
#[derive(Clone, Debug)]
pub enum TsvValue {
    /// A value that satisfies all database constraints.
    Regular(DbValue),
    /// A value that does not have the right database type.
    NotType(String),
    /// A value that fails a NOT NULL constraint.
    NotNotNull(DbValue),
    /// A value that fails a UNIQUE or PRIMARY KEY constraint.
    NotUnique(DbValue),
    /// A value that fails a FOREIGN KEY constraint.
    NotForeign(DbValue),
}

impl Into<DbValue> for TsvValue {
    fn into(self) -> DbValue {
        match self {
            TsvValue::Regular(db_value)
            | TsvValue::NotNotNull(db_value)
            | TsvValue::NotUnique(db_value)
            | TsvValue::NotForeign(db_value) => db_value.clone(),
            TsvValue::NotType(text) => DbValue::Text(text.to_string()),
        }
    }
}

impl TsvValue {
    pub fn is_regular(&self) -> bool {
        match self {
            TsvValue::Regular(_) => true,
            _ => false,
        }
    }

    pub fn typed_value(&self) -> Option<&DbValue> {
        match self {
            TsvValue::Regular(db_value)
            | TsvValue::NotNotNull(db_value)
            | TsvValue::NotUnique(db_value)
            | TsvValue::NotForeign(db_value) => Some(db_value),
            TsvValue::NotType(_) => None,
        }
    }
}

impl Into<DbValue> for &TsvValue {
    fn into(self) -> DbValue {
        self.clone().into()
    }
}

/// A TSV row has an ID, an order, a deleted status, and a map from column IDs to TsvValues.
#[derive(Clone, Debug, Default)]
pub struct TsvRow {
    pub id: RowID,
    pub order: RowOrder,
    pub deleted: bool,
    pub map: IndexMap<ColumnID, TsvValue>,
}

impl TsvRow {
    /// A row is regular if and only if all its values are regular.
    pub fn is_regular(&self) -> bool {
        self.map.values().all(|v| v.is_regular())
    }

    /// A DbRow with just the id, order, and deleted fields.
    pub fn base_row(&self) -> DbRow {
        let mut db_row = DbRow::new();
        db_row.insert("_id".to_string(), DbValue::from(self.id));
        db_row.insert("_order".to_string(), DbValue::from(self.order));
        db_row.insert("_deleted".to_string(), DbValue::from(self.deleted));
        db_row
    }

    /// A row with column ID strings as keys (e.g. "c14"), and DbValues of mixed types.
    pub fn inner(&self) -> InnerRow {
        let mut db_row = self.base_row();
        for (k, v) in self.map.iter() {
            db_row.insert(format!("c{k}"), v.into());
        }
        db_row.map
    }

    /// A DbRow that can be inserted into the regular table,
    /// if this row satisfies all the SQL types and constraints.
    pub fn regular(&self) -> Option<DbRow> {
        let mut db_row = self.base_row();
        for (key, value) in self.map.iter() {
            match value {
                TsvValue::Regular(db_value) => {
                    db_row.insert(format!("c{key}"), db_value.clone());
                }
                _ => return None,
            }
        }
        Some(db_row)
    }

    /// A DbRow that can be inserted into the alternate table,
    pub fn alternate(&self) -> Option<DbRow> {
        if self.is_regular() {
            return None;
        }
        let mut db_row = self.base_row();
        for (key, value) in self.map.iter() {
            match value {
                TsvValue::Regular(db_value)
                | TsvValue::NotNotNull(db_value)
                | TsvValue::NotUnique(db_value)
                | TsvValue::NotForeign(db_value) => {
                    db_row.insert(format!("c{key}"), db_value.clone());
                }
                TsvValue::NotType(text) => {
                    if text == "" {
                        db_row.insert(format!("c{key}_text"), DbValue::Null);
                    } else {
                        db_row.insert(format!("c{key}_text"), DbValue::Text(text.to_string()));
                    }
                }
            }
        }
        Some(db_row)
    }
}

#[derive(Clone, Debug)]
pub struct TsvRows {
    pub rows: Vec<TsvRow>,
}

impl From<Vec<TsvRow>> for TsvRows {
    fn from(value: Vec<TsvRow>) -> Self {
        Self { rows: value }
    }
}

impl TsvRows {
    /// Return all the row IDs.
    pub fn ids(&self) -> Vec<RowID> {
        self.rows.iter().map(|r| r.id).collect()
    }

    /// Get a vector of InnerRows.
    pub fn inner(&self) -> InnerRows {
        self.rows.iter().map(|row| row.inner()).collect()
    }

    /// Get DbRows for the regular table.
    pub fn regular(&self) -> Vec<DbRow> {
        self.rows.iter().filter_map(|row| row.regular()).collect()
    }

    /// Get DbRows for the alternate table.
    pub fn alternate(&self) -> Vec<DbRow> {
        self.rows.iter().filter_map(|row| row.alternate()).collect()
    }

    /// Given a column ID, return a set of DbValues, excluding the NotType variant.
    pub fn column_values(&self, column: RowID) -> HashSet<DbValue> {
        self.rows
            .iter()
            .filter_map(|row| row.map.get(&column))
            .filter_map(|tsv_value| tsv_value.typed_value())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        core::RelatableBuilder,
        data_table::{DataTable, DataTableBuilder},
        row::Row,
        user_row,
    };

    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;

    async fn vecs(table: &DataTable<'_>) -> Result<Vec<Vec<String>>> {
        let mut result = vec![];
        result.push(table.alternate_column_mapping().values().cloned().collect());
        let results = table
            .pool
            .query(
                &format!(r#"SELECT * FROM "{}_union" ORDER BY _order"#, table.name()),
                (),
            )
            .await?;
        for row in results.iter() {
            let row: Row = row.into();
            let mut strings = row.to_strings();
            strings.insert(0, row.id.to_string());
            strings.insert(1, row.order.to_string());
            strings.insert(2, row.deleted.to_string());
            result.push(strings);
        }
        Ok(result)
    }

    #[tokio::test]
    async fn test_tsv_table_ddl() -> Result<()> {
        // Testing SQLite in-memory only.
        let rltbl = RelatableBuilder::from(":memory:").init(true).await?;

        let mut builder = DataTableBuilder::from_path("island.tsv");
        builder.column("island_id").id(1).sql_type("INTEGER");
        builder.column("island").id(2).sql_type("TEXT");
        let table = builder.connect(&rltbl.pool)?;

        assert_eq!(
            r#"CREATE TABLE "t0" (
  "_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "_order" BIGINT UNIQUE,
  "c1" INTEGER,
  "c2" TEXT
);"#,
            table.regular_table_ddl()
        );
        assert_eq!(
            r#"CREATE TABLE "t0_alt" (
  "_id" INTEGER PRIMARY KEY,
  "_order" BIGINT UNIQUE,
  "_deleted" BOOL,
  "c1" INTEGER,
  "c1_text" TEXT,
  "c2" TEXT
);"#,
            table.alternate_table_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island" AS SELECT "_id", "_order", "c1" AS "island_id", "c2" AS "island" FROM "t0";"#,
            table.regular_view_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island_alt" AS SELECT "_id", "_order", "_deleted", "c1" AS "island_id", "c1_text" AS "island_id_text", "c2" AS "island" FROM "t0_alt";"#,
            table.alternate_view_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island_union" AS
  SELECT "_id", "_order", FALSE AS "_deleted", "c1" AS "island_id", NULL AS "island_id_text", "c2" AS "island" FROM "t0"
  UNION ALL
  SELECT "_id", "_order", "_deleted", "c1" AS "island_id", "c1_text" AS "island_id_text", "c2" AS "island" FROM "t0_alt";"#,
            table.union_view_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island_text" AS
  SELECT "_id", "_order", COALESCE(CAST("island_id" AS TEXT), "island_id_text") AS "island_id", "island", (
    SELECT
      NULLIF(
        JSON_GROUP_ARRAY(
          JSON_OBJECT(
            'column', "column",
            'value', "value",
            'level', "level",
            'rule', "rule",
            'message', "message"
          )
        ),
        '[]'
      ) AS "_message"
      FROM "message"
      WHERE "table" = 'island'
      AND "row" = "_id"
      ORDER BY "column", "message_id"
    ) AS "_message"
  FROM "island_union"
  WHERE "_deleted" = FALSE;"#,
            table.text_view_ddl()
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_tsv_table() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_tsv_table").init(true).await?;
        let mut builder = DataTableBuilder::from_path("island.tsv");
        builder.column("island_id").datatype("integer");
        builder.column("island");
        let table = builder.create(&rltbl).await?;

        println!("{:#?}", table.columns());

        assert_eq!(
            vec![vec![
                "_id",
                "_order",
                "_deleted",
                "island_id",
                "island_id_text",
                "island"
            ]],
            vecs(&table).await?,
            "empty table"
        );

        let rows = UserRows {
            rows: vec![user_row! {
                "island_id" => 1,
                "island" => "Torgersen",
            }],
        };
        table.append(rows).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"]
            ],
            vecs(&table).await?,
            "append one row"
        );

        let rows = UserRows {
            rows: vec![user_row! {
                "island_id" => "foo",
                "island" => "Biscoe",
            }],
        };
        table.append(rows).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["2", "2000", "false", "", "foo", "Biscoe"]
            ],
            vecs(&table).await?,
            "append one row with invalid type"
        );

        let rows = UserRows {
            rows: vec![user_row! {
                "island_id" => 3,
                "island" => "Dream",
            }],
        };
        table.insert(1, rows).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["3", "1500", "false", "3", "", "Dream"],
                vec!["2", "2000", "false", "", "foo", "Biscoe"]
            ],
            vecs(&table).await?,
            "insert one row"
        );

        table.delete(&[2, 3]).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["3", "1500", "true", "3", "", "Dream"],
                vec!["2", "2000", "true", "", "foo", "Biscoe"]
            ],
            vecs(&table).await?,
            "delete two rows"
        );

        table.undelete(&[2, 3]).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["3", "1500", "false", "3", "", "Dream"],
                vec!["2", "2000", "false", "", "foo", "Biscoe"]
            ],
            vecs(&table).await?,
            "undelete two rows"
        );

        table.update(&[(2, user_row! {"island_id" => 2})]).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["3", "1500", "false", "3", "", "Dream"],
                vec!["2", "2000", "false", "2", "", "Biscoe"]
            ],
            vecs(&table).await?,
            "update row 2 into regular table"
        );

        table
            .update(&[(2, user_row! { "island_id" => "foo"})])
            .await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["3", "1500", "false", "3", "", "Dream"],
                vec!["2", "2000", "false", "", "foo", "Biscoe"]
            ],
            vecs(&table).await?,
            "update row 2 into alternate table"
        );

        table.move_after(1, &[2]).await?;

        assert_eq!(
            vec![
                vec![
                    "_id",
                    "_order",
                    "_deleted",
                    "island_id",
                    "island_id_text",
                    "island"
                ],
                vec!["1", "1000", "false", "1", "", "Torgersen"],
                vec!["2", "1250", "false", "", "foo", "Biscoe"],
                vec!["3", "1500", "false", "3", "", "Dream"]
            ],
            vecs(&table).await?,
            "move 2 after 1"
        );

        // assert!(false, "DONE");
        rltbl.drop_test().await
    }

    #[ignore]
    #[test]
    fn test_read_table() -> Result<()> {
        let rows = read_table("somedir/table.tsv")?;
        for row in rows.rows {
            println!("ROW: {row:?}");
        }
        assert!(false, "DONE");
        Ok(())
    }
}
