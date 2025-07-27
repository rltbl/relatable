//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use crate::{self as rltbl};

use anyhow::Result;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use rltbl::{
    core::{Relatable, RelatableError, NEW_ORDER_MULTIPLIER},
    sql::{self, DbKind, DbTransaction, JsonRow, SqlParam},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use std::{collections::HashMap, fmt::Display, str::FromStr};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Table {
    /// The name of the table
    pub name: String,
    /// The name of the view (blank if there is none) to be used when querying the table
    pub view: String,
    /// The id of the most recent change to this table.
    pub change_id: u64,
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

        let table = Table::_get_table(table_name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(table)
    }

    /// Returns a [Table] corresponding to the given table name using the given transaction.
    pub fn _get_table(table_name: &str, tx: &mut DbTransaction<'_>) -> Result<Self> {
        tracing::trace!("Table::_get_table({table_name:?}, tx)");
        // If the default view exists, set the table's view to it, otherwise leave it blank:
        let result = Table::_view_exists(table_name, "default", tx)?;
        let view = {
            if result {
                format!("{table_name}_default_view")
            } else {
                String::from("")
            }
        };

        // Get the last change for this table:
        let statement = format!(
            r#"SELECT MAX("change_id") FROM "history" WHERE "table" = {sql_param}"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([table_name]);
        let change_id = match tx.query_value(&statement, Some(&params))? {
            Some(value) => value.as_u64().unwrap_or_default() as u64,
            None => 0,
        };

        Ok(Table {
            name: table_name.to_string(),
            view,
            change_id,
            columns: Table::_collect_column_info(table_name, tx)?
                .0
                .into_iter()
                .map(|column| (column.name.clone(), column))
                .collect::<IndexMap<_, _>>(),
            ..Default::default()
        })
    }

    /// Drop the given table in the database
    pub async fn drop_table(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::drop_data_tables({self:?}, {rltbl:?})");
        let sql = match rltbl.connection.kind() {
            DbKind::Postgres => {
                format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, self.name)
            }
            DbKind::Sqlite => format!(r#"DROP TABLE IF EXISTS "{}""#, self.name),
        };
        rltbl.connection.query(&sql, None).await?;
        Ok(())
    }

    /// Query the database through the given [Relatable] instance to determine whether the given
    /// table exists.
    pub async fn table_exists(table_name: &str, rltbl: &Relatable) -> Result<bool> {
        tracing::trace!("Table::table_exists({table_name}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let table_exists = Table::_table_exists(table_name, &mut tx)?;

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
                           WHERE "table_type" LIKE {sql_param_1}
                             AND "table_name" = {sql_param_2}
                             AND "table_schema" IN (
                               SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                               FROM "pg_settings"
                               WHERE "name" = 'search_path'
                             )"#,
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

        let view_exists = Table::_view_exists(&self.name, view_type, &mut tx)?;

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
                           AND "table_type" = {sql_param_2}
                           AND "table_schema" IN (
                               SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                               FROM "pg_settings"
                               WHERE "name" = 'search_path'
                           )"#,
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

    /// TODO: Add docstring
    pub async fn get_dependent_tables(
        &self,
        column_name: Option<&str>,
        rltbl: &Relatable,
    ) -> Result<Vec<Self>> {
        // TODO: Add tracing statement
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let tables = self._get_dependent_tables(column_name, &mut tx)?;

        // Commit the transaction:
        tx.commit()?;

        Ok(tables)
    }

    /// TODO: Add docstring
    pub fn _get_dependent_tables(
        &self,
        column: Option<&str>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<Vec<Self>> {
        // TODO: Add tracing statement
        let sql = format!(
            r#"SELECT * FROM "column" WHERE "table" != {sql_param} AND "structure" {is_not} NULL"#,
            sql_param = SqlParam::new(&tx.kind()).next(),
            is_not = sql::is_not_clause(&tx.kind())
        );
        let params = json!([self.name]);
        let mut dependent_tables: Vec<Table> = vec![];
        for row in &tx.query(&sql, Some(&params))? {
            let Structure::From(structure_table, structure_column) =
                Structure::from_str(&row.get_string("structure")?)?;
            if let Some(structure_table) = structure_table {
                if structure_table == self.name {
                    match column {
                        Some(column) if column == structure_column => {
                            let dependent_table = Table::_get_table(&row.get_string("table")?, tx)?;
                            let dependent_column = row.get_string("column")?;
                            let mut indirect_deps = dependent_table
                                ._get_dependent_tables(Some(&dependent_column), tx)?;
                            dependent_tables.push(dependent_table);
                            dependent_tables.append(&mut indirect_deps);
                        }
                        _ => {
                            let dependent_table = Table::_get_table(&row.get_string("table")?, tx)?;
                            let mut indirect_deps =
                                dependent_table._get_dependent_tables(None, tx)?;
                            dependent_tables.push(dependent_table);
                            dependent_tables.append(&mut indirect_deps);
                        }
                    };
                }
            }
        }
        tracing::debug!(
            "Table '{}' has the following dependent tables: {dependent_tables:#?}",
            self.name
        );
        Ok(dependent_tables)
    }

    /// Set the view for the table to the given view type (accepted types are "default" and "text"),
    /// after first ensuring that a view of the given type exists, creating it if necessary.
    pub async fn set_view(&mut self, rltbl: &Relatable, view_type: &str) -> Result<&Self> {
        match view_type {
            "text" => self.ensure_text_view_created(rltbl).await?,
            "default" => self.ensure_default_view_created(rltbl).await?,
            unsupported => {
                tracing::warn!(
                    "Unsupported view name: '{}'. Not changing view '{}' for table '{}",
                    unsupported,
                    self.view,
                    self.name
                );
            }
        };
        Ok(self)
    }

    fn get_id_order_columns(&self, meta_columns: &Vec<Column>) -> (&str, &str) {
        match self.name.as_str() {
            "message" => ("message_id", "message_id"),
            "change" => ("change_id", "change_id"),
            "history" => ("history_id", "history_id"),
            _ => {
                let id_col = match meta_columns.iter().any(|c| c.name == "_id") {
                    false => r#"rowid"#, // This *must* be lowercase.
                    true => r#"_id"#,
                };
                let order_col = match meta_columns.iter().any(|c| c.name == "_order") {
                    false => r#"rowid"#, // This *must* be lowercase.
                    true => r#"_order"#,
                };
                (id_col, order_col)
            }
        }
    }

    /// Use the given [relatable](crate) instance to ensure that the default view for this
    /// table has been created, and then set the view for this table to it.
    pub async fn ensure_default_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::ensure_default_view_created({self:?}, {rltbl:?})");
        let (columns, meta_columns) = Table::collect_column_info(&self.name, rltbl).await?;
        let view_name = format!("{}_default_view", self.name);
        tracing::debug!(r#"Creating default view "{view_name}" with columns {columns:?}"#);

        let (id_col, order_col) = self.get_id_order_columns(&meta_columns);

        for sql in sql::generate_default_view_ddl(
            &self.name,
            id_col,
            order_col,
            &columns,
            &rltbl.connection.kind(),
        ) {
            rltbl.connection.query(&sql, None).await?;
        }

        // Set the table's view name to the default view:
        self.view = view_name;

        Ok(())
    }

    /// Use the given [relatable](crate) instance to ensure that the text view for this
    /// table has been created, and then set the view for this table to it.
    pub async fn ensure_text_view_created(&mut self, rltbl: &Relatable) -> Result<()> {
        tracing::trace!("Table::ensure_text_view_created({self:?}, {rltbl:?})");

        // The default view needs to be created first:
        self.ensure_default_view_created(rltbl).await?;

        // Create the text view:
        let view_name = format!("{}_text_view", self.name);

        let (columns, meta_columns) = Table::collect_column_info(&self.name, rltbl).await?;
        tracing::debug!(r#"Creating text view "{view_name}" with columns {columns:?}"#);
        let (id_col, order_col) = self.get_id_order_columns(&meta_columns);

        for sql in sql::generate_text_view_ddl(
            &self.name,
            id_col,
            order_col,
            &columns,
            &rltbl.connection.kind(),
        ) {
            rltbl.connection.query(&sql, None).await?;
        }

        // Set the table's view name to the text view:
        self.view = view_name;

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

        let columns = Table::_get_column_table_columns(table_name, &mut tx)?;

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
        if !Table::_table_exists("column", tx)? {
            Ok(IndexMap::new())
        } else {
            let sql = match Table::_table_exists("datatype", tx)? {
                true => format!(
                    r#"SELECT
                         c."table",
                         c."column",
                         c."label",
                         c."description",
                         c."nulltype",
                         c."datatype",
                         c."structure",
                         d."description" AS "datatype_description",
                         d."parent" AS "datatype_parent",
                         d."condition" AS "datatype_condition",
                         d."sql_type" AS "datatype_sql_type",
                         d."format" AS "datatype_format"
                       FROM "column" c
                         LEFT JOIN "datatype" d ON c."datatype" = d."datatype"
                       WHERE c."table" = {sql_param}"#,
                    sql_param = SqlParam::new(&tx.kind()).next()
                ),
                false => format!(
                    r#"SELECT * FROM "column" WHERE "table" = {sql_param}"#,
                    sql_param = SqlParam::new(&tx.kind()).next()
                ),
            };
            let params = json!([table_name]);
            let json_columns = tx.query(&sql, Some(&params))?;
            let mut columns = IndexMap::new();
            for json_col in json_columns {
                let datatype = match json_col.get_string("datatype").unwrap_or_default().as_str() {
                    "" => Datatype {
                        name: "text".to_string(),
                        ..Default::default()
                    },
                    datatype if BUILTIN_DATATYPES.contains(&datatype) => {
                        tracing::debug!(
                            "Ignoring datatype table entry for built-in datatype \
                             '{datatype}'"
                        );
                        Datatype::builtin_datatype(datatype)?
                    }
                    datatype => Datatype {
                        name: datatype.to_string(),
                        description: json_col
                            .get_string("datatype_description")
                            .unwrap_or_default(),
                        parent: json_col.get_string("datatype_parent").unwrap_or_default(),
                        condition: json_col
                            .get_string("datatype_condition")
                            .unwrap_or_default(),
                        sql_type: json_col.get_string("datatype_sql_type").unwrap_or_default(),
                        format: json_col.get_string("datatype_format").unwrap_or_default(),
                    },
                };
                let nulltype = match json_col.get_string("nulltype").ok() {
                    None => None,
                    Some(nulltype) if nulltype == "" => None,
                    Some(nulltype) => match Datatype::_get_datatype(&nulltype, tx)? {
                        Some(nulltype) => Some(nulltype),
                        None => {
                            tracing::warn!("Nulltype '{nulltype}' is not a recognized datatype");
                            None
                        }
                    },
                };
                let structure = match json_col.get_string("structure").ok() {
                    None => None,
                    Some(structure) if structure == "" => None,
                    Some(structure) => Some(Structure::from_str(&structure)?),
                };
                let column_name = json_col.get_string("column")?;
                let column = Column {
                    name: column_name.clone(),
                    table: json_col.get_string("table")?,
                    label: json_col.get_string("label").ok(),
                    description: json_col.get_string("description").ok(),
                    datatype_hierarchy: datatype._get_all_ancestors(tx)?,
                    datatype: datatype,
                    nulltype: nulltype,
                    structure: structure,
                    ..Default::default()
                };
                columns.insert(column_name, column);
            }
            tracing::debug!("Retrieved columns from column table: {columns:?}");
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
                    r#"SELECT "name", "type" AS "datatype", "pk"
                       FROM pragma_table_info("{table}") ORDER BY "cid""#
                );
                let mut columns_info = vec![];
                for column_info in tx.query(&sql, None)? {
                    let mut column_info = column_info.clone();
                    if column_info.get_unsigned("pk")? == 1 {
                        // If the column is a primary key then it is also unique:
                        column_info.content.insert("unique".to_string(), json!(1));
                    } else {
                        // If the column is not a primary key, look through the pragma information
                        // for the column to see if it has a unique index (requires two queries).
                        column_info.content.insert("unique".to_string(), json!(0));
                        let sql = format!(
                            r#"SELECT "name", "unique"
                               FROM PRAGMA_INDEX_LIST("{table}")"#
                        );
                        for index_info in tx.query(&sql, None)? {
                            if index_info.get_unsigned("unique")? == 1 {
                                let idx_name = index_info.get_string("name")?;
                                let sql = format!(
                                    r#"SELECT "name" FROM PRAGMA_INDEX_INFO("{idx_name}")"#
                                );
                                if let Some(idx_cname) = tx.query_value(&sql, None)? {
                                    if idx_cname == column_info.get_string("name")? {
                                        column_info.content.insert("unique".to_string(), json!(1));
                                    }
                                }
                            }
                        }
                    }
                    columns_info.push(column_info);
                }
                tracing::debug!(
                    "Retrieved columns from db metadata ({:?}): {columns_info:?}",
                    tx.kind()
                );
                Ok(columns_info)
            }
            DbKind::Postgres => {
                let mut sql_param_gen = SqlParam::new(&tx.kind());
                let sql = format!(
                    r#"WITH "constraints" as (
                         SELECT
                           "kcu"."table_name"::TEXT,
                           "kcu"."column_name"::TEXT,
                           "tco"."constraint_type"::TEXT
                         FROM "information_schema"."table_constraints" "tco"
                         JOIN "information_schema"."key_column_usage" "kcu"
                           ON "kcu"."constraint_name" = "tco"."constraint_name"
                          AND "kcu"."constraint_schema" = "tco"."constraint_schema"
                          AND "kcu"."table_name" = {sql_param_1}
                        WHERE "kcu"."table_schema" IN (
                          SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                          FROM "pg_settings"
                          WHERE "name" = 'search_path'
                        )
                       )
                       SELECT
                         "columns"."column_name"::TEXT AS "name",
                         "columns"."data_type"::TEXT AS "datatype",
                         "constraints"."constraint_type"::TEXT AS "constraint"
                       FROM "information_schema"."columns" "columns"
                         LEFT JOIN "constraints"
                           ON "columns"."table_name" = "constraints"."table_name"
                           AND "columns"."column_name" = "constraints"."column_name"
                       WHERE "columns"."table_schema" IN (
                          SELECT REGEXP_SPLIT_TO_TABLE("setting", ', ')
                          FROM "pg_settings"
                          WHERE "name" = 'search_path'
                        )
                       AND "columns"."table_name" = {sql_param_2}
                       ORDER BY "columns"."ordinal_position""#,
                    sql_param_1 = sql_param_gen.next(),
                    sql_param_2 = sql_param_gen.next()
                );
                let params = json!([table, table]);

                let mut columns_info = vec![];
                for row in tx.query(&sql, Some(&params))? {
                    let mut column_info = JsonRow::new();
                    column_info
                        .content
                        .insert("name".to_string(), row.get_value("name")?);
                    column_info
                        .content
                        .insert("datatype".to_string(), row.get_value("datatype")?);
                    match row.get_string("constraint") {
                        Ok(constraint) if constraint == "PRIMARY KEY" => {
                            column_info.content.insert("pk".to_string(), json!(1));
                            column_info.content.insert("unique".to_string(), json!(1));
                        }
                        Ok(constraint) if constraint == "UNIQUE" => {
                            column_info.content.insert("pk".to_string(), json!(0));
                            column_info.content.insert("unique".to_string(), json!(1));
                        }
                        Ok(constraint) if constraint == "FOREIGN KEY" => {
                            column_info.content.insert("pk".to_string(), json!(0));
                            column_info.content.insert("unique".to_string(), json!(0));
                        }
                        Ok(constraint) if constraint == "" => {
                            column_info.content.insert("pk".to_string(), json!(0));
                            column_info.content.insert("unique".to_string(), json!(0));
                        }
                        Ok(unrecognized) => {
                            tracing::warn!("Unrecognized constraint type '{unrecognized}'");
                            column_info.content.insert("pk".to_string(), json!(0));
                            column_info.content.insert("unique".to_string(), json!(0));
                        }
                        Err(err) => {
                            tracing::warn!("Error geting constraint for column: '{err}'");
                            column_info.content.insert("pk".to_string(), json!(0));
                            column_info.content.insert("unique".to_string(), json!(0));
                        }
                    };
                    columns_info.push(column_info);
                }
                tracing::debug!(
                    "Retrieved columns from db metadata ({:?}): {columns_info:?}",
                    tx.kind()
                );
                Ok(columns_info)
            }
        }
    }

    /// Returns a tuple whose first position contains a list of the given table's columns, and whose
    /// second position contains a list of the given table's metacolumns
    pub async fn collect_column_info(
        table: &str,
        rltbl: &Relatable,
    ) -> Result<(Vec<Column>, Vec<Column>)> {
        tracing::trace!("Table::collect_column_info({table}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        // Begin a transaction:
        let mut tx = rltbl.connection.begin(&mut conn).await?;

        let columns = Table::_collect_column_info(table, &mut tx)?;

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
        let column_columns = Table::_get_column_table_columns(table_name, tx)?;

        // Get the table's columns from the database and merge it with the information from the
        // column table that we just collected:
        let mut columns = vec![];
        let mut meta_columns = vec![];
        let meta_datatype = Datatype::builtin_datatype("integer")?;
        let meta_datatype_hierarchy = meta_datatype._get_all_ancestors(tx)?;
        for db_column in Table::get_db_table_columns(table_name, tx)? {
            match db_column.get_string("name")? {
                column_name if column_name.starts_with("_") => meta_columns.push(Column {
                    name: column_name,
                    table: table_name.to_string(),
                    primary_key: db_column.get_unsigned("pk")? == 1,
                    unique: db_column.get_unsigned("unique")? == 1,
                    datatype: meta_datatype.clone(),
                    datatype_hierarchy: meta_datatype_hierarchy.clone(),
                    ..Default::default()
                }),
                column_name => {
                    // Fall back to the SQL type (these are returned for each column from
                    // get_db_table_columns()) if no datatype is defined in the column table
                    // or the column table does not exist:
                    let datatype = match column_columns.get(&column_name) {
                        None => {
                            let db_datatype = match db_column.get_string("datatype")? {
                                datatype if datatype == "" => "text".to_string(),
                                datatype => datatype,
                            };
                            Datatype {
                                name: db_datatype.to_lowercase(),
                                ..Default::default()
                            }
                        }
                        Some(col) => col.datatype.clone(),
                    };
                    columns.push(Column {
                        label: column_columns
                            .get(&column_name)
                            .and_then(|col| col.label.clone()),
                        description: column_columns
                            .get(&column_name)
                            .and_then(|col| col.description.clone()),
                        nulltype: column_columns
                            .get(&column_name)
                            .and_then(|col| col.nulltype.clone()),
                        datatype_hierarchy: datatype._get_all_ancestors(tx)?,
                        datatype: datatype,
                        structure: column_columns
                            .get(&column_name)
                            .and_then(|col| col.structure.clone()),
                        name: column_name,
                        table: table_name.to_string(),
                        primary_key: db_column.get_unsigned("pk")? == 1,
                        unique: db_column.get_unsigned("unique")? == 1,
                    })
                }
            };
        }
        if columns.is_empty() && meta_columns.is_empty() {
            tracing::info!("No column information found for: {}", table_name);
        }
        tracing::debug!(
            "Combined columns info from db metadata and column table: \
             Normal columns: {columns:?}, Metacolumns: {meta_columns:?}"
        );
        Ok((columns, meta_columns))
    }

    /// Returns a list of the table's primary key columns.
    pub async fn primary_key_columns(table: &str, rltbl: &Relatable) -> Result<Vec<Column>> {
        let (mut columns, mut meta_columns) = Table::collect_column_info(table, rltbl).await?;
        columns.append(&mut meta_columns);
        Ok(columns
            .into_iter()
            .filter(|col| col.primary_key)
            .collect::<Vec<_>>())
    }

    /// Returns a list of the table's primary key columns.
    pub fn _primary_key_columns(table: &str, tx: &mut DbTransaction<'_>) -> Result<Vec<Column>> {
        let (mut columns, mut meta_columns) = Table::_collect_column_info(table, tx)?;
        columns.append(&mut meta_columns);
        Ok(columns
            .into_iter()
            .filter(|col| col.primary_key)
            .collect::<Vec<_>>())
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
            "datatype" => Some(col.datatype.name.to_string()),
            "nulltype" => match &col.nulltype {
                None => None,
                Some(nulltype) => Some(nulltype.name.clone()),
            },
            _ => None,
        })
    }

    /// Return a [JsonRow] representing the given row of the given table, using the
    /// given transaction.
    pub fn _get_row(table: &str, row: u64, tx: &mut DbTransaction<'_>) -> Result<Option<JsonRow>> {
        tracing::trace!("Table::_get_row({table:}?, {row}, tx)");
        let sql = format!(
            r#"SELECT * FROM "{table}" WHERE "_id" = {sql_param}"#,
            sql_param = SqlParam::new(&tx.kind()).next()
        );
        let params = json!([row]);
        tx.query_one(&sql, Some(&params))
    }

    /// Determine what the next created row id for the given table will be
    pub async fn get_next_id(&self, rltbl: &Relatable) -> Result<u64> {
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
    pub fn _get_next_id(&self, tx: &mut DbTransaction<'_>) -> Result<u64> {
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
            Some(value) => value.as_u64().unwrap_or_default() as u64,
            None => 0,
        };
        Ok(current_row_id + 1)
    }

    /// Returns the row id that comes before the given row in the given table, using the given
    /// transaction.
    pub fn _get_previous_row_id(table: &str, row: u64, tx: &mut DbTransaction<'_>) -> Result<u64> {
        tracing::trace!("Table::_get_previous_row_id({table}, {row}, tx)");
        let curr_row_order = Table::_get_row_order(table, row, tx)?;
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
    fn _get_row_order(table: &str, row: u64, tx: &mut DbTransaction<'_>) -> Result<u64> {
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
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub table: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub primary_key: bool,
    pub unique: bool,
    pub datatype: Datatype,
    pub datatype_hierarchy: Vec<Datatype>,
    pub nulltype: Option<Datatype>,
    pub structure: Option<Structure>,
}

lazy_static! {
    /// Relatable's core built-in datatypes
    pub static ref BUILTIN_DATATYPES: Vec<&'static str> =
        vec!["text", "empty", "line", "trimmed_line", "nonspace", "word", "integer"];
}

/// Represents a column's datatype
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Datatype {
    pub name: String,
    pub description: String,
    pub parent: String,
    pub condition: String,
    pub sql_type: String,
    pub format: String,
}

impl Datatype {
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
            let sql_type = match self.name.to_lowercase().as_str() {
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
                datatype if BUILTIN_DATATYPES.contains(&datatype) => "TEXT",
                unknown => {
                    tracing::warn!("Cannot infer SQL type for unknown datatype '{unknown}'");
                    "TEXT"
                }
            };
            sql_type.to_string()
        }
    }

    /// Return a Datatype struct corresponding to the given built-in datatype
    pub fn builtin_datatype(datatype: &str) -> Result<Self> {
        tracing::trace!("Datatype::builtin_datatype({datatype})");
        let builtins = Datatype::builtin_datatypes();
        let builtin = match datatype {
            "text" => builtins.get("text").expect("Builtin 'text' not found"),
            "empty" => builtins.get("empty").expect("Builtin 'empty' not found"),
            "line" => builtins.get("line").expect("Builtin 'line' not found"),
            "trimmed_line" => builtins
                .get("trimmed_line")
                .expect("Builtin 'trimmed_line' not found"),
            "nonspace" => builtins
                .get("nonspace")
                .expect("Builtin 'nonspace' not found"),
            "word" => builtins.get("word").expect("Builtin 'word' not found"),
            "integer" => builtins
                .get("integer")
                .expect("Builtin 'integer' not found"),
            unrecognized => {
                return Err(RelatableError::InputError(format!(
                    "Unrecognized built-in datatype: '{unrecognized}'"
                ))
                .into())
            }
        };
        Ok(builtin.to_owned())
    }

    // Returns a [HashMap] representing all of the built-in datatypes, indexed by datatype name
    pub fn builtin_datatypes() -> HashMap<String, Self> {
        tracing::trace!("Datatype::builtin_datatypes()");
        [
            (
                "text".into(),
                Datatype {
                    name: "text".to_string(),
                    description: "any text".to_string(),
                    ..Default::default()
                },
            ),
            (
                "empty".into(),
                Datatype {
                    name: "empty".to_string(),
                    description: "the empty string".to_string(),
                    parent: "text".to_string(),
                    condition: "equals('')".to_string(),
                    ..Default::default()
                },
            ),
            (
                "line".into(),
                Datatype {
                    name: "line".to_string(),
                    description: "a line of text".to_string(),
                    parent: "text".to_string(),
                    // TODO: Add the right condition here once implemented.
                    condition: "".to_string(),
                    ..Default::default()
                },
            ),
            (
                "trimmed_line".into(),
                Datatype {
                    name: "trimmed_line".to_string(),
                    description: "a line of text that deos not begin or end with whitespace"
                        .to_string(),
                    parent: "line".to_string(),
                    // TODO: Add the right condition here once implemented.
                    condition: "".to_string(),
                    ..Default::default()
                },
            ),
            (
                "nonspace".into(),
                Datatype {
                    name: "nonspace".to_string(),
                    description: "text without whitespace".to_string(),
                    parent: "trimmed_line".to_string(),
                    // TODO: Add the right condition here once implemented.
                    condition: "".to_string(),
                    ..Default::default()
                },
            ),
            (
                "word".into(),
                Datatype {
                    name: "word".to_string(),
                    description: "a single word: letters, numbers, underscore".to_string(),
                    parent: "nonspace".to_string(),
                    // TODO: Add the right condition here once implemented.
                    condition: "".to_string(),
                    ..Default::default()
                },
            ),
            (
                "integer".into(),
                Datatype {
                    name: "integer".to_string(),
                    description: "an integer".to_string(),
                    parent: "nonspace".to_string(),
                    sql_type: "INTEGER".to_string(),
                    // TODO: Add the right condition here once implemented.
                    condition: "".to_string(),
                    ..Default::default()
                },
            ),
        ]
        .into_iter()
        .collect::<HashMap<_, _>>()
    }

    /// Get all of the datatypes in the database
    pub async fn get_all_datatypes(rltbl: &Relatable) -> Result<HashMap<String, Self>> {
        tracing::trace!("Datatype::get_all_datatypes({rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let datatypes = Datatype::_get_all_datatypes(&mut tx)?;
        tx.commit()?;
        Ok(datatypes)
    }

    /// Get all of the datatypes in the database using the given transaction
    fn _get_all_datatypes(tx: &mut DbTransaction<'_>) -> Result<HashMap<String, Self>> {
        tracing::trace!("Datatype::_get_all_datatypes(tx)");
        let mut datatypes = Datatype::builtin_datatypes();
        if Table::_table_exists("datatype", tx)? {
            let sql = r#"SELECT * FROM "datatype""#;
            let datatype_rows = tx.query(&sql, None)?;
            for dt_row in &datatype_rows {
                let dt_name = dt_row.get_string("datatype")?;
                datatypes.insert(
                    dt_name.to_string(),
                    Datatype {
                        name: dt_name,
                        description: dt_row.get_string("description")?,
                        parent: dt_row.get_string("parent")?,
                        condition: dt_row.get_string("condition")?,
                        sql_type: dt_row.get_string("sql_type")?,
                        format: dt_row.get_string("format")?,
                    },
                );
            }
        }
        Ok(datatypes)
    }

    /// Get the given [Datatype] from the database
    pub async fn get_datatype(datatype: &str, rltbl: &Relatable) -> Result<Option<Self>> {
        tracing::trace!("Datatype::get_datatype({datatype}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let datatype = Datatype::_get_datatype(datatype, &mut tx)?;
        tx.commit()?;
        Ok(datatype)
    }

    fn _get_datatype(datatype: &str, tx: &mut DbTransaction<'_>) -> Result<Option<Self>> {
        tracing::trace!("Datatype::_get_datatype({datatype}, tx)");
        let datatypes = Datatype::_get_all_datatypes(tx)?;
        match datatypes.get(datatype) {
            Some(datatype) => Ok(Some(datatype.to_owned())),
            None => {
                tracing::warn!("No datatype '{datatype}' found");
                Ok(None)
            }
        }
    }

    /// Get all of this datatype's ancestors
    pub async fn get_all_ancestors(&self, rltbl: &Relatable) -> Result<Vec<Self>> {
        tracing::trace!("Datatype::get_all_ancestors({self:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let ancestors = self._get_all_ancestors(&mut tx)?;
        tx.commit()?;
        Ok(ancestors)
    }

    /// Get all of this datatype's ancestors using the given transaction.
    fn _get_all_ancestors(&self, tx: &mut DbTransaction<'_>) -> Result<Vec<Self>> {
        tracing::trace!("Datatype::_get_all_ancestors({self:?}, tx)");
        let datatypes = {
            let mut datatypes = Datatype::builtin_datatypes()
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_owned()))
                .collect::<HashMap<_, _>>();
            if Table::_table_exists("datatype", tx)? {
                let builtin_names = datatypes.keys().cloned().collect::<Vec<_>>();
                let sql = r#"SELECT * from "datatype""#;
                for row in tx.query(sql, None)? {
                    let dt_name = row.get_string("datatype")?;
                    if builtin_names.contains(&dt_name) {
                        tracing::info!("Ignoring redefinition of built-in datatype '{dt_name}'");
                    } else {
                        datatypes.insert(
                            dt_name.to_string(),
                            Datatype {
                                name: dt_name,
                                description: row.get_string("description").unwrap_or_default(),
                                parent: row.get_string("parent").unwrap_or_default(),
                                condition: row.get_string("condition").unwrap_or_default(),
                                sql_type: row.get_string("sql_type").unwrap_or_default(),
                                format: row.get_string("format").unwrap_or_default(),
                            },
                        );
                    }
                }
            }
            datatypes
        };

        fn build_hierarchy(
            dt_map: &HashMap<String, Datatype>,
            start_dt_name: &str,
            dt_name: &str,
        ) -> Result<Vec<Datatype>> {
            tracing::trace!(
                "Datatype::get_all_ancestors()::build_hierarchy({dt_map:?}, {start_dt_name}, \
                 {dt_name})"
            );
            let mut datatypes = vec![];
            if dt_name != "" {
                let datatype = match dt_map.get(dt_name) {
                    Some(datatype) => datatype,
                    None => {
                        tracing::warn!("Undefined datatype '{dt_name}'");
                        return Ok(datatypes);
                    }
                };
                let dt_name = datatype.name.as_str();
                let dt_parent = datatype.parent.as_str();
                if dt_name != start_dt_name {
                    datatypes.push(datatype.clone());
                }
                let mut more_datatypes = build_hierarchy(dt_map, start_dt_name, &dt_parent)?;
                datatypes.append(&mut more_datatypes);
            }
            Ok(datatypes)
        }

        build_hierarchy(&datatypes, &self.name, &self.name)
    }

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
        let table_name = column.table.as_str();
        let column_name = column.name.as_str();
        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let mut messages_were_added = false;
        match self.condition.as_str() {
            "" => (),
            condition if condition.starts_with("equals(") => {
                let re = regex::Regex::new(r"equals\((.+?)\)")?;
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
                             "{column_name}" AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE "{column_name}" != {sql_param_5}"#,
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
                                table_name,
                                column_name,
                                format!("datatype:{}", column.datatype.name),
                                format!("{column_name} must be a {}", column.datatype.name),
                                condition,
                                row
                            ]);
                        }
                        None => {
                            params = json!([
                                table_name,
                                column_name,
                                format!("datatype:{}", column.datatype.name),
                                format!("{column_name} must be a {}", column.datatype.name),
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
                let re = regex::Regex::new(r"in\((.+?)\)").unwrap();
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
                             "{column_name}" AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{table_name}"
                           WHERE "{column_name}" NOT IN ({sql_param_5})"#,
                        sql_param_1 = sql_param_gen.next(),
                        sql_param_2 = sql_param_gen.next(),
                        sql_param_3 = sql_param_gen.next(),
                        sql_param_4 = sql_param_gen.next(),
                        sql_param_5 = sql_param_gen.get_as_list(condition_list.len()),
                    );
                    let mut params = json!([
                        table_name,
                        column_name,
                        format!("datatype:{}", column.datatype.name),
                        format!("{column_name} must be a {}", column.datatype.name),
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
            invalid => tracing::warn!("Unrecognized datatype condition '{invalid}'"),
        };

        tracing::debug!(
            "Validated datatype '{}' for column '{}.{}' (row: {:?}) {}",
            self.name,
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

/// Represents a column's structure.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Structure {
    From(Option<String>, String),
}

impl Structure {
    /// TODO: Add docstring here
    pub fn validate(
        &self,
        column: &Column,
        row: Option<&u64>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<bool> {
        // TODO: Add tracing statement here

        let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
        let mut messages_were_added = false;
        match self {
            Structure::From(s_table, s_column) => {
                let c_table = &column.table;
                let c_column = &column.name;
                let s_table = match s_table {
                    None => c_table,
                    Some(s_table) => s_table,
                };
                let s_table = unquoted_re.replace(&s_table, "$unquoted").to_string();
                let s_column = unquoted_re.replace(&s_column, "$unquoted").to_string();
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
                             "{c_column}" AS "value",
                             'error' AS "level",
                             {sql_param_3} AS "rule",
                             {sql_param_4} AS "message"
                           FROM "{c_table}"
                           WHERE "{c_column}" NOT IN (
                               SELECT "{s_column}" FROM "{s_table}"
                           )"#,
                    sql_param_1 = sql_param_gen.next(),
                    sql_param_2 = sql_param_gen.next(),
                    sql_param_3 = sql_param_gen.next(),
                    sql_param_4 = sql_param_gen.next(),
                );
                let params;
                match row {
                    Some(row) => {
                        sql.push_str(&format!(
                            r#" AND "_id" = {sql_param}"#,
                            sql_param = sql_param_gen.next()
                        ));
                        params = json!([
                            c_table,
                            c_column,
                            format!("key:foreign"),
                            format!("{c_column} must be in {s_table}.{s_column}"),
                            row
                        ]);
                    }
                    None => {
                        params = json!([
                            c_table,
                            c_column,
                            format!("key:foreign"),
                            format!("{c_column} must be in {s_table}.{s_column}"),
                        ]);
                    }
                };
                sql.push_str(r#" RETURNING 1 AS "inserted""#);
                if let Some(_) = tx.query_one(&sql, Some(&params))? {
                    messages_were_added = true;
                }
            }
        };

        tracing::debug!(
            "Validated structure '{}' for column '{}.{}' (row: {:?}) {}",
            self,
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

impl FromStr for Structure {
    type Err = anyhow::Error;

    fn from_str(structure: &str) -> Result<Self> {
        tracing::trace!("Structure::from_str({structure})");
        if structure.starts_with("from(") {
            let re = regex::Regex::new(r"from\(((.+?)\.)?(.+?)\)")?;
            let unquoted_re = regex::Regex::new(r#"^['"](?P<unquoted>.*)['"]$"#)?;
            match re.captures(structure) {
                Some(captures) => {
                    let table = &captures.get(2).and_then(|t| Some(t.as_str()));
                    let table = match table {
                        Some(table) => Some(unquoted_re.replace(table, "$unquoted").to_string()),
                        None => None,
                    };
                    let column = &captures[3];
                    let column = unquoted_re.replace(column, "$unquoted").to_string();
                    Ok(Structure::From(table, column))
                }
                None => {
                    return Err(RelatableError::InputError(format!(
                        "Invalid from() structure: '{structure}'"
                    ))
                    .into());
                }
            }
        } else {
            return Err(
                RelatableError::InputError(format!("Invalid structure: '{structure}'")).into(),
            );
        }
    }
}

impl Display for Structure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Structure::From(s_table, s_column) => match s_table {
                None => write!(f, "from({s_column})"),
                Some(s_table) => write!(f, "from({s_table}.{s_column})"),
            },
        }
    }
}

/// Represents a row from some table
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub id: u64,
    pub order: u64,
    pub change_id: u64,
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
        row.order = NEW_ORDER_MULTIPLIER as u64 * row.id;
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
    pub fn validate_sql_types(
        &mut self,
        table: &Table,
        tx: &mut DbTransaction<'_>,
    ) -> Result<&Self> {
        for (column, cell) in self.cells.iter_mut() {
            let column_details = table.get_config_for_column(column);
            cell.validate_sql_type(&column_details)?;
            for message in cell.messages.iter() {
                let (msg_id, msg) = Relatable::_add_message(
                    "rltbl",
                    &table.name,
                    &self.id,
                    column,
                    &cell.value,
                    &message.level,
                    &message.rule,
                    &message.message,
                    tx,
                )?;
                tracing::debug!("Added message (ID {msg_id}): {msg:?}");
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
            .unwrap_or_default() as u64;
        let order = row
            .content
            .get("_order")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as u64;
        let change_id = row
            .content
            .get("_change_id")
            .and_then(|i| i.as_u64())
            .unwrap_or_default() as u64;
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
                    let message: Message = match serde_json::from_value(message.to_owned()) {
                        Ok(message) => message,
                        Err(err) => {
                            tracing::warn!(
                                "Unable to parse message '{message}' due to error '{err}'"
                            );
                            continue;
                        }
                    };
                    if let Some(cell) = cells.get(column) {
                        let mut new_cell = cell.clone();
                        new_cell.value = message.value.clone();
                        new_cell.text = sql::json_to_string(&new_cell.value);
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
    pub fn validate_sql_type(&mut self, column: &Column) -> Result<&Self> {
        tracing::trace!("Cell::validate_sql_type({self:?}, {column:?})");

        fn invalidate(cell: &mut Cell, column: &Column) {
            let datatype = &column.datatype.name;
            cell.messages.push(Message {
                value: cell.value.clone(),
                level: "error".to_string(),
                rule: format!("sql_type:{datatype}"),
                message: format!("{column} must be of type {datatype}", column = column.name),
            });
        }

        match column
            .datatype
            .infer_sql_type(&column.datatype_hierarchy)
            .as_str()
        {
            "INTEGER" => match &mut self.value {
                JsonValue::Number(number) => match number.to_string().parse::<i64>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, column),
            },
            "REAL" | "NUMERIC" => match &mut self.value {
                JsonValue::Number(number) => match number.to_string().parse::<f64>() {
                    Ok(_) => (),
                    Err(_) => invalidate(self, column),
                },
                JsonValue::Null => (),
                _ => invalidate(self, column),
            },
            "TEXT" => (),
            unsupported => {
                return Err(RelatableError::InputError(format!(
                    "Unsupported SQL type: '{unsupported}'"
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

    /// Determine whether this cell contains a SQL type error.
    pub fn has_sql_type_error(&self) -> bool {
        self.messages
            .iter()
            .filter(|m| m.level == "error" && m.rule.starts_with("sql_type:"))
            .collect::<Vec<_>>()
            .len()
            > 0
    }
}

/// Represents a validation message
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// The value referred to by the message
    pub value: JsonValue,
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
                    value: json!("FOO"),
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
