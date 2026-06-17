use crate::{self as rltbl};
use indexmap::IndexMap;
use rltbl::{
    column::{Columns, SqlColumn},
    core::{RelatableError, RowID, RowOrder, ID_SQL_TYPE, NEW_ORDER_MULTIPLIER, ORDER_SQL_TYPE},
    row::{
        ConstrainedRow, ConstrainedRows, ConstrainedValue, InsertedRow, InsertedRows, OrderedRows,
        TypedRow, TypedRows, TypedValue,
    },
};
use rltbl_db::{
    any::AnyPool,
    core::DbQuery,
    db_value::{DbRows, IntoDbRows},
};

use anyhow::Result;
use async_trait::async_trait;
use std::iter::zip;

#[async_trait]
pub trait TsvTable {
    fn name(&self) -> String;

    fn id(&self) -> String;

    fn pool(&self) -> &AnyPool;

    // Does not include meta columns: _id, _order, _deleted.
    fn columns(&self) -> Columns;

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
            .map(|col| col.ddl(&self.pool().kind()))
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
            .map(|col| col.ddl(&self.pool().kind()))
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

    /// Generate SQL DDLs for this table, including alternate table and maybe views.
    fn ddl(&self) -> String {
        let mut ddls = vec![self.regular_table_ddl(), self.alternate_table_ddl()];

        // Create views.
        if self.name() != self.id() {
            ddls.push(self.regular_view_ddl());
            ddls.push(self.alternate_view_ddl());
            ddls.push(self.union_view_ddl());
        }

        ddls.join("\n")
    }

    /// Create all SQL tables and views for this TsvTable.
    async fn create(&self) -> Result<()> {
        self.pool().execute_batch(&self.ddl()).await?;
        Ok(())
    }

    /// Drop all tables and views for this TsvTable.
    async fn drop(&self) -> Result<()> {
        let mut table_names = vec![self.id().to_string(), format!("{}_alt", self.id())];
        table_names.reverse();
        for table_name in table_names {
            self.pool().drop_table(&table_name).await?;
        }

        if self.id() != self.name() {
            let view_names = vec![
                self.name().to_string(),
                format!("{}_alt", self.name()),
                format!("{}_union", self.name()),
            ];
            for view_name in view_names {
                self.pool().drop_view(&view_name).await?;
            }
        }

        Ok(())
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
            .try_into()
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
        let sql = format!(
            r#"SELECT _order
            FROM "{table_name}_union"
            WHERE _order >= (
                SELECT _order
                FROM "{table_name}_union"
                WHERE _id = $1
            )
            ORDER BY _order
            LIMIT 2"#,
            table_name = self.name()
        );
        let mut orders: Vec<RowOrder> = self
            .pool()
            .query(&sql, &[after_id])
            .await?
            .try_into_vec()
            .unwrap_or_default();

        if orders.len() == 0 {
            return Err(
                RelatableError::DataError(format!("No orders found for {}", self.name())).into(),
            );
        }

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

    async fn reorder(&self, _previous_order: RowOrder, _count: usize) -> Result<usize> {
        todo!("implement reorder");
    }

    /// Given TypedRows, ensure that they conform to all database constraints,
    /// including uniqueness, not null, and foreign keys.
    /// TODO: Generate placeholders as a second step? Only for insert.
    async fn constrain(&self, rows: TypedRows) -> ConstrainedRows {
        for column in self.columns().iter() {
            if column.is_constrained() {
                todo!("Handle constrained columns");
            }
        }
        ConstrainedRows {
            rows: rows
                .rows
                .iter()
                .map(|row| ConstrainedRow {
                    map: row
                        .map
                        .iter()
                        .map(|(key, value)| {
                            (
                                key.to_string(),
                                ConstrainedValue {
                                    actual: value.clone(),
                                    placeholder: None,
                                },
                            )
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Append a set of DbRows to the table, returning the new rows.
    async fn append(&self, rows: impl IntoDbRows + Send) -> Result<InsertedRows> {
        // type the rows
        let rows = self.columns().rows(rows);
        // constrain the rows
        let rows = self.constrain(rows).await;
        // order the rows
        let new_orders = self.new_orders(rows.rows.len()).await?;
        let rows = OrderedRows::order(&new_orders, rows);
        self.insert_ordered(rows).await
    }

    async fn insert(&self, after_id: RowID, rows: impl IntoDbRows + Send) -> Result<InsertedRows> {
        let rows = self.columns().rows(rows);
        let rows = self.constrain(rows).await;
        let new_orders = self.new_orders_after(rows.rows.len(), after_id).await?;
        let rows = OrderedRows::order(&new_orders, rows);
        self.insert_ordered(rows).await
    }

    /// Insert ordered rows,
    /// first using "insertable" rows (maybe with placeholder) to generate IDs,
    /// then moving placeholder and alternate rows.
    async fn insert_ordered(&self, rows: OrderedRows) -> Result<InsertedRows> {
        // insert the rows and get IDs
        let db_rows = rows.insertable();
        let new_rows = self.insert_regular(&db_rows).await?;

        // collect the IDs
        let inserted_rows = InsertedRows::identify(rows, new_rows.clone());

        // If a row is regular, leave it in place.
        // If a row is alternate, delete from the regular table
        //  and insert into the alternate table.
        let alternate_rows = inserted_rows.alternate();
        if alternate_rows.len() > 0 {
            let alternate_ids: Vec<RowID> = alternate_rows
                .iter()
                .map(|r| r.get("_id").unwrap().as_i32().unwrap())
                .collect();
            let kind = self.pool().kind();
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
                .insert(&format!("{}_alt", self.id()), &refs, alternate_rows)
                .await?;
        }

        Ok(inserted_rows)
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

    /// Given DbRows from a union table,
    /// type and constrain the rows,
    /// and return InsertedRows.
    async fn validate(&self, rows: impl IntoDbRows + Send) -> Result<InsertedRows> {
        let mut db_rows = rows.into_db_rows();
        let mut meta = vec![];
        let mut typed_rows = vec![];
        for db_row in db_rows.iter_mut() {
            let id = db_row.get("_id").unwrap().as_i32().unwrap();
            let order = db_row.get("_order").unwrap().as_i64().unwrap();
            let deleted = db_row
                .get("_deleted")
                .and_then(|v| v.as_bool())
                .unwrap_or_default();
            meta.push((id, order, deleted));

            db_row.shift_remove("_id");
            db_row.shift_remove("_order");
            db_row.shift_remove("_deleted");

            let mut map: IndexMap<String, TypedValue> = IndexMap::new();
            for (k, v) in db_row.iter() {
                if k.ends_with("_text") && !v.is_null() {
                    map.insert(
                        k.strip_suffix("_text").unwrap().to_string(),
                        TypedValue::Text(v.as_str().unwrap_or_default().to_string()),
                    );
                } else {
                    map.insert(k.to_string(), TypedValue::Regular(v.clone()));
                }
            }
            typed_rows.push(TypedRow { map });
        }
        let typed_rows = TypedRows { rows: typed_rows };
        let constrained_rows = self.constrain(typed_rows).await;
        Ok(InsertedRows {
            rows: zip(meta, constrained_rows.rows)
                .map(|((id, order, deleted), row)| InsertedRow {
                    id,
                    order,
                    deleted,
                    map: row.map,
                })
                .collect(),
        })
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
        let inserted_rows = self.validate(db_rows).await?;
        let regular_ids = inserted_rows
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
        sql += &format!(
            r#"INSERT INTO "{id}"({columns}) SELECT {columns} FROM "{id}_alt" WHERE _id IN ({regular_ids});"#
        );
        sql += &format!(r#"DELETE FROM "{id}_alt" WHERE _id IN ({regular_ids});"#);
        self.pool().execute_batch(&sql).await?;
        Ok(())
    }

    /// Append rows to the regular table.
    async fn append_regular(&self, rows: impl IntoDbRows + Send) -> Result<DbRows> {
        let mut rows = rows.into_db_rows();
        let new_orders = self.new_orders(rows.len()).await?;
        for (row, order) in zip(rows.iter_mut(), new_orders) {
            row.insert("_order".to_string(), order.into());
        }
        self.insert_regular(rows).await
    }

    /// Insert rows to the regular table.
    async fn insert_regular(&self, rows: impl IntoDbRows + Send) -> Result<DbRows> {
        let mut column_names = self.columns().ids();
        column_names.insert(0, "_order".to_string());
        // TODO: resolve column IDs
        // TODO: nullify rows
        let refs: Vec<&str> = column_names.iter().map(|x| x.as_str()).collect();
        let new_rows = self
            .pool()
            .insert_returning(&self.id(), &refs, rows, &[])
            .await?;
        Ok(new_rows)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        column::{Column, ColumnBuilder},
        core::Relatable,
        data_table::DataTable,
        row::Row,
        table::Table,
    };

    use super::*;
    use anyhow::Result;
    use pretty_assertions::assert_eq;
    use rltbl_db::{db_row, db_value::DbRow};

    #[allow(dead_code)]
    async fn tsv(table: &DataTable<'_>) -> Result<String> {
        let header = table
            .columns()
            .iter()
            .map(|c| c.column.clone())
            .collect::<Vec<String>>()
            .join("\t");
        let header = format!("_id\t_order\t{header}");
        let results = table
            .pool
            .query(&format!(r#"SELECT * FROM "{}""#, table.name()), ())
            .await?;
        let data = results
            .iter()
            .map(|row| {
                row.values()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join("\t")
            })
            .collect::<Vec<String>>()
            .join("\n");
        Ok(format!("{header}\n{data}"))
    }

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

    async fn create_table<'a>(rltbl: &'a Relatable) -> Result<DataTable<'a>> {
        let mut table = rltbl
            .table_table()
            .add(Table::from_path("island.tsv"))
            .await?
            .connect(&rltbl.pool);
        table.columns = vec![
            ColumnBuilder::new("island", "island_id")
                .datatype("integer")
                .build()
                .unwrap(),
            ColumnBuilder::new("island", "island")
                .datatype("trimmed_line")
                .build()
                .unwrap(),
        ]
        .into();
        let datatypes = rltbl.datatypes().await;
        table.columns.sql_types(&datatypes);
        let refs: Vec<&Column> = table.columns.iter().collect();
        table.columns = rltbl.column_table().add(&refs).await?.into();
        table.create().await?;
        Ok(table)
    }

    #[tokio::test]
    async fn test_tsv_table_ddl() -> Result<()> {
        // Testing SQLite in-memory only.
        let rltbl = Relatable::init(
            &true,
            Some(":memory:"),
            &rltbl_db::core::CachingStrategy::None,
        )
        .await?;

        let mut table = Table::from_path("island.tsv").connect(&rltbl.pool);
        table.id = 1;
        let mut col1 = ColumnBuilder::new("island", "island_id")
            .datatype("integer")
            .build()
            .unwrap();
        col1.id = 1;
        let mut col2 = ColumnBuilder::new("island", "island")
            .datatype("trimmed_line")
            .build()
            .unwrap();
        col2.id = 2;
        table.columns = vec![col1, col2].into();
        let datatypes = rltbl.datatypes().await;
        table.columns.sql_types(&datatypes);

        assert_eq!(
            r#"CREATE TABLE "t1" (
  "_id" INTEGER PRIMARY KEY AUTOINCREMENT,
  "_order" BIGINT UNIQUE,
  "c1" INTEGER,
  "c2" TEXT
);"#,
            table.regular_table_ddl()
        );
        assert_eq!(
            r#"CREATE TABLE "t1_alt" (
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
            r#"CREATE VIEW "island" AS SELECT "_id", "_order", "c1" AS "island_id", "c2" AS "island" FROM "t1";"#,
            table.regular_view_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island_alt" AS SELECT "_id", "_order", "_deleted", "c1" AS "island_id", "c1_text" AS "island_id_text", "c2" AS "island" FROM "t1_alt";"#,
            table.alternate_view_ddl()
        );
        assert_eq!(
            r#"CREATE VIEW "island_union" AS
SELECT "_id", "_order", FALSE AS "_deleted", "c1" AS "island_id", NULL AS "island_id_text", "c2" AS "island" FROM "t1"
UNION ALL
SELECT "_id", "_order", "_deleted", "c1" AS "island_id", "c1_text" AS "island_id_text", "c2" AS "island" FROM "t1_alt";"#,
            table.union_view_ddl()
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_tsv_table() -> Result<()> {
        let rltbl = Relatable::test("test_tsv_table", false).await?;
        let table = create_table(&rltbl).await?;

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

        let rows = vec![db_row! {
            "island_id" => 1,
            "island" => "Torgersen",
        }];
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

        let rows = vec![db_row! {
            "island_id" => "foo",
            "island" => "Biscoe",
        }];
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

        let rows = vec![db_row! {
            "island_id" => 3,
            "island" => "Dream",
        }];
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

        // assert!(false, "DONE");
        rltbl.drop_test().await
    }
}
