//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[sql](crate::sql)).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! elements of the API that are particularly database-specific.

////////////////////////////////////
// Internal imports
////////////////////////////////////
use crate as rltbl;
use rltbl::{
    core::{self, RelatableError, NEW_ORDER_MULTIPLIER},
    table::{Column, ColumnDatatype, Table},
};

////////////////////////////////////
// External imports
////////////////////////////////////
use anyhow::Result;
use async_std::task::block_on;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::{fmt::Display, str::FromStr};

////////////////////////////////////
// Database-driver-specific imports
////////////////////////////////////
#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use bigdecimal::{BigDecimal, ToPrimitive};

#[cfg(feature = "sqlx")]
use sqlx::{
    any::{install_default_drivers, Any, AnyArguments, AnyRow},
    postgres::{PgArguments, PgConnectOptions, PgPool, PgPoolOptions, PgRow, Postgres},
    query::Query,
    Acquire as _, AnyPool, Column as _, Row as _, Transaction, TypeInfo as _,
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

/// Represents a database connection pool
#[cfg(feature = "sqlx")]
#[derive(Debug)]
pub enum DbPool {
    Sqlite(AnyPool),
    Postgres(PgPool),
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
    Sqlx(DbPool, DbKind),

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
                    let connection_options = PgConnectOptions::from_str(database)?;
                    let db_kind = DbKind::Postgres;
                    let pool = PgPoolOptions::new()
                        .max_connections(MAX_DB_CONNECTIONS)
                        .connect_with(connection_options)
                        .await?;
                    let connection = Self::Sqlx(DbPool::Postgres(pool), db_kind);
                    Ok((connection, None))
                }
            }
            false => {
                // We suppress warnings for unused variables for this particular variable because
                // of the way that we are assigning the connection. We start by assigning a
                // rusqlite connection and then, if the sqlx drivers are enabled, we immediately
                // shadow the connection we just created. This is intentional so we need to
                // suppress the compiler warnings about the unused rusqlite connection.
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
                    install_default_drivers();
                    let pool = AnyPool::connect(&url).await?;
                    let connection = Self::Sqlx(DbPool::Sqlite(pool), DbKind::Sqlite);
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
        tracing::trace!("DbConnection::begin({self:?}, {conn:?})");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(db_pool, kind) => match db_pool {
                DbPool::Sqlite(pool) => {
                    let tx = pool.begin().await?;
                    Ok(DbTransaction::Sqlx(SqlxDbTransaction::Sqlite(tx), *kind))
                }
                DbPool::Postgres(pool) => {
                    let tx = pool.begin().await?;
                    Ok(DbTransaction::Sqlx(SqlxDbTransaction::Postgres(tx), *kind))
                }
            },
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

    /// Given a generic SQL string with placeholders and a list of parameters to interpolate into
    /// the string, return a vector of [JsonRow]s. Note that since this returns a vector,
    /// statements should be limited to those that will return a sane number of rows.
    pub async fn query(&self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        tracing::trace!("DbConnection::query({self:?}, {statement}, {params:?})");
        if !valid_params(params) {
            tracing::warn!("Invalid parameter argument");
            return Ok(vec![]);
        }
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(db_pool, _) => match db_pool {
                DbPool::Sqlite(pool) => {
                    let query = prepare_sqlx_sqlite_query(&statement, params)?;
                    let mut rows = vec![];
                    for row in query.fetch_all(pool).await? {
                        rows.push(JsonRow::try_from(row)?);
                    }
                    Ok(rows)
                }
                DbPool::Postgres(pool) => {
                    let query = prepare_sqlx_pg_query(&statement, params)?;
                    let mut rows = vec![];
                    for row in query.fetch_all(pool).await? {
                        rows.push(JsonRow::try_from(row)?);
                    }
                    Ok(rows)
                }
            },
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

/// A database transaction as defined specifically for the sqlx driver
#[cfg(feature = "sqlx")]
#[derive(Debug)]
pub enum SqlxDbTransaction<'a> {
    Sqlite(Transaction<'a, Any>),
    Postgres(Transaction<'a, Postgres>),
}

/// A database transaction
#[derive(Debug)]
pub enum DbTransaction<'a> {
    #[cfg(feature = "sqlx")]
    Sqlx(SqlxDbTransaction<'a>, DbKind),

    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Transaction<'a>),
}

impl DbTransaction<'_> {
    /// The kind of database this transaction is associated with
    pub fn kind(&self) -> DbKind {
        tracing::trace!("DbTransaction::kind({self:?})");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(_, kind) => *kind,
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(_) => DbKind::Sqlite,
        }
    }

    /// Commit this transaction
    pub fn commit(self) -> Result<()> {
        tracing::trace!("DbTransaction::commit({self:?})");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => match tx {
                SqlxDbTransaction::Sqlite(tx) => block_on(tx.commit())?,
                SqlxDbTransaction::Postgres(tx) => block_on(tx.commit())?,
            },
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.commit()?,
        };
        Ok(())
    }

    /// Rollback this transaction
    pub fn rollback(self) -> Result<()> {
        tracing::trace!("DbTransaction::rollback({self:?})");
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => match tx {
                SqlxDbTransaction::Sqlite(tx) => block_on(tx.rollback())?,
                SqlxDbTransaction::Postgres(tx) => block_on(tx.rollback())?,
            },
            #[cfg(feature = "rusqlite")]
            Self::Rusqlite(tx) => tx.rollback()?,
        };
        Ok(())
    }

    /// Given a generic SQL string with placeholders and a list of parameters to interpolate into
    /// the string, return a vector of [JsonRow]s. Note that since this returns a vector,
    /// statements should be limited to those that will return a sane number of rows.
    pub fn query(&mut self, statement: &str, params: Option<&JsonValue>) -> Result<Vec<JsonRow>> {
        tracing::trace!("DbTransaction::query({self:?}, {statement}, {params:?})");
        if !valid_params(params) {
            tracing::warn!("invalid parameter argument");
            return Ok(vec![]);
        }
        match self {
            #[cfg(feature = "sqlx")]
            Self::Sqlx(tx, _) => match tx {
                SqlxDbTransaction::Sqlite(tx) => {
                    let query = prepare_sqlx_sqlite_query(&statement, params)?;
                    let mut rows = vec![];
                    for row in block_on(query.fetch_all(block_on(tx.acquire())?))? {
                        rows.push(JsonRow::try_from(row)?);
                    }
                    Ok(rows)
                }
                SqlxDbTransaction::Postgres(tx) => {
                    let query = prepare_sqlx_pg_query(&statement, params)?;
                    let mut rows = vec![];
                    for row in block_on(query.fetch_all(block_on(tx.acquire())?))? {
                        rows.push(JsonRow::try_from(row)?);
                    }
                    Ok(rows)
                }
            },
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
        tracing::trace!("DbTransaction::query_one({self:?}, {statement}, {params:?})");
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
        tracing::trace!("DbTransaction::query_value({self:?}, {statement}, {params:?})");
        let rows = self.query(statement, params)?;
        Ok(extract_value(&rows))
    }
}

///////////////////////////////////////////////////////////////////////////////
// Database-specific utilities and functions
///////////////////////////////////////////////////////////////////////////////

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

/// Return the SQL type corresponding to the given datatype
pub fn get_sql_type(datatype: &ColumnDatatype) -> Result<&str> {
    tracing::trace!("get_sql_type({datatype:?})");
    match datatype.name.as_str() {
        "text" => Ok("TEXT"),
        "integer" => Ok("INTEGER"),
        "real" => Ok("REAL"),
        "numeric" => Ok("NUMERIC"),
        unsupported => {
            return Err(RelatableError::InputError(format!(
                "Unsupported datatype: '{unsupported}'",
            ))
            .into());
        }
    }
}

// TODO (maybe): Possibly define a new enum called DbQuery and save some lines of code by
// refactoring prepare_sqlx_sqlite_query() and prepare_sqlx_pg_query() into one function that
// accepts a DbQuery, unless doing that makes things unnecessarily complicated in other ways.

/// Given an SQL string that has been bound to the given parameter vector, construct a database
/// query and return it.
#[cfg(feature = "sqlx")]
pub fn prepare_sqlx_sqlite_query<'a>(
    statement: &'a str,
    params: Option<&'a JsonValue>,
) -> Result<Query<'a, Any, AnyArguments<'a>>> {
    tracing::trace!("prepare_sqlx_query({statement}, {params:?})");
    let mut query = sqlx::query::<Any>(&statement);
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

/// Given an SQL string that has been bound to the given parameter vector, construct a database
/// query and return it.
#[cfg(feature = "sqlx")]
pub fn prepare_sqlx_pg_query<'a>(
    statement: &'a str,
    params: Option<&'a JsonValue>,
) -> Result<Query<'a, Postgres, PgArguments>> {
    tracing::trace!("prepare_sqlx_query({statement}, {params:?})");
    let mut query = sqlx::query::<Postgres>(&statement);
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
        let sql_type = get_sql_type(&col.datatype)?;
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
                 _order BIGINT UNIQUE, "
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

/// Generate the DDL for creating the default view on the given table,
pub(crate) fn generate_default_view_ddl(
    table_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<Column>,
    kind: &DbKind,
) -> Vec<String> {
    tracing::trace!(
        "generate_default_view_ddl({table_name}, {id_col}, {order_col}, {columns:?}, {kind:?})"
    );
    let view_name = format!("{table_name}_default_view");
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

/// Generate the DDL for creating the text view on the given table,
pub(crate) fn generate_text_view_ddl(
    table_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<Column>,
    kind: &DbKind,
) -> Vec<String> {
    tracing::trace!(
        "generate_text_view_ddl({table_name}, {id_col}, {order_col}, {columns:?}, {kind:?})"
    );
    let view_name = format!("{table_name}_text_view");
    // Note that '?' parameters are not allowed in views so we must hard code them:
    let mut inner_columns = columns
        .iter()
        .map(|column| {
            format!(
                r#"CASE
                     WHEN "{column}" {is_clause} NULL THEN (
                       SELECT "value"
                       FROM "message"
                       WHERE "row" = "_id"
                         AND "column" = '{column}'
                         AND "table" = '{table_name}'
                       ORDER BY "message_id" DESC
                       LIMIT 1
                     )
                     ELSE {column_cast}
                   END AS "{column}""#,
                column = column.name,
                is_clause = is_clause(kind),
                column_cast = {
                    let datatype = column.datatype.name.to_string();
                    if *kind == DbKind::Sqlite {
                        if datatype.as_str() == "text" {
                            format!(r#""{}""#, column.name)
                        } else {
                            format!(r#"CAST("{}" AS TEXT)"#, column.name)
                        }
                    } else {
                        format!(r#""{}"::TEXT"#, column.name)
                    }
                }
            )
        })
        .collect::<Vec<_>>();

    let inner_columns = {
        let mut v = vec![
            "_id".to_string(),
            "_order".to_string(),
            "_message".to_string(),
            "_history".to_string(),
        ];
        v.append(&mut inner_columns);
        v
    };

    let mut outer_columns = columns
        .iter()
        .map(|column| format!(r#"t."{}""#, column.name))
        .collect::<Vec<_>>();

    let outer_columns = {
        let mut v = vec![
            "t._id".to_string(),
            "t._order".to_string(),
            "t._message".to_string(),
            "t._history".to_string(),
        ];
        v.append(&mut outer_columns);
        v
    };

    let create_view_sql = format!(
        r#"CREATE VIEW "{view_name}" AS
           SELECT {outer_columns}
           FROM (
               SELECT {inner_columns}
               FROM "{table_name}_default_view"
           ) t"#,
        outer_columns = outer_columns.join(", "),
        inner_columns = inner_columns.join(", "),
    );

    vec![
        format!(r#"DROP VIEW IF EXISTS "{}""#, view_name),
        create_view_sql,
    ]
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
             "_order" BIGINT UNIQUE,
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
                      "row" BIGINT NOT NULL,
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
                     "row" BIGINT NOT NULL,
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
                      "row" BIGINT NOT NULL,
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
                     "row" BIGINT NOT NULL,
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
    tracing::trace!("json_to_string({value:?})");
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
pub fn json_to_unsigned(value: &JsonValue) -> Result<u64> {
    tracing::trace!("json_to_unsigned({value:?})");
    match value {
        JsonValue::Bool(flag) => match flag {
            true => Ok(1),
            false => Ok(0),
        },
        JsonValue::Number(value) => match value.as_u64() {
            Some(unsigned) => Ok(unsigned as u64),
            None => Err(
                RelatableError::InputError(format!("{value} is not an unsigned integer")).into(),
            ),
        },
        JsonValue::String(value_str) => match value_str.parse::<u64>() {
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
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    /// Set any column values whose content matches that column's nulltype to [JsonValue::Null]
    pub fn nullify(row: &Self, table: &Table) -> Self {
        tracing::trace!("JsonRow::nullify({row:?}, {table:?})");
        let mut nullified_row = Self::new();
        for (column, value) in row.content.iter() {
            let nulltype = table
                .get_configured_column_attribute(&column, "nulltype")
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
        tracing::debug!("Nullified row: {row:?} to: {nullified_row:?}");
        nullified_row
    }

    /// Use the [columns configuration](Table::columns) for the given table to lookup the
    /// [nulltype](Column::nulltype) of the given column, and then if the given value matches the
    /// column's nulltype, set it to [Null](JsonValue::Null)
    pub fn nullify_value(table: &Table, column: &str, value: &JsonValue) -> JsonValue {
        tracing::trace!("JsonRow::nullify_value({table:?}, {column}, {value:?})");
        match table.get_configured_column_attribute(column, "nulltype") {
            Some(supported) if supported == "empty" => match value {
                JsonValue::String(s) if s == "" => JsonValue::Null,
                _ => value.clone(),
            },
            Some(unsupported) => {
                tracing::warn!("Unsupported nulltype: '{unsupported}'");
                value.clone()
            }
            None => value.clone(),
        }
    }

    /// Get the value of the given column from the row
    pub fn get_value(&self, column_name: &str) -> Result<JsonValue> {
        tracing::trace!("JsonRow::get_value({self:?}, {column_name})");
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(value.clone()),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Get the value of the given column fromt he row and convert it to a string before returning
    /// it
    pub fn get_string(&self, column_name: &str) -> Result<String> {
        tracing::trace!("JsonRow::get_string({self:?}, {column_name})");
        let value = self.content.get(column_name);
        match value {
            Some(value) => Ok(json_to_string(&value)),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Get the value of the given column from the row and convert it to an unsigned integer
    /// before returning it
    pub fn get_unsigned(&self, column_name: &str) -> Result<u64> {
        tracing::trace!("JsonRow::get_unsigned({self:?}, {column_name})");
        let value = self.content.get(column_name);
        match value {
            Some(value) => json_to_unsigned(&value),
            None => Err(RelatableError::DataError("missing value".to_string()).into()),
        }
    }

    /// Initialize a new row from the given list of column names and set all values to
    /// [JsonValue::Null]
    pub fn from_strings(strings: &Vec<&str>) -> Self {
        tracing::trace!("JsonRow::from_strings({strings:?})");
        let mut json_row = Self::new();
        for string in strings {
            json_row.content.insert(string.to_string(), JsonValue::Null);
        }
        json_row
    }

    /// Return all of the values in this row to a vector of strings and return it
    pub fn to_strings(&self) -> Vec<String> {
        tracing::trace!("JsonRow::to_strings({self:?})");
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
        tracing::trace!("JsonRow::to_string_map({self:?})");
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
        tracing::trace!("JsonRow::from_rusqlite({column_names:?}, {row:?})");
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
impl TryFrom<AnyRow> for JsonRow {
    type Error = anyhow::Error;

    fn try_from(row: AnyRow) -> Result<Self> {
        tracing::trace!("JsonRow::try_from::<AnyRow>(row)");
        let mut content = JsonMap::new();
        for column in row.columns() {
            // We had problems getting a type for columns that are not in the schema,
            // e.g. "SELECT COUNT() AS count".
            // So now we start with Null and try BIGINT/INTEGER, NUMERIC/REAL, STRING, BOOL.
            let mut value: JsonValue = JsonValue::Null;
            if value.is_null() {
                let x: Result<i64, sqlx::Error> = row.try_get(column.ordinal());
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
}

#[cfg(feature = "sqlx")]
impl TryFrom<PgRow> for JsonRow {
    type Error = anyhow::Error;

    fn try_from(row: PgRow) -> Result<Self> {
        tracing::trace!("JsonRow::try_from::<PgRow>(row)");
        let mut content = JsonMap::new();
        for column in row.columns() {
            let column_type = column.type_info().name();
            let value = match column_type {
                "INT4" => {
                    let value: Result<i32, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
                "INT8" => {
                    let value: Result<i64, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
                "FLOAT4" => {
                    let value: Result<f32, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
                "NUMERIC" => {
                    let value: Result<BigDecimal, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => {
                            let value = value.to_f64();
                            JsonValue::from(value)
                        }
                        Err(_) => JsonValue::Null,
                    }
                }
                "TEXT" => {
                    let value: Result<String, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
                "BOOL" => {
                    let value: Result<bool, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
                unsupported => {
                    tracing::warn!(
                        "Got unsupported column '{}' with type '{}'",
                        column.name(),
                        unsupported
                    );
                    let value: Result<String, sqlx::Error> = row.try_get(column.ordinal());
                    match value {
                        Ok(value) => JsonValue::from(value),
                        Err(_) => JsonValue::Null,
                    }
                }
            };
            content.insert(column.name().into(), value);
        }
        Ok(Self { content })
    }
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
