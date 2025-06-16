//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use crate::{self as rltbl};

use anyhow::Result;
use indexmap::IndexMap;
use rltbl::{
    core::{Relatable, RelatableError, NEW_ORDER_MULTIPLIER},
    sql::{self, DbKind, DbTransaction, JsonRow, SqlParam},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Table {
    pub name: String,
    /// The name of the view (blank if there is none) associated with the table
    pub view: String,
    /// The id of the most recent change to this table.
    pub change_id: usize,
    // We may eventually want to turn `columns` into a special-purpose struct, but for now a
    // simple IndexMap suffices.
    /// The table's column configuration, implemented as a map from column names to [Column]s.
    pub columns: IndexMap<String, Column>,
    pub editable: bool,
    /// Indicates whether the table has the _id and _order meta columns enabled:
    pub has_meta: bool,
}

impl Default for Table {
    fn default() -> Self {
        Self {
            name: "".into(),
            view: "".into(),
            change_id: 0,
            columns: IndexMap::new(),
            editable: true,
            has_meta: true,
        }
    }
}

impl Table {
    /// Returns a [Table] corresponding to the given table name.
    pub async fn get_table(table_name: &str, rltbl: &Relatable) -> Result<Self> {
        tracing::trace!("Table::get_table({table_name:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let table = Self::_get_table(table_name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(table)
    }

    /// Returns a [Table] corresponding to the given table name using the given transaction.
    pub fn _get_table(table_name: &str, tx: &mut DbTransaction<'_>) -> Result<Self> {
        tracing::trace!("Table::_get_table({table_name:?}, tx)");
        let result = Self::_view_exists(table_name, "default", tx)?;
        let view = {
            if result {
                format!("{table_name}_default_view")
            } else {
                String::from(table_name)
            }
        };

        let statement = format!(
            r#"SELECT MAX("change_id") FROM "history" WHERE "table" = {sql_param}"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([table_name]);
        let change_id = match tx.query_value(&statement, Some(&params))? {
            Some(value) => value.as_u64().unwrap_or_default() as usize,
            None => 0,
        };

        Ok(Table {
            name: table_name.to_string(),
            view,
            change_id,
            columns: Self::_collect_column_info(table_name, tx)?
                .0
                .into_iter()
                .map(|column| (column.name.clone(), column))
                .collect::<IndexMap<_, _>>(),
            ..Default::default()
        })
    }

    /// Query the database through the given [Relatable] instance to determine whether the given
    /// table exists.
    pub async fn table_exists(table_name: &str, rltbl: &Relatable) -> Result<bool> {
        tracing::trace!("Table::table_exists({table_name}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let table_exists = Self::_table_exists(table_name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(table_exists)
    }

    /// Query the database through the given [DbTransaction] instance to determine whether the given
    /// table exists.
    pub fn _table_exists(table_name: &str, tx: &mut DbTransaction<'_>) -> Result<bool> {
        tracing::trace!("Table::_table_exists({table_name}, tx)");
        let (sql, params) = match tx.kind() {
            DbKind::Sqlite => {
                let sql_param = SqlParam::new(&tx.kind()).next();
                (
                    format!(
                        r#"SELECT 1 FROM "sqlite_master"
                           WHERE "type" = {sql_param} AND name = {sql_param} LIMIT 1"#,
                    ),
                    json!(["table", table_name]),
                )
            }
            DbKind::Postgres => {
                let mut sql_param_gen = SqlParam::new(&tx.kind());
                let sql_param_1 = sql_param_gen.next();
                let sql_param_2 = sql_param_gen.next();
                (
                    format!(
                        r#"SELECT 1 FROM "information_schema"."tables"
                           WHERE "table_type" LIKE {sql_param_1} AND "table_name" = {sql_param_2}"#,
                    ),
                    json!(["%TABLE", table_name]),
                )
            }
        };
        match tx.query_value(&sql, Some(&params))? {
            None => Ok(false),
            Some(_) => Ok(true),
        }
    }

    /// Determine whether a view of the given type exists for the table in the database.
    pub async fn view_exists(&self, view_type: &str, rltbl: &Relatable) -> Result<bool> {
        tracing::trace!("Table::view_exists({self:?}, {view_type}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let view_exists = Self::_view_exists(&self.name, view_type, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(view_exists)
    }

    /// Determine whether a view of the given type exists for the table in the database, using the
    /// given transaction.
    pub fn _view_exists(table: &str, view_type: &str, tx: &mut DbTransaction<'_>) -> Result<bool> {
        tracing::trace!("Table::_view_exists({table}, {view_type}, tx)");
        let (statement, params) = match tx.kind() {
            DbKind::Sqlite => {
                let sql_param = SqlParam::new(&tx.kind()).next();
                (
                    format!(
                        r#"SELECT 1
                           FROM sqlite_master
                           WHERE type = 'view' AND name = {sql_param}"#
                    ),
                    json!([format!("{table}_{view_type}_view")]),
                )
            }
            DbKind::Postgres => {
                let mut sql_param_gen = SqlParam::new(&tx.kind());
                let sql_param_1 = sql_param_gen.next();
                let sql_param_2 = sql_param_gen.next();
                (
                    format!(
                        r#"SELECT 1
                           FROM "information_schema"."tables"
                           WHERE "table_name" = {sql_param_1}
                           AND "table_type" = {sql_param_2}"#,
                    ),
                    json!([format!("{table}_{view_type}_view"), "VIEW"]),
                )
            }
        };
        let result = tx.query_value(&statement, Some(&params))?;
        match result {
            None => Ok(false),
            _ => Ok(true),
        }
    }

    /// Use the given [relatable](crate) instance to ensure that the default view for this
    /// table has been created, and to set the [view name](Table::view) for this table to
    /// TABLENAME_default_view.
    pub async fn ensure_default_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::ensure_default_view_created({self:?}, {rltbl:?})");
        let (columns, meta_columns) = self.collect_column_info(rltbl).await?;
        self.view = format!("{}_default_view", self.name);
        tracing::debug!(
            r#"Creating default view "{}" with columns {columns:?}"#,
            self.view
        );
        let id_col = match meta_columns.iter().any(|c| c.name == "_id") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_id"#,
        };
        let order_col = match meta_columns.iter().any(|c| c.name == "_order") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_order"#,
        };

        for sql in sql::generate_default_view_ddl(
            &self.name,
            &self.view,
            id_col,
            order_col,
            &columns,
            &rltbl.connection.kind(),
        ) {
            rltbl.connection.query(&sql, None).await?;
        }
        Ok(())
    }

    /// Use the given [relatable](crate) instance to ensure that the text view for this
    /// table has been created, and to set the [view name](Table::view) for this table to
    /// TABLENAME_text_view.
    pub async fn ensure_text_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::ensure_text_view_created({self:?}, {rltbl:?})");

        // The default view needs to be created first:
        self.ensure_default_view_created(rltbl).await?;

        // Create the text view:
        let (columns, meta_columns) = self.collect_column_info(rltbl).await?;
        self.view = format!("{}_text_view", self.name);
        tracing::debug!(
            r#"Creating text view "{}" with columns {columns:?}"#,
            self.view
        );
        let id_col = match meta_columns.iter().any(|c| c.name == "_id") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_id"#,
        };
        let order_col = match meta_columns.iter().any(|c| c.name == "_order") {
            false => r#"rowid"#, // This *must* be lowercase.
            true => r#"_order"#,
        };

        for sql in sql::generate_text_view_ddl(
            &self.name,
            &self.view,
            id_col,
            order_col,
            &columns,
            &rltbl.connection.kind(),
        ) {
            rltbl.connection.query(&sql, None).await?;
        }
        Ok(())
    }

    /// Returns the given table's columns, as defined by the (optional) column table, as a map from
    /// column names to [Column]s using the given [Relatable] instance. When the column table does
    /// not exist, returns an empty map
    pub async fn get_column_table_columns(
        table_name: &str,
        rltbl: &Relatable,
    ) -> Result<IndexMap<String, Column>> {
        tracing::trace!("Table::get_column_table_columns({table_name}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let columns = Self::_get_column_table_columns(table_name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(columns)
    }

    /// Returns the given table's columns, as defined by the (optional) column table, as a map from
    /// column names to [Column]s using the given [DbTransaction]. When the column table does
    /// not exist, returns an empty map
    fn _get_column_table_columns(
        table_name: &str,
        tx: &mut DbTransaction<'_>,
    ) -> Result<IndexMap<String, Column>> {
        tracing::trace!("Table::_get_column_table_columns({table_name:?}, tx)");
        if !Self::_table_exists("column", tx)? {
            Ok(IndexMap::new())
        } else {
            let sql = format!(
                r#"SELECT * FROM "column" WHERE "table" = {sql_param}"#,
                sql_param = SqlParam::new(&tx.kind()).next()
            );
            let params = json!([table_name]);
            let json_columns = tx.query(&sql, Some(&params))?;
            let mut columns = IndexMap::new();
            for json_col in json_columns {
                columns.insert(
                    json_col.get_string("column")?,
                    Column {
                        name: json_col.get_string("column")?,
                        table: json_col.get_string("table")?,
                        label: json_col.get_string("label").ok(),
                        description: json_col.get_string("description").ok(),
                        datatype: json_col.get_string("datatype").ok(),
                        nulltype: json_col.get_string("nulltype").ok(),
                        ..Default::default()
                    },
                );
            }
            Ok(columns)
        }
    }

    /// Query the database for the column names associated with the given table and their
    /// datatypes
    fn get_db_table_columns(table: &str, tx: &mut DbTransaction<'_>) -> Result<Vec<JsonRow>> {
        tracing::trace!("Table::_get_db_table_columns({table:?}, tx)");
        match tx.kind() {
            DbKind::Sqlite => {
                let sql = format!(
                    r#"SELECT "name", "type" AS "datatype"
                       FROM pragma_table_info("{table}") ORDER BY "cid""#
                );
                tx.query(&sql, None)
            }
            DbKind::Postgres => {
                let sql = format!(
                    r#"SELECT "column_name"::TEXT AS "name", "data_type"::TEXT AS "datatype"
                       FROM "information_schema"."columns"
                       WHERE "table_name" = {sql_param}
                       ORDER BY "ordinal_position""#,
                    sql_param = SqlParam::new(&tx.kind()).next()
                );
                let params = json!([table]);
                tx.query(&sql, Some(&params))
            }
        }
    }

    /// Returns a tuple whose first position contains a list of the given table's columns, and whose
    /// second position contains a list of the given table's metacolumns
    pub async fn collect_column_info(
        &self,
        rltbl: &Relatable,
    ) -> Result<(Vec<Column>, Vec<Column>)> {
        tracing::trace!("Table::get_db_table_columns({self:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let columns = Self::_collect_column_info(&self.name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(columns)
    }

    /// Returns a tuple whose first position contains a list of the given table's columns, and whose
    /// second position contains a list of the given table's metacolumns, using the given database
    /// transaction
    pub fn _collect_column_info(
        table_name: &str,
        tx: &mut DbTransaction<'_>,
    ) -> Result<(Vec<Column>, Vec<Column>)> {
        tracing::trace!("Table::collect_column_info({table_name}, tx)");

        // Get information about the table's columns from the optional column table:
        let column_columns = Self::_get_column_table_columns(table_name, tx)?;

        // Get the table's columns from the database and merge it with the information from the
        // column table that we just collected:
        let mut columns = vec![];
        let mut meta_columns = vec![];
        for column in Self::get_db_table_columns(table_name, tx)? {
            match column.get_string("name")? {
                column_name if column_name.starts_with("_") => meta_columns.push(Column {
                    name: column_name,
                    table: table_name.to_string(),
                    ..Default::default()
                }),
                column_name => columns.push(Column {
                    label: column_columns
                        .get(&column_name)
                        .and_then(|col| col.label.clone()),
                    description: column_columns
                        .get(&column_name)
                        .and_then(|col| col.description.clone()),
                    nulltype: column_columns
                        .get(&column_name)
                        .and_then(|col| col.nulltype.clone()),
                    datatype: {
                        // Fall back to the SQL type if no datatype is defined in the column
                        // table or the column table does not exist:
                        match column_columns
                            .get(&column_name)
                            .and_then(|col| col.datatype.clone())
                        {
                            Some(datatype) => Some(datatype),
                            None => Some(column.get_string("datatype")?),
                        }
                    },
                    name: column_name,
                    table: table_name.to_string(),
                    ..Default::default()
                }),
            };
        }
        if columns.is_empty() && meta_columns.is_empty() {
            return Err(RelatableError::DataError(format!(
                "No db columns found for: {}",
                table_name
            ))
            .into());
        }
        Ok((columns, meta_columns))
    }

    /// Fetches the [Column] struct representing the configuration of the given column from this
    /// table's [columns configuration](Table::columns)
    pub fn get_config_for_column(&self, column: &str) -> Column {
        tracing::trace!("Table::get_config_for_column({self:?}, {column}, tx)");
        match self.columns.get(column) {
            Some(column) => column.clone(),
            None => {
                tracing::warn!(
                    "No configuration found for column '{table}.{column}'",
                    table = self.name
                );
                Column::default()
            }
        }
    }

    /// Retrieve the given attribute of the given column from this table's
    /// [columns configuration](Table::columns)
    pub fn get_configured_column_attribute(&self, column: &str, attribute: &str) -> Option<String> {
        tracing::trace!(
            "Table::get_configured_column_attribute({self:?}, {column:?}, {attribute:?})"
        );
        self.columns.get(column).and_then(|col| match attribute {
            "table" => Some(col.table.to_string()),
            "column" => Some(col.name.to_string()),
            "label" => match &col.label {
                None => None,
                Some(label) if label == "" => None,
                Some(_) => col.label.clone(),
            },
            "description" => match &col.description {
                None => None,
                Some(description) if description == "" => None,
                Some(_) => col.description.clone(),
            },
            "datatype" => match &col.datatype {
                None => None,
                Some(datatype) if datatype == "" => None,
                Some(_) => col.datatype.clone(),
            },
            "nulltype" => match &col.nulltype {
                None => None,
                Some(nulltype) if nulltype == "" => None,
                Some(_) => col.nulltype.clone(),
            },
            _ => None,
        })
    }

    /// Return a [JsonRow] representing the given row of the given table, using the
    /// given transaction.
    pub fn _get_row(
        table: &str,
        row: usize,
        tx: &mut DbTransaction<'_>,
    ) -> Result<Option<JsonRow>> {
        tracing::trace!("Table::_get_row({table:}?, {row}, tx)");
        let sql = format!(
            r#"SELECT * FROM "{table}" WHERE "_id" = {sql_param}"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([row]);
        tx.query_one(&sql, Some(&params))
    }

    /// Determine what the next created row id for the given table will be
    pub async fn get_next_id(&self, rltbl: &Relatable) -> Result<usize> {
        tracing::trace!("Table::get_next_id({self:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let rowid = self._get_next_id(&mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(rowid)
    }

    /// Query the database for what the id of the next created row of the given table will be
    pub fn _get_next_id(&self, tx: &mut DbTransaction<'_>) -> Result<usize> {
        tracing::trace!("Table::_get_next_id({self:?}, tx)");
        let current_row_id = match tx.kind() {
            DbKind::Sqlite => {
                let sql = r#"SELECT seq FROM sqlite_sequence WHERE name = ?"#;
                let params = json!([self.name]);
                tx.query_value(sql, Some(&params))?
            }
            DbKind::Postgres => {
                let sql = format!(
                    // Note that in the case of postgres an _id column is required.
                    r#"SELECT last_value FROM "{table}__id_seq""#,
                    table = self.name
                );
                tx.query_value(&sql, None)?
            }
        };
        let current_row_id = match current_row_id {
            Some(value) => value.as_u64().unwrap_or_default() as usize,
            None => 0,
        };
        Ok(current_row_id + 1)
    }

    /// Returns the row id that comes before the given row in the given table, using the given
    /// transaction.
    pub fn _get_previous_row_id(
        table: &str,
        row: usize,
        tx: &mut DbTransaction<'_>,
    ) -> Result<usize> {
        tracing::trace!("Table::_get_previous_row_id({table}, {row}, tx)");
        let curr_row_order = Self::_get_row_order(table, row, tx)?;
        let sql = format!(
            r#"SELECT "_id" FROM "{table}" WHERE "_order" < {sql_param}
               ORDER BY "_order" DESC LIMIT 1"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([curr_row_order]);
        let rows = tx.query(&sql, Some(&params))?;
        if rows.len() == 0 {
            Ok(0)
        } else {
            rows[0].get_unsigned("_id")
        }
    }

    /// Returns the value of the _order column of the given row from the given table using the
    /// given transaction.
    fn _get_row_order(table: &str, row: usize, tx: &mut DbTransaction<'_>) -> Result<usize> {
        tracing::trace!("Table::_get_row_order({table:?}, {row}, tx)");
        let sql = format!(
            r#"SELECT "_order" FROM "{table}" WHERE "_id" = {sql_param}"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([row]);
        let rows = tx.query(&sql, Some(&params))?;
        if rows.len() == 0 {
            return Err(
                RelatableError::InputError(format!("No row {row} in table '{table}'")).into(),
            );
        }
        Ok(rows[0].get_unsigned("_order")?)
    }
}

/// Represents a column from some table
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Column {
    pub name: String,
    pub table: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub datatype: Option<String>,
    pub nulltype: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
}

/// Represents a row from some table
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: usize,
    pub order: usize,
    pub change_id: usize,
    pub cells: IndexMap<String, Cell>,
}

impl Row {
    /// Prepares a new [Row] for insertion to the given [Table], with its [id](Row::id) and
    /// [order](Row::order) fields pre-assigned with their correct next values for this table
    pub fn prepare_new(
        table: &Table,
        json_row: Option<&JsonRow>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<Self> {
        tracing::trace!("Row::prepare_new({table:?}, {json_row:?}, tx)");
        let json_row = match json_row {
            None => {
                let columns = {
                    let columns = Table::get_db_table_columns(&table.name, tx)?;
                    if columns.is_empty() {
                        return Err(RelatableError::DataError(format!(
                            "No defined columns for: {table}",
                            table = table.name
                        ))
                        .into());
                    }
                    columns
                        .iter()
                        .map(|c| c.get_string("name").expect("No 'name' found"))
                        .filter(|n| !n.starts_with("_"))
                        .collect::<Vec<_>>()
                };
                let columns = columns.iter().map(|c| c.as_str()).collect::<Vec<_>>();
                JsonRow::from_strings(&columns)
            }
            Some(json_row) => json_row.clone(),
        };
        let mut row = Row::from(json_row);
        row.id = table._get_next_id(tx)?;
        row.order = NEW_ORDER_MULTIPLIER * row.id;
        row.change_id = table.change_id;
        tracing::debug!("Prepared a new row: {row:?}");
        Ok(row)
    }

    /// Convert the [text](Cell::text) values of all of the row's [cells](Row::cells) to
    /// strings and return them to the caller as a vector
    pub fn to_strings(&self) -> Vec<String> {
        tracing::trace!("Row::to_strings()");
        self.cells.values().map(|cell| cell.text.clone()).collect()
    }

    /// Generate an insert statement and a [JsonValue] representing an [Array](JsonValue::Array) of
    /// parameters that need to be bound to the statement before it is executed.
    pub fn as_insert(&self, table: &str, db_kind: &DbKind) -> (String, JsonValue) {
        tracing::trace!("Row::as_insert({table:?})");
        let id = self.id;
        let order = self.order;
        let quoted_column_names = self
            .cells
            .keys()
            .map(|k| format!(r#""{k}""#))
            .collect::<Vec<_>>();

        let mut sql_param_gen = SqlParam::new(db_kind);
        let (value_placeholders, params) = {
            let mut params = vec![json!(id), json!(order)];
            let mut value_placeholders = vec![sql_param_gen.next(), sql_param_gen.next()];
            for cell in self.cells.values() {
                if cell.value == JsonValue::Null {
                    value_placeholders.push("NULL".to_string());
                } else {
                    value_placeholders.push(sql_param_gen.next());
                    params.push(cell.value.clone());
                }
            }
            (value_placeholders, params)
        };

        let sql = if quoted_column_names.len() == 0 {
            format!(
                r#"INSERT INTO "{table}"
                   ("_id", "_order")
                   VALUES ({column_values})"#,
                column_values = value_placeholders.join(", ")
            )
        } else {
            format!(
                r#"INSERT INTO "{table}"
                   ("_id", "_order", {quoted_column_names})
                   VALUES ({column_values})"#,
                quoted_column_names = quoted_column_names.join(", "),
                column_values = value_placeholders.join(", "),
            )
        };
        (sql, json!(params))
    }

    /// Validate this row, which belongs to the given [Table], using the given [DbTransaction],
    /// and add any resulting validation [messages](Message) to the message table
    pub fn validate(&mut self, table: &Table, tx: &mut DbTransaction<'_>) -> Result<&Self> {
        for (column, cell) in self.cells.iter_mut() {
            let column_details = table.get_config_for_column(column);
            let datatype = match column_details.datatype {
                None => "text".to_string(),
                Some(ref dt) if dt == "" => "text".to_string(),
                Some(ref dt) => dt.to_string(),
            };
            cell.validate(&column_details)?;
            if cell.message_level() >= 2 {
                let mut sql_param_gen = SqlParam::new(&tx.kind());
                let sql = format!(
                    r#"INSERT INTO "message"
                       ("added_by", "table", "row", "column", "value", "level", "rule", "message")
                       VALUES ({p1}, {p2}, {p3}, {p4}, {p5}, {p6}, {p7}, {p8})"#,
                    p1 = sql_param_gen.next(),
                    p2 = sql_param_gen.next(),
                    p3 = sql_param_gen.next(),
                    p4 = sql_param_gen.next(),
                    p5 = sql_param_gen.next(),
                    p6 = sql_param_gen.next(),
                    p7 = sql_param_gen.next(),
                    p8 = sql_param_gen.next(),
                );
                let params = json!([
                    "Valve",
                    table.name,
                    self.id,
                    column,
                    cell.value,
                    "error",
                    format!("datatype:{datatype}"),
                    "incorrect datatype"
                ]);
                tx.query(&sql, Some(&params))?;
            }
        }

        Ok(self)
    }
}

impl From<Row> for Vec<String> {
    /// Wrapper around [Row::to_strings()]
    fn from(row: Row) -> Self {
        tracing::trace!("Row::from({row:?})");
        row.to_strings()
    }
}

impl From<JsonRow> for Row {
    fn from(row: JsonRow) -> Self {
        let id = row
            .content
            .get("_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let order = row
            .content
            .get("_order")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let change_id = row
            .content
            .get("_change_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as usize;
        let mut cells: IndexMap<String, Cell> = row
            .content
            .iter()
            // Ignore columns that start with "_"
            .filter(|(k, _)| !k.starts_with("_"))
            .map(|(k, v)| (k.clone(), v.into()))
            .collect();
        let messages = row.content.get("_message");
        if let Some(m) = messages {
            let mut messages = m.clone();
            // WARN: Converting _message string to JSON.
            match m {
                JsonValue::String(m) => messages = serde_json::from_str(&m).unwrap(),
                _ => (),
            }
            if let Some(messages) = messages.as_array() {
                for message in messages.iter() {
                    let column = message
                        .as_object()
                        .unwrap()
                        .get("column")
                        .unwrap()
                        .as_str()
                        .unwrap();
                    let message: Message =
                        serde_json::from_value(message.clone()).unwrap_or_default();
                    if let Some(cell) = cells.get(column) {
                        let mut new_cell = cell.clone();
                        new_cell.messages.push(message);
                        cells.insert(column.to_string(), new_cell);
                    }
                }
            }
        }

        Self {
            id,
            order,
            change_id,
            cells,
        }
    }
}

/// Represents a cell from a row in a given table
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    pub value: JsonValue,
    pub text: String,
    pub messages: Vec<Message>,
}

impl From<&JsonValue> for Cell {
    /// Converts a [JsonValue] to a [Cell]
    fn from(value: &JsonValue) -> Self {
        tracing::trace!("Cell::from({value:?})");
        Self {
            value: value.clone(),
            text: match value {
                JsonValue::String(value) => value.to_string(),
                JsonValue::Null => String::new(),
                value => format!("{value}"),
            },
            messages: vec![],
        }
    }
}

impl Cell {
    /// Validate this cell, which belongs to the given [Column], adding any validation
    /// [messages](Message) to the cell's [messages](Cell::messages) field.
    pub fn validate(&mut self, column: &Column) -> Result<&Self> {
        fn invalidate(cell: &mut Cell, column: &Column) {
            cell.messages.push(Message {
                level: "error".to_string(),
                rule: format!(
                    "datatype:{}",
                    match &column.datatype {
                        None => "text",
                        Some(datatype) => datatype,
                    }
                ),
                message: "incorrect datatype".to_string(),
            });
        }

        match sql::get_sql_type(&column.datatype)?.to_lowercase().as_str() {
            "integer" => match &mut self.value {
                JsonValue::Number(number) => match number.to_string().parse::<isize>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, column),
            },
            "text" | "" => (),
            unsupported => {
                return Err(RelatableError::InputError(format!(
                    "Unsupported datatype: '{unsupported}'"
                ))
                .into())
            }
        };

        Ok(self)
    }

    /// Report the maximum [error level](Message::level) associated with this cell's
    /// [messages](Cell::messages), where 0 represents no error, 1 represents the presence of
    /// at least one warning message, and 2 represents the presence of at least one error message.
    pub fn message_level(&self) -> usize {
        let mut level = 0;
        for message in &self.messages {
            match message.level.as_str() {
                "info" => (),
                "warn" => {
                    if level < 1 {
                        level = 1;
                    }
                }
                "error" => {
                    level = 2;
                    break;
                }
                unsupported => {
                    tracing::warn!("Unsupported message level '{unsupported}'");
                }
            };
        }
        level
    }
}

/// Represents a validation message
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The severity of the message.
    pub level: String,
    /// The rule violation that the message is about.
    pub rule: String,
    /// The contents of the message.
    pub message: String,
}

// Tests

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn test_json_to_row() {
        let json_blob = json!({
            "content": {
                "_id": 1,
                "_order": 1000,
                "_change_id": 0,
                "foo": "FOO",
            }
        });
        let json_row: JsonRow = serde_json::from_value(json_blob).unwrap();
        let row: Row = json_row.into();
        let mut cells = IndexMap::new();
        cells.insert(
            "foo".to_string(),
            Cell {
                value: json!("FOO"),
                text: "FOO".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(
            row,
            Row {
                id: 1,
                order: 1000,
                change_id: 0,
                cells
            }
        )
    }

    #[test]
    fn test_json_to_row_messages() {
        let json_blob = json!({
            "content": {
                "_id": 1,
                "_order": 1000,
                "_change_id": 0,
                "_message": [{
                    "column": "foo",
                    "value": "FOO",
                    "level": "error",
                    "rule": "test rule",
                    "message": "Test message 'FOO'"
                }],
                "foo": "FOO",
            }
        });
        let json_row: JsonRow = serde_json::from_value(json_blob).unwrap();
        let row: Row = json_row.into();
        let mut cells = IndexMap::new();
        cells.insert(
            "foo".to_string(),
            Cell {
                value: json!("FOO"),
                text: "FOO".to_string(),
                messages: vec![Message {
                    level: "error".to_string(),
                    rule: "test rule".to_string(),
                    message: "Test message 'FOO'".to_string(),
                }],
            },
        );
        assert_eq!(
            row,
            Row {
                id: 1,
                order: 1000,
                change_id: 0,
                cells
            }
        )
    }
}
