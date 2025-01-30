//! # rltbl/relatable
//!
//! This is relatable (rltbl::sql).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! any elements of the API that are database-specific.

use crate as rltbl;
use rltbl::core::RelatableError;

use anyhow::Result;
use async_std::sync::{Mutex, MutexGuard};
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use sqlx::{Acquire as _, Column as _, Row as _};

#[cfg(feature = "sqlx")]
use sqlx_core::any::AnyTypeInfoKind;

#[derive(Debug)]
pub enum DbConnection {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::AnyPool),

    #[cfg(feature = "rusqlite")]
    Rusqlite(Mutex<rusqlite::Connection>),
}

#[derive(Debug)]
pub enum DbTransaction<'a> {
    #[cfg(feature = "sqlx")]
    Sqlx(sqlx::Transaction<'a, sqlx::Any>),

    #[cfg(feature = "rusqlite")]
    Rusqlite(rusqlite::Transaction<'a>),
}

impl DbTransaction<'_> {
    pub async fn commit(self) -> Result<()> {
        match self {
            #[cfg(feature = "sqlx")]
            DbTransaction::Sqlx(tx) => {
                tx.commit().await?;
            }
            #[cfg(feature = "rusqlite")]
            DbTransaction::Rusqlite(tx) => tx.commit()?,
        };
        Ok(())
    }
}

///////////////////////////////////////////////////////////////////////////////
// Functions for connecting to and querying the database
///////////////////////////////////////////////////////////////////////////////

pub async fn connect(path: &str) -> Result<DbConnection> {
    // We suppress warnings for unused variables for this particular variable because the
    // compiler is becoming confused about which variables have been actually used as a result
    // of the conditional sqlx/rusqlite compilation (or maybe the programmer is confused).
    #[allow(unused_variables)]
    #[cfg(feature = "rusqlite")]
    let connection = DbConnection::Rusqlite(Mutex::new(rusqlite::Connection::open(path)?));

    #[cfg(feature = "sqlx")]
    let connection = {
        let url = format!("sqlite://{path}?mode=rwc");
        sqlx::any::install_default_drivers();
        let connection = DbConnection::Sqlx(sqlx::AnyPool::connect(&url).await?);
        connection
    };

    Ok(connection)
}

pub async fn lock_connection<'a>(
    connection: &'a DbConnection,
) -> Option<MutexGuard<'a, rusqlite::Connection>> {
    let conn = match connection {
        #[cfg(feature = "sqlx")]
        DbConnection::Sqlx(_) => None,
        #[cfg(feature = "rusqlite")]
        DbConnection::Rusqlite(conn) => Some(conn),
    };
    match conn {
        None => None,
        Some(conn) => Some(conn.lock().await),
    }
}

pub async fn begin<'a>(
    connection: &DbConnection,
    locked_conn: &'a mut Option<MutexGuard<'_, rusqlite::Connection>>,
) -> Result<DbTransaction<'a>> {
    match connection {
        #[cfg(feature = "sqlx")]
        DbConnection::Sqlx(pool) => {
            let tx = pool.begin().await?;
            Ok(DbTransaction::Sqlx(tx))
        }
        #[cfg(feature = "rusqlite")]
        DbConnection::Rusqlite(_conn) => match locked_conn {
            None => {
                return Err(RelatableError::InputError(
                    "Can't begin Rusqlite transaction: No locked connection provided".to_string(),
                )
                .into())
            }
            Some(ref mut conn) => {
                let tx = conn.transaction()?;
                Ok(DbTransaction::Rusqlite(tx))
            }
        },
    }
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

// Given a connection and a SQL string, return a vector of JsonRows.
// This is intended as a low-level function that abstracts over the SQL engine,
// and whatever result types it returns.
// Since it uses a vector, statements should be limited to a sane number of rows.
pub async fn query(
    connection: &DbConnection,
    statement: &str,
    params: Option<&JsonValue>,
) -> Result<Vec<JsonRow>> {
    if !valid_params(params) {
        tracing::warn!("invalid parameter argument");
        return Ok(vec![]);
    }

    match connection {
        #[cfg(feature = "sqlx")]
        DbConnection::Sqlx(pool) => {
            let query = prepare_sqlx_query(statement, params)?;
            let mut rows = vec![];
            for row in query.fetch_all(pool).await? {
                rows.push(JsonRow::try_from(row)?);
            }
            Ok(rows)
        }
        #[cfg(feature = "rusqlite")]
        DbConnection::Rusqlite(conn) => {
            let conn = conn.lock().await;
            let mut stmt = conn.prepare(statement)?;
            submit_rusqlite_statement(&mut stmt, params)
        }
    }
}

pub async fn query_one(
    connection: &DbConnection,
    statement: &str,

    params: Option<&JsonValue>,
) -> Result<Option<JsonRow>> {
    let rows = query(&connection, &statement, params).await?;
    match rows.iter().next() {
        Some(row) => Ok(Some(row.clone())),
        None => Ok(None),
    }
}

// Given a connection and a SQL string, return a vector of JsonRows.
// This is intended as a low-level function that abstracts over the SQL engine,
// and whatever result types it returns.
// Since it uses a vector, statements should be limited to a sane number of rows.
pub async fn query_tx(
    transaction: &mut DbTransaction<'_>,
    statement: &str,
    params: Option<&JsonValue>,
) -> Result<Vec<JsonRow>> {
    if !valid_params(params) {
        tracing::warn!("invalid parameter argument");
        return Ok(vec![]);
    }

    match transaction {
        #[cfg(feature = "sqlx")]
        DbTransaction::Sqlx(tx) => {
            let query = prepare_sqlx_query(statement, params)?;
            let mut rows = vec![];
            for row in query.fetch_all(tx.acquire().await?).await? {
                rows.push(JsonRow::try_from(row)?);
            }
            Ok(rows)
        }
        #[cfg(feature = "rusqlite")]
        DbTransaction::Rusqlite(tx) => {
            let mut stmt = tx.prepare(statement)?;
            submit_rusqlite_statement(&mut stmt, params)
        }
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

pub async fn query_value(
    connection: &DbConnection,
    statement: &str,
    params: Option<&JsonValue>,
) -> Result<Option<JsonValue>> {
    let rows = query(connection, statement, params).await?;
    extract_value(&rows)
}

pub async fn query_value_tx(
    transaction: &mut DbTransaction<'_>,
    statement: &str,
    params: Option<&JsonValue>,
) -> Result<Option<JsonValue>> {
    let rows = query_tx(transaction, statement, params).await?;
    extract_value(&rows)
}

///////////////////////////////////////////////////////////////////////////////
// Other database-related utilities and functions
///////////////////////////////////////////////////////////////////////////////

/// Represents a 'simple' database name
pub const DB_OBJECT_MATCH_STR: &str = r"^[\w_]+$";

/// Helper function to determine whether the given name is 'simple', as defined by
/// [DB_OBJECT_MATCH_STR]
pub fn is_simple(db_object_name: &str) -> Result<(), String> {
    let db_object_regex: Regex = Regex::new(DB_OBJECT_MATCH_STR).unwrap();

    let db_object_root = db_object_name.splitn(2, ".").collect::<Vec<_>>()[0];
    if !db_object_regex.is_match(&db_object_root) {
        Err(format!(
            "Illegal database object name: '{}' in '{}'. Does not match: /{}/",
            db_object_root, db_object_name, DB_OBJECT_MATCH_STR,
        ))
    } else {
        Ok(())
    }
}

///////////////////////////////////////////////////////////////////////////////
// Utilities for dealing with JSON representations of rows. The reason thses
// are located here instead of in core.rs is because the implementation of
// JsonRow is dependent, in part, on whether the sqlx or rusqlite crate feature
// is enabled. Encapsulating the handling of that crate feature from the rest of
// the API the other purpose of this module.
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

    pub fn get_string(&self, column_name: &str) -> String {
        let value = self.content.get(column_name);
        match value {
            Some(value) => json_to_string(&value),
            None => unimplemented!("missing value"),
        }
    }

    fn to_strings(&self) -> Vec<String> {
        let mut result = vec![];
        for column_name in self.content.keys() {
            result.push(self.get_string(column_name));
        }
        result
    }

    fn to_string_map(&self) -> IndexMap<String, String> {
        let mut result = IndexMap::new();
        for column_name in self.content.keys() {
            result.insert(column_name.clone(), self.get_string(column_name));
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
            let value = match column.type_info().kind() {
                AnyTypeInfoKind::SmallInt | AnyTypeInfoKind::Integer | AnyTypeInfoKind::BigInt => {
                    let value: i32 = row.try_get(column.ordinal())?;
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Real | AnyTypeInfoKind::Double => {
                    let value: f64 = row.try_get(column.ordinal())?;
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Text => {
                    let value: String = row.try_get(column.ordinal())?;
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Bool => {
                    let value: bool = row.try_get(column.ordinal())?;
                    JsonValue::from(value)
                }
                AnyTypeInfoKind::Null => JsonValue::Null,
                AnyTypeInfoKind::Blob => {
                    return Err(
                        RelatableError::InputError("Unimplemented: SQL blob".to_string()).into(),
                    );
                }
            };
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
