//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[table](crate::table)).

use crate::{
    column::Column,
    core::{Relatable, RelatableError},
    datatype::{Datatype, Datatypes},
    sql::{self, DbKind, DbTransaction, JsonRow, SqlParam},
    structure::Structure,
};

use anyhow::Result;
use indexmap::IndexMap;
use rltbl_db::core::DbQuery;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::str::FromStr;

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
        tracing::info!("Dropped table '{}'", self.name);
        rltbl.pool.execute(&sql, &[]).await?;
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

    /// Get the tables that depend on this table. If `column_name` is specified, only get the
    /// tables that depend on this particular column.
    pub async fn get_dependent_tables(
        &self,
        column: Option<&str>,
        rltbl: &Relatable,
    ) -> Result<Vec<Self>> {
        tracing::trace!("Table::get_dependent_tables({self:?}, {column:?}, {rltbl:?})");
        let mut conn = rltbl.connection.reconnect()?;
        let mut tx = rltbl.connection.begin(&mut conn).await?;
        let tables = self._get_dependent_tables(column, &mut tx)?;
        tx.commit()?;
        Ok(tables)
    }

    /// Get the tables that depend on this table, using the given transaction. If `column_name`
    /// is specified, only get the tables that depend on this particular column.
    pub fn _get_dependent_tables(
        &self,
        column: Option<&str>,
        tx: &mut DbTransaction<'_>,
    ) -> Result<Vec<Self>> {
        tracing::trace!("Table::get_dependent_tables({self:?}, {column:?}, tx)");
        if !Table::_table_exists("column", tx)? {
            return Ok(vec![]);
        }

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
            rltbl.pool.execute(&sql, &[]).await?;
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
            rltbl.pool.execute(&sql, &[]).await?;
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
            // TODO: replace with DatatypeTable::get(db)
            let builtin_datatypes = Datatypes::builtins();
            for json_col in json_columns {
                let datatype = match json_col.get_string("datatype").unwrap_or_default().as_str() {
                    "" => Datatype {
                        datatype: "text".to_string(),
                        ..Default::default()
                    },
                    datatype if builtin_datatypes.contains_key(datatype) => {
                        tracing::debug!(
                            "Ignoring datatype table entry for built-in datatype \
                             '{datatype}'"
                        );
                        builtin_datatypes.get(datatype).unwrap().clone()
                    }
                    datatype => Datatype {
                        datatype: datatype.to_string(),
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
                    Some(nulltype) => match builtin_datatypes.get(&nulltype) {
                        Some(nulltype) => Some(nulltype.clone()),
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
                    // TODO: remove this field
                    datatype_hierarchy: builtin_datatypes
                        .ancestors(&datatype)
                        .into_iter()
                        .cloned()
                        .collect(),
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
    pub fn get_db_table_columns(table: &str, tx: &mut DbTransaction<'_>) -> Result<Vec<JsonRow>> {
        tracing::trace!("Table::_get_db_table_columns({table:?}, tx)");
        match tx.kind() {
            DbKind::Sqlite => {
                let sql = format!(
                    r#"SELECT "name", "type" AS "datatype", "pk"
                       FROM pragma_table_info({sql_param}) ORDER BY "cid""#,
                    sql_param = SqlParam::new(&tx.kind()).next()
                );
                let mut columns_info = vec![];
                let params = json!([table]);
                for column_info in tx.query(&sql, Some(&params))? {
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
                               FROM PRAGMA_INDEX_LIST({sql_param})"#,
                            sql_param = SqlParam::new(&tx.kind()).next()
                        );
                        let params = json!([table]);
                        for index_info in tx.query(&sql, Some(&params))? {
                            if index_info.get_unsigned("unique")? == 1 {
                                let idx_name = index_info.get_string("name")?;
                                let sql = format!(
                                    r#"SELECT "name" FROM PRAGMA_INDEX_INFO({sql_param})"#,
                                    sql_param = SqlParam::new(&tx.kind()).next()
                                );
                                let params = json!([idx_name]);
                                if let Some(idx_cname) = tx.query_value(&sql, Some(&params))? {
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
        let builtin_datatypes = Datatypes::builtins();
        let meta_datatype = builtin_datatypes.get("integer").unwrap();
        let meta_datatype_hierarchy: Vec<Datatype> = builtin_datatypes
            .ancestors(meta_datatype)
            .into_iter()
            .cloned()
            .collect();
        for db_column in Table::get_db_table_columns(table_name, tx)? {
            match db_column.get_string("name")? {
                column_name if column_name.starts_with("_") => meta_columns.push(Column {
                    name: column_name,
                    table: table_name.to_string(),
                    primary_key: db_column.get_unsigned("pk")? == 1,
                    unique: db_column.get_unsigned("unique")? == 1,
                    datatype: meta_datatype.clone(),
                    // TODO: drop this field
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
                                datatype: db_datatype.to_lowercase(),
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
                        datatype_hierarchy: builtin_datatypes
                            .ancestors(&datatype)
                            .into_iter()
                            .cloned()
                            .collect(),
                        datatype: datatype,
                        structure: column_columns
                            .get(&column_name)
                            .and_then(|col| col.structure.clone()),
                        name: column_name,
                        table: table_name.to_string(),
                        primary_key: db_column.get_unsigned("pk")? == 1,
                        unique: db_column.get_unsigned("unique")? == 1,
                        ..Default::default()
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
            "datatype" => Some(col.datatype.datatype.to_string()),
            "nulltype" => match &col.nulltype {
                None => None,
                Some(nulltype) => Some(nulltype.datatype.clone()),
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
