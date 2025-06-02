//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[sql](crate::sql)).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! elements of the API that are particularly database-specific.

use crate as rltbl;
use rltbl::{
    core::{self, RelatableError, NEW_ORDER_MULTIPLIER},
    table::{Column, Table},
};

use anyhow::Result;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::{fmt::Display, str::FromStr};

#[cfg(feature = "rusqlite")]
use rusqlite;

use async_std::task::block_on;

#[cfg(feature = "sqlx")]
use sqlx::{
    any::{AnyConnectOptions, AnyPoolOptions},
    Acquire as _, Column as _, Row as _,
};

/// A 'simple' database name
pub static DB_OBJECT_MATCH_STR: &str = r"^[\w_]+$";

lazy_static! {
    /// The regex used to match ['simple'](DB_OBJECT_MATCH_STR) database names
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

/// Default size for the in-memory cache
pub static DEFAULT_MEMORY_CACHE_SIZE: usize = 1000;

/// Strategy to use for caching
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CachingStrategy {
    None,
    TruncateAll,
    Truncate,
    Trigger,
    Memory(usize),
}

/// The structure used to look up query results in the in-memory cache:
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct MemoryCacheKey {
    pub tables: String,
    pub sql: String,
}

impl FromStr for CachingStrategy {
    type Err = anyhow::Error;

    fn from_str(strategy: &str) -> Result<Self> {
        tracing::trace!("CachingStrategy::from_str({strategy:?})");
        match strategy.to_lowercase().as_str() {
            "none" => Ok(Self::None),
            "truncate_all" => Ok(Self::TruncateAll),
            "truncate" => Ok(Self::Truncate),
            "trigger" => Ok(Self::Trigger),
            strategy if strategy.starts_with("memory:") => {
                let elems = strategy.split(":").collect::<Vec<_>>();
                let cache_size = {
                    if elems.len() < 2 {
                        DEFAULT_MEMORY_CACHE_SIZE
                    } else {
                        let cache_size = elems[1];
                        let cache_size = cache_size.parse::<usize>()?;
                        match cache_size {
                            0 => DEFAULT_MEMORY_CACHE_SIZE,
                            size => size,
                        }
                    }
                };
                Ok(Self::Memory(cache_size))
            }
            _ => {
                return Err(RelatableError::InputError(format!(
                    "Unrecognized strategy: {strategy}"
                ))
                .into());
            }
        }
    }
}

impl Display for CachingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CachingStrategy::None => write!(f, "none"),
            CachingStrategy::TruncateAll => write!(f, "truncate_all"),
            CachingStrategy::Truncate => write!(f, "truncate"),
            CachingStrategy::Trigger => write!(f, "trigger"),
            CachingStrategy::Memory(size) => write!(f, "memory:{size}"),
        }
    }
}

/// Represents the kind of database being managed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbKind {
    Postgres,
    Sqlite,
}

/// Used to generate database-specific parameter placeholder strings for binding to SQL statements
#[derive(Clone, Copy, Debug)]
pub struct SqlParam {
    /// The kind of database the parameters will be generated for
    pub kind: DbKind,
    /// The current parameter index, if applicable
    pub index: usize,
}

impl SqlParam {
    /// Create a new parameter for the given database kind
    pub fn new(kind: &DbKind) -> Self {
        Self {
            kind: *kind,
            index: 0,
        }
    }

    /// Generate one parameter. If the database syntax involves an index, this is incremented
    /// automatically.
    pub fn next(&mut self) -> String {
        match self.kind {
            DbKind::Postgres => {
                self.index += 1;
                format!("${}", self.index)
            }
            DbKind::Sqlite => "?".to_string(),
        }
    }

    /// Generate `amount` parameters, incrementing the index accordingly.
    pub fn get(&mut self, amount: usize) -> Vec<String> {
        let mut params = vec![];
        let mut made = 0;
        while made < amount {
            params.push(self.next());
            made += 1;
        }
        params
    }

    /// Generate `amount` parameters and return then as a single comma-separated string rather than
    /// as a list of strings.
    pub fn get_as_list(&mut self, amount: usize) -> String {
        self.get(amount).join(", ")
    }

    /// Resets the index
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

/// Represents an active database connection
#[derive(Debug)]
pub enum DbActiveConnection {
    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Connection),
}

/// Represents a database connection
#[derive(Debug)]
pub enum DbConnection {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::AnyPool, DbKind),

    #[cfg(feature = "rusqlite")]
    Rusqlite(String),
}

impl DbConnection {
    /// Returns the kind of database that this connection is associated with
    pub fn kind(&self) -> DbKind {
        tracing::trace!("DbConnection::kind()");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, kind) => *kind,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => DbKind::Sqlite,
        }
    }

    /// Connects to the given database
    pub async fn connect(database: &str) -> Result<(Self, Option<DbActiveConnection>)> {
        tracing::trace!("DbConnection::connect({database})");
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
                    let connection_options = AnyConnectOptions::from_str(database)?;
                    let db_kind = DbKind::Postgres;
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
                    sqlx::any::install_default_drivers();
                    let connection =
                        Self::Sqlx(sqlx::AnyPool::connect(&url).await?, DbKind::Sqlite);
                    (connection, None)
                };

                Ok(tuple)
            }
        }
    }

    /// Reconnect to the current database
    pub fn reconnect(&self) -> Result<Option<DbActiveConnection>> {
        tracing::trace!("DbConnection::reconnect()");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, _) => Ok(None),
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(path) => Ok(Some(DbActiveConnection::Rusqlite(
                rusqlite::Connection::open(path)?,
            ))),
        }
    }

    /// Begin a transaction
    pub async fn begin<'a>(
        &self,
        conn: &'a mut Option<DbActiveConnection>,
    ) -> Result<DbTransaction<'a>> {
        tracing::trace!("DbConnection::begin()");
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

    /// Given a connection and a generic SQL string with placeholders and a list of parameters
    /// to interpolate into the string, return a vector of [JsonRow]s.
    /// Note that since this returns a vector, statements should be limited to those that will
    /// return a sane number of rows.
    pub async fn query(&self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        tracing::trace!("DbConnection::query({statement}, {params:?})");
        if !valid_params(params) {
            tracing::warn!("invalid parameter argument");
            return Ok(vec![]);
        }
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

    /// Query for a single row
    pub async fn query_one(
        &self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonRow>> {
        tracing::trace!("DbConnection::query_one({statement}, {params:?})");
        let rows = self.query(&statement, params).await?;
        match rows.iter().next() {
            Some(row) => Ok(Some(row.clone())),
            None => Ok(None),
        }
    }

    /// Query for a single value
    pub async fn query_value(
        &self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonValue>> {
        tracing::trace!("DbConnection::query_value({statement}, {params:?})");
        let rows = self.query(statement, params).await?;
        Ok(extract_value(&rows))
    }

    /// Attempt to use the cache to query
    pub async fn cache(
        &self,
        sql: &str,
        params: Option<&JsonValue>,
        tables: &Vec<String>,
        strategy: &CachingStrategy,
    ) -> Result<Vec<JsonRow>> {
        tracing::trace!("cache({sql}, {params:?}, {strategy:?})");

        async fn _cache(
            conn: &DbConnection,
            tables: &Vec<String>,
            sql: &str,
            params: Option<&JsonValue>,
        ) -> Result<Vec<JsonRow>> {
            let tables = tables
                .iter()
                .map(|t| json!(t).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let (cache_sql, tables) = {
                let mut sql_param = SqlParam::new(&conn.kind());
                match conn.kind() {
                    DbKind::Postgres => {
                        let sql = format!(
                            r#"SELECT {}||rtrim(ltrim("value", '['), ']')||{} AS "value"
                               FROM "cache"
                               WHERE "tables"::TEXT = {}
                               AND "key" = {} LIMIT 1"#,
                            sql_param.next(),
                            sql_param.next(),
                            sql_param.next(),
                            sql_param.next()
                        );
                        (sql, format!("[{tables}]"))
                    }
                    DbKind::Sqlite => {
                        let sql = format!(
                            r#"SELECT {}||rtrim(ltrim("value", '['), ']')||{} AS "value"
                               FROM "cache"
                               WHERE CAST("tables" AS TEXT) = {}
                               AND "key" = {} LIMIT 1"#,
                            sql_param.next(),
                            sql_param.next(),
                            sql_param.next(),
                            sql_param.next()
                        );
                        (sql, format!("[{tables}]"))
                    }
                }
            };
            let cache_params = json!([r#"[{"content": "#, "}]", tables, sql]);
            match conn.query_one(&cache_sql, Some(&cache_params)).await? {
                Some(json_row) => {
                    tracing::debug!("Cache hit for tables {tables}");
                    let value = json_row.get_string("value")?;
                    let json_rows: Vec<JsonRow> = serde_json::from_str(&value)?;
                    Ok(json_rows)
                }
                None => {
                    tracing::debug!("Cache miss for tables {tables}");
                    let json_rows = conn.query(sql, params).await?;
                    let json_rows_content = json_rows
                        .iter()
                        .map(|r| r.content.clone())
                        .collect::<Vec<_>>();
                    let mut sql_param = SqlParam::new(&conn.kind());
                    let update_cache_sql = match conn.kind() {
                        DbKind::Postgres => {
                            format!(
                                r#"INSERT INTO "cache" ("tables", "key", "value")
                                   VALUES ({}::JSONB, {}, {})"#,
                                sql_param.next(),
                                sql_param.next(),
                                sql_param.next(),
                            )
                        }
                        DbKind::Sqlite => {
                            format!(
                                r#"INSERT INTO "cache" ("tables", "key", "value")
                                   VALUES ({}, {}, {})"#,
                                sql_param.next(),
                                sql_param.next(),
                                sql_param.next(),
                            )
                        }
                    };
                    let update_cache_params = json!([tables, sql, json_rows_content]);
                    conn.query(&update_cache_sql, Some(&update_cache_params))
                        .await?;
                    Ok(json_rows)
                }
            }
        }

        match strategy {
            CachingStrategy::None => self.query(sql, params).await,
            CachingStrategy::TruncateAll | CachingStrategy::Truncate | CachingStrategy::Trigger => {
                _cache(self, tables, sql, params).await
            }
            CachingStrategy::Memory(cache_size) => {
                let mut cache = core::CACHE.lock().expect("Could not lock cache");
                let keys = cache.keys().map(|key| key.clone()).collect::<Vec<_>>();

                for (i, key) in keys.iter().enumerate().rev() {
                    if i >= *cache_size {
                        tracing::debug!("Removing {key:?} ({i}th entry) from cache");
                        cache.remove(&key);
                    } else {
                        break;
                    }
                }

                let tables = tables
                    .iter()
                    .map(|t| json!(t).to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let mem_key = MemoryCacheKey {
                    tables: tables.to_string(),
                    sql: sql.to_string(),
                };
                match cache.get(&mem_key) {
                    Some(json_rows) => {
                        tracing::debug!("Cache hit for tables {tables}");
                        Ok(json_rows.to_vec())
                    }
                    None => {
                        tracing::debug!("Cache miss for tables {tables}");
                        // Why is a block_on() call needed here but not above?
                        let json_rows = block_on(self.query(sql, params))?;
                        cache.insert(
                            MemoryCacheKey {
                                tables: tables.to_string(),
                                sql: sql.to_string(),
                            },
                            json_rows.to_vec(),
                        );
                        Ok(json_rows)
                    }
                }
            }
        }
    }
}

/// A database transaction
#[derive(Debug)]
pub enum DbTransaction<'a> {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::Transaction<'a, sqlx::Any>, DbKind),

    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Transaction<'a>),
}

impl DbTransaction<'_> {
    /// The kind of database this transaction is associated with
    pub fn kind(&self) -> DbKind {
        tracing::trace!("DbTransaction::kind()");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, kind) => *kind,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => DbKind::Sqlite,
        }
    }

    /// Commit this transaction
    pub fn commit(self) -> Result<()> {
        tracing::trace!("DbTransaction::commit()");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => block_on(tx.commit())?,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.commit()?,
        };
        Ok(())
    }

    /// Rollback this transaction
    pub fn rollback(self) -> Result<()> {
        tracing::trace!("DbTransaction::rollback()");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => block_on(tx.rollback())?,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.rollback()?,
        };
        Ok(())
    }

    /// Given a connection and a generic SQL string with placeholders and a list of parameters
    /// to interpolate into the string, return a vector of [JsonRow]s.
    /// Note that since this returns a vector, statements should be limited to those that will
    /// return a sane number of rows.
    pub fn query(&mut self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        tracing::trace!("DbTransaction::query({statement}, {params:?})");
        if !valid_params(params) {
            tracing::warn!("invalid parameter argument");
            return Ok(vec![]);
        }
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

    /// Query for a single row
    pub fn query_one(
        &mut self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonRow>> {
        tracing::trace!("DbTransaction::query_one({statement}, {params:?})");
        let rows = self.query(&statement, params)?;
        match rows.iter().next() {
            Some(row) => Ok(Some(row.clone())),
            None => Ok(None),
        }
    }

    /// Query for a single value
    pub fn query_value(
        &mut self,
        statement: &str,
        params: Option<&JsonValue>,
    ) -> Result<Option<JsonValue>> {
        tracing::trace!("DbTransaction::query_value({statement}, {params:?})");
        let rows = self.query(statement, params)?;
        Ok(extract_value(&rows))
    }
}

///////////////////////////////////////////////////////////////////////////////
// Database-specific utilities and functions
///////////////////////////////////////////////////////////////////////////////

/// Determine whether the given table exists in the database
pub async fn table_exists(table: &str, conn: &DbConnection) -> Result<bool> {
    tracing::trace!("table_exists({table}, {conn:?})");
    let sql_param = SqlParam::new(&conn.kind()).next();
    let (sql, params) = match conn.kind() {
        DbKind::Sqlite => (
            format!(
                r#"SELECT 1 FROM "sqlite_master"
                   WHERE "type" = {sql_param} AND name = {sql_param} LIMIT 1"#,
            ),
            json!(["table", table]),
        ),
        DbKind::Postgres => (
            format!(
                r#"SELECT 1 FROM "information_schema"."tables"
                   WHERE "table_type" LIKE {sql_param} AND "table_name" = {sql_param}"#,
            ),
            json!(["%TABLE", table]),
        ),
    };
    match conn.query_value(&sql, Some(&params)).await? {
        None => Ok(false),
        Some(_) => Ok(true),
    }
}

/// Determine whether a view already exists for the table in the database.
pub fn view_exists_for(table: &str, tx: &mut DbTransaction<'_>) -> Result<bool> {
    tracing::trace!("view_exists_for({table}, tx)");
    let sql_param = SqlParam::new(&tx.kind()).next();
    let statement = match tx.kind() {
        DbKind::Sqlite => format!(
            r#"SELECT 1
               FROM sqlite_master
               WHERE type = 'view' AND name = {sql_param}"#
        ),
        DbKind::Postgres => format!(
            r#"SELECT 1
               FROM "information_schema"."tables"
               WHERE "table_name" = {sql_param}
               AND "table_type" = 'VIEW'"#,
        ),
    };
    let params = json!([format!("{table}_default_view")]);
    let result = tx.query_value(&statement, Some(&params))?;
    match result {
        None => Ok(false),
        _ => Ok(true),
    }
}

/// Query the database for the columns associated with the given table
pub fn get_db_table_columns(table: &str, tx: &mut DbTransaction<'_>) -> Result<Vec<JsonRow>> {
    tracing::trace!("get_db_table_columns({table:?}, tx)");
    match tx.kind() {
        DbKind::Sqlite => {
            let sql = format!(r#"SELECT "name" FROM pragma_table_info("{table}") ORDER BY "cid""#);
            tx.query(&sql, None)
        }
        DbKind::Postgres => {
            let sql = format!(
                r#"SELECT "column_name"::TEXT AS "name"
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

/// Get the given attribute of the given table and column from the column table
pub async fn get_db_column_attribute(
    table: &str,
    column: &str,
    attribute: &str,
    conn: &DbConnection,
) -> Result<Option<String>> {
    let mut sql_param = SqlParam::new(&conn.kind());
    let is_not_clause = is_not_clause(&conn.kind());
    let sql = format!(
        r#"SELECT "{attribute}"
           FROM "column"
           WHERE "{attribute}" {is_not_clause} NULL
           AND "table" = {}
           AND "column" = {}"#,
        sql_param.next(),
        sql_param.next(),
    );
    let params = json!([table, column]);
    let value = match conn.query_one(&sql, Some(&params)).await {
        Ok(Some(row)) => Some(row.get_string(attribute)?),
        Ok(None) => None,
        Err(err) => {
            // We assume that any database errors encountered here are because the (optional)
            // column table does not exist. But we log a debug message in case we need to
            // troubleshoot.
            tracing::debug!("Received message: '{err}' from database. Returning None");
            None
        }
    };
    Ok(value)
}

/// Query the database for what the id of the next created row of the given table will be
pub fn get_next_id(table: &str, tx: &mut DbTransaction<'_>) -> Result<usize> {
    tracing::trace!("get_next_id({table:?}, tx)");
    let current_row_id = match tx.kind() {
        DbKind::Sqlite => {
            let sql = r#"SELECT seq FROM sqlite_sequence WHERE name = ?"#;
            let params = json!([table]);
            tx.query_value(sql, Some(&params))?
        }
        DbKind::Postgres => {
            let sql = format!(
                // Note that in the case of postgres an _id column is required.
                r#"SELECT last_value FROM "{table}__id_seq""#
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

/// Helper function to determine whether the given name is 'simple', as defined by
/// [DB_OBJECT_MATCH_STR]
pub fn is_simple(db_object_name: &str) -> Result<(), String> {
    tracing::trace!("is_simple({db_object_name})");
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

/// Helper function to deal with alternative "IS" syntax for different SQL flavours
pub fn is_clause(db_kind: &DbKind) -> String {
    tracing::trace!("is_clause({db_kind:?})");
    match db_kind {
        DbKind::Sqlite => "IS".into(),
        DbKind::Postgres => "IS NOT DISTINCT FROM".into(),
    }
}

/// Helper function to deal with alternative "IS NOT" syntax for different SQL flavours
pub fn is_not_clause(db_kind: &DbKind) -> String {
    tracing::trace!("is_not_clause({db_kind:?})");
    match db_kind {
        DbKind::Sqlite => "IS NOT".into(),
        DbKind::Postgres => "IS DISTINCT FROM".into(),
    }
}

/// Given an SQL string that has been bound to the given parameter vector, construct a database
/// query and return it.
#[cfg(feature = "sqlx")]
pub fn prepare_sqlx_query<'a>(
    statement: &'a str,
    params: Option<&'a JsonValue>,
) -> Result<sqlx::query::Query<'a, sqlx::Any, sqlx::any::AnyArguments<'a>>> {
    tracing::trace!("prepare_sqlx_query({statement}, {params:?})");
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

/// Execute the given rusqlite statement
#[cfg(feature = "rusqlite")]
fn submit_rusqlite_statement(
    stmt: &mut rusqlite::Statement<'_>,
    params: Option<&JsonValue>,
) -> Result<Vec<JsonRow>> {
    tracing::trace!("submit_rusqlite_statement({stmt:?}, {params:?})");
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

/// Validate that the given parameters are in the form of a JSON Array.
fn valid_params(params: Option<&JsonValue>) -> bool {
    tracing::trace!("valid_params({params:?})");
    if let Some(params) = params {
        match params {
            JsonValue::Array(_) => true,
            _ => false,
        }
    } else {
        true
    }
}

/// Extract the first value of the first row in `rows`.
fn extract_value(rows: &Vec<JsonRow>) -> Option<JsonValue> {
    tracing::trace!("extract_value({rows:?})");
    match rows.iter().next() {
        Some(row) => match row.content.values().next() {
            Some(value) => Some(value.clone()),
            None => None,
        },
        None => None,
    }
}

/////////////////
// Functions for generating DDL
////////////////

/// Generate DDL to create the given table in the database. If `force` is set, drop the table
/// first.
pub fn generate_table_ddl(
    table: &Table,
    force: bool,
    db_kind: &DbKind,
    caching_strategy: &CachingStrategy,
) -> Result<Vec<String>> {
    tracing::trace!("generate_table_ddl({table:?}, {force}, {db_kind:?}, {caching_strategy:?})");
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

        let sql_type = match &col.datatype {
            None => "TEXT",
            Some(datatype) if datatype.to_lowercase() == "text" => "TEXT",
            Some(datatype) if datatype.to_lowercase() == "integer" => "INTEGER",
            Some(unsupported) => {
                return Err(RelatableError::InputError(format!(
                    "Unsupported datatype: '{unsupported}'"
                ))
                .into());
            }
        };

        let clause = format!(
            r#""{cname}" {sql_type}{unique}"#,
            unique = match col.unique {
                true => " UNIQUE",
                false => "",
            },
        );
        column_clauses.push(clause);
    }

    if force {
        match db_kind {
            DbKind::Postgres => {
                ddl.push(format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, table.name))
            }
            DbKind::Sqlite => ddl.push(format!(r#"DROP TABLE IF EXISTS "{}""#, table.name)),
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

    // Add triggers for metacolumns if they are present:
    if table.has_meta {
        add_metacolumn_trigger_ddl(&mut ddl, &table.name, db_kind);
    }

    // Add triggers for updating the "cache" and "table" tables whenever this table is
    // changed, if the Trigger caching strategy has been specified:
    if let CachingStrategy::Trigger = caching_strategy {
        add_caching_trigger_ddl(&mut ddl, &table.name, db_kind);
    }

    Ok(ddl)
}

/// Add triggers for updating the meta columns, _id, and _order, of the given table.
pub fn add_metacolumn_trigger_ddl(ddl: &mut Vec<String>, table: &str, db_kind: &DbKind) {
    let update_stmt = format!(
        r#"UPDATE "{table}" SET _order = ({NEW_ORDER_MULTIPLIER} * NEW._id)
           WHERE _id = NEW._id;"#
    );
    match db_kind {
        DbKind::Sqlite => {
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_order"
                   AFTER INSERT ON "{table}"
                   WHEN NEW._order IS NULL
                     BEGIN
                       {update_stmt}
                     END"#
            ));
        }
        DbKind::Postgres => {
            // This is required, because in PostgreSQL, assigning SERIAL PRIMARY KEY to a column is
            // equivalent to:
            //   CREATE SEQUENCE table_name_id_seq;
            //   CREATE TABLE table_name (
            //     id integer NOT NULL DEFAULT nextval('table_name_id_seq')
            //   );
            //   ALTER SEQUENCE table_name_id_seq OWNED BY table_name.id;
            // This means that such a column is only ever auto-incremented when it is explicitly
            // left out of an INSERT statement. To replicate SQLite's more sane behaviour, we define
            // the following trigger to *always* update the last value of the sequence to the
            // currently inserted row number. A similar trigger is also defined generically for
            // postgresql tables in [rltbl::core].
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
                     IF NEW._id > (SELECT MAX(last_value) FROM "{table}__id_seq") THEN
                       PERFORM setval('{table}__id_seq', NEW._id);
                     END IF;
                     RETURN NEW;
                   END;
                   $$"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_order"
                   AFTER INSERT ON "{table}"
                   FOR EACH ROW
                   EXECUTE FUNCTION "update_order_and_nextval_{table}"()"#
            ));
        }
    };
}

/// Add a trigger to update the query cache for the given table.
pub fn add_caching_trigger_ddl(ddl: &mut Vec<String>, table: &str, db_kind: &DbKind) {
    match db_kind {
        DbKind::Sqlite => {
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_insert"
                   AFTER INSERT ON "{table}"
                   BEGIN
                     DELETE FROM "cache" WHERE "tables" LIKE '%"{table}"%';
                   END"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_update"
                   AFTER UPDATE ON "{table}"
                   BEGIN
                     DELETE FROM "cache" WHERE "tables" LIKE '%"{table}"%';
                   END"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_delete"
                   AFTER DELETE ON "{table}"
                   BEGIN
                     DELETE FROM "cache" WHERE "tables" LIKE '%"{table}"%';
                   END"#
            ));
        }
        DbKind::Postgres => {
            // Note that the '?' is *not* being used as a parameter placeholder here
            // but a JSONB operator.
            ddl.push(format!(
                r#"CREATE OR REPLACE FUNCTION "clean_cache_for_{table}"()
                     RETURNS TRIGGER
                     LANGUAGE PLPGSQL
                   AS
                   $$
                   BEGIN
                     DELETE FROM "cache" WHERE "tables" ? '{table}';
                     RETURN NEW;
                   END;
                   $$"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_insert"
                   AFTER INSERT ON "{table}"
                   EXECUTE FUNCTION "clean_cache_for_{table}"()"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_update"
                   AFTER UPDATE ON "{table}"
                   EXECUTE FUNCTION "clean_cache_for_{table}"()"#
            ));
            ddl.push(format!(
                r#"CREATE TRIGGER "{table}_cache_after_delete"
                   AFTER DELETE ON "{table}"
                   EXECUTE FUNCTION "clean_cache_for_{table}"()"#
            ));
        }
    };
}

/// Generate the DDL for creating a view on the given table,
pub fn generate_view_ddl(
    table_name: &str,
    view_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<Column>,
    kind: &DbKind,
) -> Vec<String> {
    tracing::trace!(
        "generate_view_ddl({table_name}, {view_name}, {id_col}, {order_col}, {columns:?}, {kind:?})"
    );
    // Note that '?' parameters are not allowed in views so we must hard code them:
    match kind {
        DbKind::Sqlite => vec![
            format!(r#"DROP VIEW IF EXISTS "{}""#, view_name),
            format!(
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
        ],
        DbKind::Postgres => vec![format!(
            r#"CREATE OR REPLACE VIEW "{view}" AS
                 SELECT
                   "{table}"._id,
                   "{table}"._order,
                   (
                     SELECT ('['::TEXT || string_agg(h.after, ','::TEXT)) || ']'::TEXT
                     FROM ( SELECT "history"."after"
                            FROM "history"
                            WHERE "history"."table" = '{table}'
                            AND "after" IS DISTINCT FROM NULL
                            AND "row" = "{table}"._id
                            ORDER BY "history_id" ) h
                   ) AS "_history",
                   (
                     SELECT json_agg(m.*)::TEXT AS json_agg
                     FROM ( SELECT "message"."column",
                                   "message"."value",
                                   "message"."level",
                                   "message"."rule",
                                   "message"."message"
                            FROM "message"
                     WHERE "message"."table" = '{table}' AND "message"."row" = "{table}"._id
                     ORDER BY "message"."column", "message"."message_id") m
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
        )],
    }
}

/// Generate the DDL used to create the table table. If `force` is set, drop the table first
pub fn generate_table_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_table_table_ddl({force}, {db_kind:?})");
    let mut ddl = vec![];
    if force {
        if let DbKind::Postgres = db_kind {
            ddl.push(format!(r#"DROP TABLE IF EXISTS "table" CASCADE"#));
        }
    }
    let pkey_clause = match db_kind {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    ddl.push(format!(
        r#"CREATE TABLE "table" (
             "_id" {pkey_clause},
             "_order" INTEGER UNIQUE,
             "table" TEXT UNIQUE,
             "path" TEXT UNIQUE
           )"#
    ));

    // Add metacolumn triggers before returning the DDL:
    add_metacolumn_trigger_ddl(&mut ddl, "table", db_kind);
    ddl
}

/// Generate the DDL used to create the cache table. If `force` is set, drop the table first
pub fn generate_cache_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_cache_table_ddl({force}, {db_kind:?})");
    let mut ddl = vec![];
    if force {
        if let DbKind::Postgres = db_kind {
            ddl.push(format!(r#"DROP TABLE IF EXISTS "cache" CASCADE"#));
        }
    }

    let json_type = match db_kind {
        DbKind::Postgres => "JSONB",
        DbKind::Sqlite => "JSON",
    };

    ddl.push(format!(
        r#"CREATE TABLE "cache" (
             "tables" {json_type},
             "key" TEXT,
             "value" TEXT,
              PRIMARY KEY ("tables", "key")
           )"#
    ));
    ddl
}

// TODO: When the Table struct is rich enough to support different datatypes, foreign keys,
// and defaults, create these other meta tables in a similar way to the table table above.

/// Generate the DDL used to create the user table. If `force` is set, drop the table first
pub fn generate_user_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_user_table_ddl({force}, {db_kind:?})");
    let mut ddl = vec![];
    if force {
        if let DbKind::Postgres = db_kind {
            ddl.push(format!(r#"DROP TABLE IF EXISTS "user" CASCADE"#));
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

/// Generate the DDL used to create the change table. If `force` is set, drop the table first
pub fn generate_change_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_change_table_ddl({force}, {db_kind:?})");
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
                    ddl.push(format!(r#"DROP TABLE IF EXISTS "change" CASCADE"#));
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

/// Generate the DDL used to create the history table. If `force` is set, drop the table first
pub fn generate_history_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_history_table_ddl({force}, {db_kind:?})");
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
                    ddl.push(format!(r#"DROP TABLE IF EXISTS "history" CASCADE"#));
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

/// Generate the DDL used to create the message table. If `force` is set, drop the table first
pub fn generate_message_table_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_message_table_ddl({force}, {db_kind:?})");
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
                    ddl.push(format!(r#"DROP TABLE IF EXISTS "message" CASCADE"#));
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

/// Generate the DDL used to create all of the required meta tables. If `force` is set, drop the
/// tables first
pub fn generate_meta_tables_ddl(force: bool, db_kind: &DbKind) -> Vec<String> {
    tracing::trace!("generate_meta_tables_ddl({force}, {db_kind:?})");
    let mut ddl = generate_table_table_ddl(force, db_kind);
    ddl.append(&mut generate_cache_table_ddl(force, db_kind));
    ddl.append(&mut generate_user_table_ddl(force, db_kind));
    ddl.append(&mut generate_change_table_ddl(force, db_kind));
    ddl.append(&mut generate_history_table_ddl(force, db_kind));
    ddl.append(&mut generate_message_table_ddl(force, db_kind));
    ddl
}

///////////////////////////////////////////////////////////////////////////////
// Utilities for dealing with JSON representations of database rows.
///////////////////////////////////////////////////////////////////////////////

// WARN: This needs to be thought through.
/// Convert the given JSON value to a string
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

/// Convert the given JSON value to an unsigned integer
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

/// A JSON representation of a database row
#[derive(Clone, Serialize, Deserialize)]
pub struct JsonRow {
    pub content: JsonMap<String, JsonValue>,
}

impl JsonRow {
    /// Initialize an empty [JsonRow]
    pub fn new() -> Self {
        Self {
            content: JsonMap::new(),
        }
    }

    /// Set any column values whose content matches the column's nulltype to [JsonValue::Null]
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

    /// Get the value of the given column from the row
    pub fn get_value(&self, column_name: &str) -> Result<JsonValue> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(value.clone()),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Get the value of the given column fromt he row and convert it to a string before returning
    /// it
    pub fn get_string(&self, column_name: &str) -> Result<String> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(json_to_string(&value)),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Get the value of the given column fromt he row and convert it to an unsigned integer
    /// before returning it
    pub fn get_unsigned(&self, column_name: &str) -> Result<usize> {
        let value = self.content.get(column_name);
        match value {
            Some(value) => json_to_unsigned(&value),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Initialize a new row from the given list of column names and set all values to
    /// [JsonValue::Null]
    pub fn from_strings(strings: &Vec<&str>) -> Self {
        let mut json_row = Self::new();
        for string in strings {
            json_row.content.insert(string.to_string(), JsonValue::Null);
        }
        json_row
    }

    /// Return all of the values in this row to a vector of strings and return it
    pub fn to_strings(&self) -> Vec<String> {
        let mut result = vec![];
        for column_name in self.content.keys() {
            // The logic of this implies that this should not fail, so an expect() is
            // appropriate here.
            result.push(self.get_string(column_name).expect("Column not found"));
        }
        result
    }

    /// Generate a map from the column names of the row to their values and return it
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

    /// Initialize a [JsonRow] from the given [rusqlite::Row]
    #[cfg(feature = "rusqlite")]
    pub fn from_rusqlite(column_names: &Vec<&str>, row: &rusqlite::Row) -> Self {
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
