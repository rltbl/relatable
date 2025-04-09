//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[sql](crate::sql)).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! any elements of the API that are database-specific.

use crate as rltbl;
use rltbl::core::{Column, RelatableError, Table, MOVE_INTERVAL};

use anyhow::Result;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

#[cfg(feature = "sqlx")]
use std::str::FromStr as _;

#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use async_std::task::block_on;

#[cfg(feature = "sqlx")]
use sqlx::{
    any::{AnyConnectOptions, AnyPoolOptions},
    Acquire as _, Column as _, Row as _,
};

// In principle SQL_PARAM can be set to any arbitrary sequence of non-word characters. If you would
// like SQL_PARAM to be a word then you must also modify SQL_PARAM_REGEX correspondingly. See the
// comment beside it, below, for instructions on how to do that.
/// The placeholder to use for query parameters when binding using sqlx. Currently set to "?",
/// which corresponds to SQLite's parameter syntax. To convert SQL to postgres, use the function
/// [local_sql_syntax()].
pub static SQL_PARAM: &str = "VALVEPARAM";

lazy_static! {
    // This accepts a non-word SQL_PARAM unless it is enclosed in quotation marks. To use a word
    // SQL_PARAM change '\B' to '\b' below.
    /// Regular expression used to find the next instance of [SQL_PARAM] in a given SQL statement.
    pub static ref SQL_PARAM_REGEX: Regex = Regex::new(&format!(
        r#"('[^'\\]*(?:\\.[^'\\]*)*'|"[^"\\]*(?:\\.[^"\\]*)*")|\b{}\b"#,
        SQL_PARAM
    ))
    .unwrap();
}

/// Represents a 'simple' database name
pub static DB_OBJECT_MATCH_STR: &str = r"^[\w_]+$";

lazy_static! {
    pub static ref DB_OBJECT_REGEX: Regex = Regex::new(DB_OBJECT_MATCH_STR).unwrap();
}

/// Maximum number of database connections.
pub static MAX_DB_CONNECTIONS: u32 = 5;

/// The [maximum number of parameters](https://www.sqlite.org/limits.html#max_variable_number)
/// that can be bound to a SQLite query
pub static MAX_PARAMS_SQLITE: usize = 32766;

/// The [maximum number of parameters](https://www.postgresql.org/docs/current/limits.html)
/// that can be bound to a Postgres query
pub static MAX_PARAMS_POSTGRES: usize = 65535;

/// Represents the kind of database being managed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbKind {
    Postgres,
    Sqlite,
}

#[derive(Debug)]
pub enum DbActiveConnection {
    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Connection),
}

#[derive(Debug)]
pub enum DbConnection {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::AnyPool, DbKind),

    #[cfg(feature = "rusqlite")]
    Rusqlite(String),
}

impl DbConnection {
    pub fn kind(&self) -> DbKind {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, kind) => *kind,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => DbKind::Sqlite,
        }
    }

    pub async fn connect(database: &str) -> Result<(Self, Option<DbActiveConnection>)> {
        let is_postgresql = database.starts_with("postgresql://");
        match is_postgresql {
            true => {
                #[cfg(not(feature = "sqlx"))]
                return Err(RelatableError::InputError(
                    "rltbl was built without the sqlx feature, which is required for PostgreSQL \
                     support. To build rltbl with sqlx enabled, run \
                     `cargo build --features sqlx`"
                        .to_string(),
                )
                .into());

                #[cfg(feature = "sqlx")]
                {
                    sqlx::any::install_default_drivers();
                    let connection_options;
                    let db_kind;
                    if database.starts_with("postgresql://") {
                        connection_options = AnyConnectOptions::from_str(database)?;
                        db_kind = DbKind::Postgres;
                    } else {
                        let connection_string;
                        if !database.starts_with("sqlite://") {
                            connection_string = format!("sqlite://{}?mode=rwc", database);
                        } else {
                            connection_string = database.to_string();
                        }
                        connection_options =
                            AnyConnectOptions::from_str(connection_string.as_str())?;
                        db_kind = DbKind::Sqlite;
                    }

                    let pool = AnyPoolOptions::new()
                        .max_connections(MAX_DB_CONNECTIONS)
                        .connect_with(connection_options)
                        .await?;

                    let connection = Self::Sqlx(pool, db_kind);
                    Ok((connection, None))
                }
            }
            false => {
                // We suppress warnings for unused variables for this particular variable because
                // the compiler is becoming confused about which variables have been actually used
                // as a result of the conditional sqlx/rusqlite compilation (or maybe the programmer
                // is confused).
                #[allow(unused_variables)]
                #[cfg(feature = "rusqlite")]
                let tuple = (
                    Self::Rusqlite(database.to_string()),
                    Some(DbActiveConnection::Rusqlite(rusqlite::Connection::open(
                        database,
                    )?)),
                );

                #[cfg(feature = "sqlx")]
                let tuple = {
                    let url = {
                        if database.starts_with("sqlite://") {
                            database.to_string()
                        } else {
                            format!("sqlite://{database}?mode=rwc")
                        }
                    };
                    let kind = {
                        if url.starts_with("sqlite://") {
                            DbKind::Sqlite
                        } else {
                            DbKind::Postgres
                        }
                    };
                    sqlx::any::install_default_drivers();
                    let connection = Self::Sqlx(sqlx::AnyPool::connect(&url).await?, kind);
                    (connection, None)
                };

                Ok(tuple)
            }
        }
    }

    pub fn reconnect(&self) -> Result<Option<DbActiveConnection>> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, _) => Ok(None),
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(path) => Ok(Some(DbActiveConnection::Rusqlite(
                rusqlite::Connection::open(path)?,
            ))),
        }
    }

    pub async fn begin<'a>(
        &self,
        conn: &'a mut Option<DbActiveConnection>,
    ) -> Result<DbTransaction<'a>> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(pool, kind) => {
                let tx = pool.begin().await?;
                Ok(DbTransaction::Sqlx(tx, *kind))
            }
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => match conn {
                None => {
                    return Err(RelatableError::InputError(
                        "Can't begin Rusqlite transaction: No connection provided".to_string(),
                    )
                    .into())
                }
                Some(DbActiveConnection::Rusqlite(ref mut conn)) => {
                    let tx = conn.transaction()?;
                    Ok(DbTransaction::Rusqlite(tx))
                }
            },
        }
    }

    // Given a connection and a SQL string, return a vector of JsonRows.
    // This is intended as a low-level function that abstracts over the SQL engine,
    // and whatever result types it returns.
    // Since it uses a vector, statements should be limited to a sane number of rows.
    pub async fn query(&self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        if !valid_params(params) {
            tracing::warn!("invalid parameter argument");
            return Ok(vec![]);
        }
        let statement = match self.kind() {
            DbKind::Sqlite => statement,
            DbKind::Postgres => &local_sql_syntax(&self.kind(), statement),
        };
        self.query_direct(&statement, params).await
    }

    pub async fn query_direct(
        &self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Vec<JsonRow>> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(pool, _) => {
                let query = prepare_sqlx_query(&statement, params)?;
                let mut rows = vec![];
                for row in query.fetch_all(pool).await? {
                    rows.push(JsonRow::try_from(row)?);
                }
                Ok(rows)
            }
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(path) => {
                let conn = self.reconnect()?;
                match conn {
                    Some(DbActiveConnection::Rusqlite(conn)) => {
                        let mut stmt = conn.prepare(&statement)?;
                        submit_rusqlite_statement(&mut stmt, params)
                    }
                    None => Err(RelatableError::DataError(format!(
                        "Unable to connect to the db at '{path}'"
                    ))
                    .into()),
                }
            }
        }
    }

    pub async fn query_one(
        &self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonRow>> {
        let rows = self.query(&statement, params).await?;
        match rows.iter().next() {
            Some(row) => Ok(Some(row.clone())),
            None => Ok(None),
        }
    }

    pub async fn query_value(
        &self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonValue>> {
        let rows = self.query(statement, params).await?;
        extract_value(&rows)
    }

    pub async fn get_table_columns(&self, table: &str) -> Result<Vec<JsonRow>> {
        match self.kind() {
            DbKind::Sqlite => {
                let sql =
                    format!(r#"SELECT "name" FROM pragma_table_info("{table}") ORDER BY "cid""#);
                self.query(&sql, None).await
            }
            DbKind::Postgres => {
                let sql = format!(
                    r#"SELECT "column_name"::TEXT AS "name"
                       FROM "information_schema"."columns"
                       WHERE "table_schema" = 'public'
                       AND "table_name" = {SQL_PARAM}
                       ORDER BY "ordinal_position""#,
                );
                let params = json!([table]);
                self.query(&sql, Some(&params)).await
            }
        }
    }

    pub async fn view_exists_for(&mut self, table: &str) -> Result<bool> {
        // TODO: Add a trace! call here and at the beginning of any other functions in this module
        // that are missing one.
        let statement = match self.kind() {
            DbKind::Sqlite => format!(
                r#"SELECT 1
                   FROM sqlite_master
                   WHERE type = 'view' AND name = {SQL_PARAM}"#
            ),
            DbKind::Postgres => format!(
                r#"SELECT 1
                   FROM "information_schema"."tables"
                   WHERE "table_schema" = 'public'
                   AND "table_name" = {SQL_PARAM}
                   AND "table_type" = 'VIEW'"#,
            ),
        };
        let params = json!([format!("{table}_default_view")]);
        let result = self.query_value(&statement, Some(&params)).await?;
        match result {
            None => Ok(false),
            _ => Ok(true),
        }
    }

    pub async fn get_next_id(&self, table: &str) -> Result<usize> {
        tracing::trace!("Row::get_next_id({table:?}, tx)");
        let current_row_id = match self.kind() {
            DbKind::Sqlite => {
                let sql = format!(r#"SELECT seq FROM sqlite_sequence WHERE name = {SQL_PARAM}"#);
                let params = json!([table]);
                self.query_value(&sql, Some(&params)).await?
            }
            DbKind::Postgres => {
                let sql = format!(
                    // Note that in the case of postgres an _id column is required.
                    r#"SELECT last_value FROM public."{table}__id_seq""#
                );
                self.query_value(&sql, None).await?
            }
        };
        let current_row_id = match current_row_id {
            Some(value) => value.as_u64().unwrap_or_default() as usize,
            None => 0,
        };
        Ok(current_row_id + 1)
    }
}

#[derive(Debug)]
pub enum DbTransaction<'a> {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::Transaction<'a, sqlx::Any>, DbKind),

    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Transaction<'a>),
}

// TODO: Try to share more code (i.e., refactor a little) between DbTransaction and DbConnection.
// E.g., the query() methods share things in common.
impl DbTransaction<'_> {
    pub fn kind(&self) -> DbKind {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, kind) => *kind,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => DbKind::Sqlite,
        }
    }

    pub fn commit(self) -> Result<()> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => block_on(tx.commit())?,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.commit()?,
        };
        Ok(())
    }

    pub fn rollback(self) -> Result<()> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => block_on(tx.rollback())?,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.rollback()?,
        };
        Ok(())
    }

    // Given a connection and a SQL string, return a vector of JsonRows.
    // This is intended as a low-level function that abstracts over the SQL engine,
    // and whatever result types it returns.
    // Since it uses a vector, statements should be limited to a sane number of rows.
    pub fn query(&mut self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        if !valid_params(params) {
            tracing::warn!("invalid parameter argument");
            return Ok(vec![]);
        }
        let statement = match self.kind() {
            DbKind::Sqlite => statement,
            DbKind::Postgres => &local_sql_syntax(&self.kind(), statement),
        };
        self.query_direct(&statement, params)
    }

    pub fn query_direct(
        &mut self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Vec<JsonRow>> {
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => {
                let query = prepare_sqlx_query(&statement, params)?;
                let mut rows = vec![];
                for row in block_on(query.fetch_all(block_on(tx.acquire())?))? {
                    rows.push(JsonRow::try_from(row)?);
                }
                Ok(rows)
            }
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => {
                let mut stmt = tx.prepare(&statement)?;
                submit_rusqlite_statement(&mut stmt, params)
            }
        }
    }

    pub fn query_one(
        &mut self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonRow>> {
        let rows = self.query(&statement, params)?;
        match rows.iter().next() {
            Some(row) => Ok(Some(row.clone())),
            None => Ok(None),
        }
    }

    pub fn query_value(
        &mut self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonValue>> {
        let rows = self.query(statement, params)?;
        extract_value(&rows)
    }

    pub fn get_table_columns(&mut self, table: &str) -> Result<Vec<JsonRow>> {
        match self.kind() {
            DbKind::Sqlite => {
                let sql =
                    format!(r#"SELECT "name" FROM pragma_table_info("{table}") ORDER BY "cid""#);
                self.query(&sql, None)
            }
            DbKind::Postgres => {
                let sql = format!(
                    r#"SELECT "column_name"::TEXT AS "name"
                       FROM "information_schema"."columns"
                       WHERE "table_schema" = 'public'
                       AND "table_name" = {SQL_PARAM}
                       ORDER BY "ordinal_position""#,
                );
                let params = json!([table]);
                self.query(&sql, Some(&params))
            }
        }
    }

    pub fn view_exists_for(&mut self, table: &str) -> Result<bool> {
        // TODO: Add a trace! call here and at the beginning of any other functions in this module
        // that are missing one.
        let statement = match self.kind() {
            DbKind::Sqlite => format!(
                r#"SELECT 1
                   FROM sqlite_master
                   WHERE type = 'view' AND name = {SQL_PARAM}"#
            ),
            DbKind::Postgres => format!(
                r#"SELECT 1
                   FROM "information_schema"."tables"
                   WHERE "table_schema" = 'public'
                   AND "table_name" = {SQL_PARAM}
                   AND "table_type" = 'VIEW'"#,
            ),
        };
        let params = json!([format!("{table}_default_view")]);
        let result = self.query_value(&statement, Some(&params))?;
        match result {
            None => Ok(false),
            _ => Ok(true),
        }
    }

    pub fn get_next_id(&mut self, table: &str) -> Result<usize> {
        tracing::trace!("Row::get_next_id({table:?}, tx)");
        let current_row_id = match self.kind() {
            DbKind::Sqlite => {
                let sql = format!(r#"SELECT seq FROM sqlite_sequence WHERE name = {SQL_PARAM}"#);
                let params = json!([table]);
                self.query_value(&sql, Some(&params))?
            }
            DbKind::Postgres => {
                let sql = format!(
                    // Note that in the case of postgres an _id column is required.
                    r#"SELECT last_value FROM public."{table}__id_seq""#
                );
                self.query_value(&sql, None)?
            }
        };
        let current_row_id = match current_row_id {
            Some(value) => value.as_u64().unwrap_or_default() as usize,
            None => 0,
        };
        Ok(current_row_id + 1)
    }
}

///////////////////////////////////////////////////////////////////////////////
// Database-related utilities and functions
///////////////////////////////////////////////////////////////////////////////

/// Helper function to determine whether the given name is 'simple', as defined by
/// [DB_OBJECT_MATCH_STR]
pub fn is_simple(db_object_name: &str) -> Result<(), String> {
    let db_object_root = db_object_name.splitn(2, ".").collect::<Vec<_>>()[0];
    if !DB_OBJECT_REGEX.is_match(&db_object_root) {
        Err(format!(
            "Illegal database object name: '{}' in '{}'. Does not match: /{}/",
            db_object_root, db_object_name, DB_OBJECT_MATCH_STR,
        ))
    } else {
        Ok(())
    }
}

pub fn is_clause(db_kind: &DbKind) -> String {
    match db_kind {
        DbKind::Sqlite => "IS".into(),
        DbKind::Postgres => "IS NOT DISTINCT FROM".into(),
    }
}

pub fn is_not_clause(db_kind: &DbKind) -> String {
    match db_kind {
        DbKind::Sqlite => "IS NOT".into(),
        DbKind::Postgres => "IS DISTINCT FROM".into(),
    }
}

/// Given a SQL string, possibly with unbound parameters represented by the placeholder string
/// [SQL_PARAM], and given a database kind, if the kind is [Sqlite](DbKind::Sqlite), then change
/// the syntax usedters to SQLite syntax, which uses "?", otherwise use the syntax appropriate for
/// that kind.
pub fn local_sql_syntax(kind: &DbKind, sql: &str) -> String {
    let mut pg_param_idx = 1;
    let mut final_sql = String::from("");
    let mut saved_start = 0;
    for m in SQL_PARAM_REGEX.find_iter(sql) {
        let this_match = &sql[m.start()..m.end()];
        final_sql.push_str(&sql[saved_start..m.start()]);
        if this_match == SQL_PARAM {
            match *kind {
                DbKind::Postgres => {
                    final_sql.push_str(&format!("${}", pg_param_idx));
                    pg_param_idx += 1;
                }
                DbKind::Sqlite => {
                    final_sql.push_str(&format!("?"));
                }
            }
        } else {
            final_sql.push_str(&format!("{}", this_match));
        }
        saved_start = m.start() + this_match.len();
    }
    final_sql.push_str(&sql[saved_start..]);
    final_sql
}

/// Given an SQL string that has been bound to the given parameter vector, construct a database
/// query and return it.
#[cfg(feature = "sqlx")]
pub fn prepare_sqlx_query<'a>(
    statement: &'a str,
    params: Option<&'a JsonValue>,
) -> Result<sqlx::query::Query<'a, sqlx::Any, sqlx::any::AnyArguments<'a>>> {
    let mut query = sqlx::query::<sqlx::Any>(&statement);
    if let Some(params) = params {
        for param in params.as_array().unwrap() {
            match param {
                JsonValue::Number(n) => match n.as_i64() {
                    Some(p) => query = query.bind(p),
                    None => match n.as_f64() {
                        Some(p) => query = query.bind(p),
                        None => panic!(),
                    },
                },
                JsonValue::String(s) => query = query.bind(s),
                _ => query = query.bind(param.to_string()),
            };
        }
    }
    Ok(query)
}

#[cfg(feature = "rusqlite")]
pub fn submit_rusqlite_statement(
    stmt: &mut rusqlite::Statement<'_>,
    params: Option<&JsonValue>,
) -> Result<Vec<JsonRow>> {
    let column_names = stmt
        .column_names()
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>();
    let column_names = column_names.iter().map(|c| c.as_str()).collect::<Vec<_>>();

    if let Some(params) = params {
        for (i, param) in params.as_array().unwrap().iter().enumerate() {
            let param = match param {
                JsonValue::String(s) => s,
                _ => &param.to_string(),
            };
            // Binding must begin with 1 rather than 0:
            stmt.raw_bind_parameter(i + 1, param)?;
        }
    }
    let mut rows = stmt.raw_query();

    let mut result = Vec::new();
    while let Some(row) = rows.next()? {
        result.push(JsonRow::from_rusqlite(&column_names, row));
    }
    Ok(result)
}

pub fn valid_params(params: Option<&JsonValue>) -> bool {
    if let Some(params) = params {
        match params {
            JsonValue::Array(_) => true,
            _ => false,
        }
    } else {
        true
    }
}

pub fn extract_value(rows: &Vec<JsonRow>) -> Result<Option<JsonValue>> {
    match rows.iter().next() {
        Some(row) => match row.content.values().next() {
            Some(value) => Ok(Some(value.clone())),
            None => Ok(None),
        },
        None => Ok(None),
    }
}

pub fn generate_table_ddl(table: &Table, force: bool, db_kind: &DbKind) -> Result<Vec<String>> {
    if table.has_meta {
        for (cname, col) in table.columns.iter() {
            if cname == "_id" || cname == "_order" {
                return Err(RelatableError::InputError(format!(
                    "column {cname} conflicts with has_meta == {has_meta}",
                    has_meta = table.has_meta,
                ))
                .into());
            }

            if col.primary_key {
                return Err(RelatableError::InputError(format!(
                    "Primary key on column {cname} conflicts with has_meta == {has_meta}",
                    has_meta = table.has_meta,
                ))
                .into());
            }
        }
    }

    let mut ddl = vec![];
    let mut column_clauses = vec![];
    for (cname, col) in table.columns.iter() {
        if col.table != table.name {
            return Err(RelatableError::InputError(format!(
                "Table name mismatch: '{}' != '{}'",
                col.table, table.name,
            ))
            .into());
        }
        let clause = format!(
            r#""{cname}" TEXT{unique}"#,
            unique = match col.unique {
                true => " UNIQUE",
                false => "",
            },
        );
        column_clauses.push(clause);
    }

    if force {
        if let DbKind::Postgres = db_kind {
            ddl.push(format!(r#"DROP TABLE "{}" CASCADE"#, table.name));
        }
    }

    let mut sql = format!(r#"CREATE TABLE "{}" ( "#, table.name);
    if table.has_meta {
        sql.push_str(match db_kind {
            DbKind::Sqlite => {
                "_id INTEGER PRIMARY KEY AUTOINCREMENT, \
                 _order INTEGER UNIQUE, "
            }
            DbKind::Postgres => {
                "_id SERIAL PRIMARY KEY, \
                 _order INTEGER UNIQUE, "
            }
        });
    }
    sql.push_str(&format!(" {})", column_clauses.join(", ")));
    ddl.push(sql);

    if table.has_meta {
        let update_stmt = format!(
            r#"UPDATE "{table}" SET _order = ({MOVE_INTERVAL} * NEW._id)
                WHERE _id = NEW._id;"#,
            table = table.name,
        );
        match db_kind {
            DbKind::Sqlite => {
                ddl.push(format!(
                    r#"CREATE TRIGGER "{table}_order"
                         AFTER INSERT ON "{table}"
                         WHEN NEW._order IS NULL
                           BEGIN
                             {update_stmt}
                           END"#,
                    table = table.name,
                ));
            }
            DbKind::Postgres => {
                ddl.push(format!(
                    r#"CREATE OR REPLACE FUNCTION "update_order_and_nextval_{table}"()
                         RETURNS TRIGGER
                         LANGUAGE PLPGSQL
                         AS
                       $$
                       BEGIN
                         IF NEW._order IS NOT DISTINCT FROM NULL THEN
                           {update_stmt}
                         END IF;
                         PERFORM setval('{table}__id_seq', NEW._id);
                         RETURN NEW;
                       END;
                       $$"#,
                    table = table.name,
                ));
                ddl.push(format!(
                    r#"CREATE TRIGGER "{table}_order"
                         AFTER INSERT ON "{table}"
                         FOR EACH ROW
                         EXECUTE FUNCTION "update_order_and_nextval_{table}"()"#,
                    table = table.name,
                ));
            }
        };
    }

    Ok(ddl)
}

pub fn generate_view_ddl(
    table_name: &str,
    view_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<Column>,
    db_kind: &DbKind,
) -> String {
    // TODO: The behaviour for sqlite is slightly different than for postgres (if not exits vs
    // or replace). Make them consistent.

    // Note that '?' parameters are not allowed in views so we must hard code them:
    match db_kind {
        DbKind::Sqlite => format!(
            r#"CREATE VIEW IF NOT EXISTS "{view}" AS
                 SELECT
                   {id_col} AS _id,
                   {order_col} AS _order,
                   (SELECT '[' || GROUP_CONCAT("after") || ']'
                      FROM (
                        SELECT "after"
                        FROM "history"
                        WHERE "table" = '{table}'
                        AND "after" IS NOT NULL
                        AND "row" = {id_col}
                        ORDER BY "history_id"
                     )
                   ) AS "_history",
                   (SELECT NULLIF(
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
                      WHERE "table" = '{table}'
                      AND "row" = {id_col}
                      ORDER BY "column", "message_id"
                   ) AS "_message",
                   {columns}
                 FROM "{table}""#,
            table = table_name,
            view = view_name,
            columns = columns
                .iter()
                .map(|c| format!(r#""{}""#, c.name))
                .collect::<Vec<_>>()
                .join(", "),
        ),
        DbKind::Postgres => format!(
            r#"CREATE OR REPLACE VIEW "{view}" AS
                 SELECT
                   "{table}"._id,
                   "{table}"._order,
                   ( SELECT json_agg(m.*)::TEXT AS json_agg
                     FROM ( SELECT "message"."column",
                                   "message"."value",
                                   "message"."level",
                                   "message"."rule",
                                   "message"."message"
                            FROM "message"
                     WHERE "message"."table" = '{table}' AND "message"."row" = "{table}"._id
                     ORDER BY "message"."column", "message"."message_id") m) AS "message",
                     ( SELECT ('['::TEXT || string_agg(h.after, ','::TEXT)) || ']'::TEXT
                       FROM ( SELECT "history"."after"
                              FROM "history"
                              WHERE "history"."table" = '{table}'
                                AND "after" IS DISTINCT FROM NULL
                                AND "row" = "{table}"._id
                              ORDER BY "history_id" ) h ) AS "history",
                     {columns}
                     FROM "{table}""#,
            table = table_name,
            view = view_name,
            columns = columns
                .iter()
                .map(|c| format!(r#""{}""#, c.name))
                .collect::<Vec<_>>()
                .join(", "),
        ),
    }
}

pub fn generate_table_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    let mut table = Table::new("table");
    table.columns.insert(
        "table".into(),
        Column {
            table: "table".into(),
            name: "table".into(),
            unique: true,
            ..Default::default()
        },
    );
    table.columns.insert(
        "path".into(),
        Column {
            table: "table".into(),
            name: "path".into(),
            unique: true,
            ..Default::default()
        },
    );
    generate_table_ddl(&table, force, db_kind).unwrap()
}

// TODO: When the Table struct is rich enough to support different datatypes, foreign keys,
// and defaults, create these other meta tables in a similar way to the table table above.

pub fn generate_user_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    let mut ddl = vec![];
    if force {
        if let DbKind::Postgres = db_kind {
            ddl.push(format!(r#"DROP TABLE "user" CASCADE"#));
        }
    }

    ddl.push(format!(
        r#"CREATE TABLE "user" (
             "name" TEXT PRIMARY KEY,
             "color" TEXT,
             "cursor" TEXT,
             "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP
           )"#
    ));
    ddl
}

pub fn generate_change_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    match db_kind {
        DbKind::Sqlite => {
            vec![r#"CREATE TABLE "change" (
                      change_id INTEGER PRIMARY KEY AUTOINCREMENT,
                      "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                      "user" TEXT NOT NULL,
                      "action" TEXT NOT NULL,
                      "table" TEXT NOT NULL,
                      "description" TEXT,
                      "content" TEXT,
                      FOREIGN KEY ("user") REFERENCES "user"("name")
                    )"#
            .to_string()]
        }
        DbKind::Postgres => {
            let mut ddl = vec![];
            if force {
                if let DbKind::Postgres = db_kind {
                    ddl.push(format!(r#"DROP TABLE "change" CASCADE"#));
                }
            }
            ddl.push(format!(
                r#"CREATE TABLE "change" (
                     change_id SERIAL PRIMARY KEY,
                     "datetime" TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                     "user" TEXT NOT NULL,
                     "action" TEXT NOT NULL,
                     "table" TEXT NOT NULL,
                     "description" TEXT,
                     "content" TEXT,
                     FOREIGN KEY ("user") REFERENCES "user"("name")
                   )"#
            ));
            ddl
        }
    }
}

pub fn generate_history_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    match db_kind {
        DbKind::Sqlite => {
            vec![r#"CREATE TABLE "history" (
                      history_id INTEGER PRIMARY KEY AUTOINCREMENT,
                      change_id INTEGER NOT NULL,
                      "table" TEXT NOT NULL,
                      "row" INTEGER NOT NULL,
                      "before" TEXT,
                      "after" TEXT,
                      FOREIGN KEY ("change_id") REFERENCES "change"("change_id"),
                      FOREIGN KEY ("table") REFERENCES "table"("table")
                    )"#
            .to_string()]
        }
        DbKind::Postgres => {
            let mut ddl = vec![];
            if force {
                if let DbKind::Postgres = db_kind {
                    ddl.push(format!(r#"DROP TABLE "history" CASCADE"#));
                }
            }
            ddl.push(format!(
                r#"CREATE TABLE "history" (
                     history_id SERIAL PRIMARY KEY,
                     change_id INTEGER NOT NULL,
                     "table" TEXT NOT NULL,
                     "row" INTEGER NOT NULL,
                     "before" TEXT,
                     "after" TEXT,
                     FOREIGN KEY ("change_id") REFERENCES "change"("change_id"),
                     FOREIGN KEY ("table") REFERENCES "table"("table")
                   )"#
            ));
            ddl
        }
    }
}

pub fn generate_message_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    match db_kind {
        DbKind::Sqlite => {
            vec![r#"CREATE TABLE "message" (
                      "message_id" INTEGER PRIMARY KEY AUTOINCREMENT,
                      "added_by" TEXT,
                      "table" TEXT NOT NULL,
                      "row" INTEGER NOT NULL,
                      "column" TEXT NOT NULL,
                      "value" TEXT,
                      "level" TEXT,
                      "rule" TEXT,
                      "message" TEXT,
                      FOREIGN KEY ("table") REFERENCES "table"("table")
                    )"#
            .to_string()]
        }
        DbKind::Postgres => {
            let mut ddl = vec![];
            if force {
                if let DbKind::Postgres = db_kind {
                    ddl.push(format!(r#"DROP TABLE "message" CASCADE"#));
                }
            }
            ddl.push(format!(
                r#"CREATE TABLE "message" (
                     "message_id" SERIAL PRIMARY KEY,
                     "added_by" TEXT,
                     "table" TEXT NOT NULL,
                     "row" INTEGER NOT NULL,
                     "column" TEXT NOT NULL,
                     "value" TEXT,
                     "level" TEXT,
                     "rule" TEXT,
                     "message" TEXT,
                     FOREIGN KEY ("table") REFERENCES "table"("table")
                   )"#
            ));
            ddl
        }
    }
}

pub fn generate_meta_tables_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    let mut ddl = generate_table_table_ddl(force, db_kind);
    ddl.append(&mut generate_user_table_ddl(force, db_kind));
    ddl.append(&mut generate_change_table_ddl(force, db_kind));
    ddl.append(&mut generate_history_table_ddl(force, db_kind));
    ddl.append(&mut generate_message_table_ddl(force, db_kind));
    ddl
}

///////////////////////////////////////////////////////////////////////////////
// Utilities for dealing with JSON representations of rows. The reason these
// are located here instead of in core.rs is because the implementation of
// JsonRow is dependent, in part, on whether the sqlx or rusqlite crate feature
// is enabled. Encapsulating the handling of that crate feature from the rest of
// the API is the other purpose of this module.
///////////////////////////////////////////////////////////////////////////////

// WARN: This needs to be thought through.
pub fn json_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "".to_string(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => value.to_string(),
        JsonValue::Array(value) => format!("{value:?}"),
        JsonValue::Object(value) => format!("{value:?}"),
    }
}

pub fn json_to_unsigned(value: &JsonValue) -> Result<usize> {
    match value {
        JsonValue::Bool(flag) => match flag {
            true => Ok(1),
            false => Ok(0),
        },
        JsonValue::Number(value) => match value.as_u64() {
            Some(unsigned) => Ok(unsigned as usize),
            None => Err(
                RelatableError::InputError(format!("{value} is not an unsigned integer")).into(),
            ),
        },
        JsonValue::String(value_str) => match value_str.parse::<usize>() {
            Ok(unsigned) => Ok(unsigned),
            Err(err) => Err(RelatableError::InputError(format!(
                "{value} could not be parsed as an unsigned integer: {err}"
            ))
            .into()),
        },
        _ => Err(RelatableError::InputError(format!(
            "{value} could not be parsed as an unsigned integer"
        ))
        .into()),
    }
}

// From https://stackoverflow.com/a/78372188
pub trait VecInto<D> {
    fn vec_into(self) -> Vec<D>;
}

impl<E, D> VecInto<D> for Vec<E>
where
    D: From<E>,
{
    fn vec_into(self) -> Vec<D> {
        self.into_iter().map(std::convert::Into::into).collect()
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct JsonRow {
    pub content: JsonMap<String, JsonValue>,
}

impl JsonRow {
    pub fn new() -> Self {
        Self {
            content: JsonMap::new(),
        }
    }

    pub fn nullified(row: &Self, table: &Table) -> Self {
        tracing::debug!("nullified({row:?}, {table:?})");
        let mut nullified_row = Self::new();
        for (column, value) in row.content.iter() {
            let nulltype = table
                .get_column_attribute(&column, "nulltype")
                .unwrap_or("".to_string());
            match value {
                JsonValue::String(s) if s == "" && nulltype == "empty" => {
                    nullified_row
                        .content
                        .insert(column.to_string(), JsonValue::Null);
                }
                value => {
                    nullified_row
                        .content
                        .insert(column.to_string(), value.clone());
                }
            };
        }
        tracing::debug!("Nullified row: {nullified_row:?}");
        nullified_row
    }

    pub fn get_value(&self, column_name: &str) -> Result<JsonValue> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(value.clone()),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    pub fn get_string(&self, column_name: &str) -> Result<String> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(json_to_string(&value)),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    pub fn get_unsigned(&self, column_name: &str) -> Result<usize> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => json_to_unsigned(&value),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    pub fn from_strings(strings: &Vec<&str>) -> Self {
        let mut json_row = Self::new();
        for string in strings {
            json_row.content.insert(string.to_string(), JsonValue::Null);
        }
        json_row
    }

    pub fn to_strings(&self) -> Vec<String> {
        let mut result = vec![];
        for column_name in self.content.keys() {
            // The logic of this implies that this should not fail, so an expect() is
            // appropriate here.
            result.push(self.get_string(column_name).expect("Column not found"));
        }
        result
    }

    pub fn to_string_map(&self) -> IndexMap<String, String> {
        let mut result = IndexMap::new();
        for column_name in self.content.keys() {
            result.insert(
                column_name.clone(),
                self.get_string(column_name).expect("Column not found"),
            );
        }
        result
    }

    #[cfg(feature = "rusqlite")]
    fn from_rusqlite(column_names: &Vec<&str>, row: &rusqlite::Row) -> Self {
        let mut content = JsonMap::new();
        for column_name in column_names {
            let value = match row.get_ref(*column_name) {
                Ok(value) => match value {
                    rusqlite::types::ValueRef::Null => JsonValue::Null,
                    rusqlite::types::ValueRef::Integer(value) => JsonValue::from(value),
                    rusqlite::types::ValueRef::Real(value) => JsonValue::from(value),
                    rusqlite::types::ValueRef::Text(value)
                    | rusqlite::types::ValueRef::Blob(value) => {
                        let value = std::str::from_utf8(value).unwrap_or_default();
                        JsonValue::from(value)
                    }
                },
                Err(_) => JsonValue::Null,
            };
            content.insert(column_name.to_string(), value);
        }
        Self { content }
    }
}

#[cfg(feature = "sqlx")]
impl TryFrom<sqlx::any::AnyRow> for JsonRow {
    fn try_from(row: sqlx::any::AnyRow) -> Result<Self> {
        let mut content = JsonMap::new();
        for column in row.columns() {
            // I had problems getting a type for columns that are not in the schema,
            // e.g. "SELECT COUNT() AS count".
            // So now I start with Null and try INTEGER, NUMBER, STRING, BOOL.
            let mut value: JsonValue = JsonValue::Null;
            if value.is_null() {
                let x: Result<i32, sqlx::Error> = row.try_get(column.ordinal());
                if let Ok(x) = x {
                    value = JsonValue::from(x);
                }
            }
            if value.is_null() {
                let x: Result<f64, sqlx::Error> = row.try_get(column.ordinal());
                if let Ok(x) = x {
                    value = JsonValue::from(x);
                }
            }
            if value.is_null() {
                let x: Result<String, sqlx::Error> = row.try_get(column.ordinal());
                if let Ok(x) = x {
                    value = JsonValue::from(x);
                }
            }
            if value.is_null() {
                let x: Result<bool, sqlx::Error> = row.try_get(column.ordinal());
                if let Ok(x) = x {
                    value = JsonValue::from(x);
                }
            }
            content.insert(column.name().into(), value);
        }
        Ok(Self { content })
    }

    type Error = anyhow::Error;
}

impl std::fmt::Display for JsonRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_strings().join("\t"))
    }
}

impl std::fmt::Debug for JsonRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.to_string_map())
    }
}

impl From<JsonRow> for Vec<String> {
    fn from(row: JsonRow) -> Self {
        row.to_strings()
    }
}
