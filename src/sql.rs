//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[sql](crate::sql)).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! any elements of the API that are database-specific.

use crate as rltbl;
use rltbl::core::{RelatableError, Table};

use anyhow::Result;
use indexmap::IndexMap;
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};

#[cfg(feature = "rusqlite")]
use rusqlite;

#[cfg(feature = "sqlx")]
use async_std::task::block_on;

#[cfg(feature = "sqlx")]
use sqlx::{Acquire as _, Column as _, Row as _};

// In principle SQL_PARAM can be set to any arbitrary sequence of non-word characters. If you would
// like SQL_PARAM to be a word then you must also modify SQL_PARAM_REGEX correspondingly. See the
// comment beside it, below, for instructions on how to do that.
/// The placeholder to use for query parameters when binding using sqlx. Currently set to "?",
/// which corresponds to SQLite's parameter syntax. To convert SQL to postgres, use the function
/// [local_sql_syntax()].
pub static SQL_PARAM: &str = "?";

lazy_static! {
    // This accepts a non-word SQL_PARAM unless it is enclosed in quotation marks. To use a word
    // SQL_PARAM change '\B' to '\b' below.
    /// Regular expression used to find the next instance of [SQL_PARAM] in a given SQL statement.
    pub static ref SQL_PARAM_REGEX: Regex = Regex::new(&format!(
        r#"('[^'\\]*(?:\\.[^'\\]*)*'|"[^"\\]*(?:\\.[^"\\]*)*")|\B{}\B"#,
        SQL_PARAM
    ))
    .unwrap();
}

/// Represents a 'simple' database name
pub static DB_OBJECT_MATCH_STR: &str = r"^[\w_]+$";

/// The [maximum number of parameters](https://www.sqlite.org/limits.html#max_variable_number)
/// that can be bound to a SQLite query
pub static MAX_PARAMS_SQLITE: usize = 32766;

/// The [maximum number of parameters](https://www.postgresql.org/docs/current/limits.html)
/// that can be bound to a Postgres query
#[cfg(feature = "sqlx")]
pub static MAX_PARAMS_POSTGRES: usize = 65535;

/// Represents the kind of database being managed
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbKind {
    #[cfg(feature = "sqlx")]
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

    pub async fn connect(path: &str) -> Result<(Self, Option<DbActiveConnection>)> {
        // We suppress warnings for unused variables for this particular variable because the
        // compiler is becoming confused about which variables have been actually used as a result
        // of the conditional sqlx/rusqlite compilation (or maybe the programmer is confused).
        #[allow(unused_variables)]
        #[cfg(feature = "rusqlite")]
        let tuple = (
            Self::Rusqlite(path.to_string()),
            Some(DbActiveConnection::Rusqlite(rusqlite::Connection::open(
                path,
            )?)),
        );

        #[cfg(feature = "sqlx")]
        let tuple = {
            let url = {
                if path.starts_with("postgresql://") || path.starts_with("sqlite://") {
                    path.to_string()
                } else {
                    format!("sqlite://{path}?mode=rwc")
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
            #[cfg(feature = "sqlx")]
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
            #[cfg(feature = "sqlx")]
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
}

///////////////////////////////////////////////////////////////////////////////
// Database-related utilities and functions
///////////////////////////////////////////////////////////////////////////////

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

/// Given a SQL string, possibly with unbound parameters represented by the placeholder string
/// [SQL_PARAM], and given a database kind, if the kind is Sqlite, then change the syntax used
/// for unbound parameters to Sqlite syntax, which uses "?", otherwise use Postgres syntax, which
/// uses numbered parameters, i.e., $1, $2, ...
pub fn local_sql_syntax(kind: &DbKind, sql: &str) -> String {
    #[cfg(feature = "sqlx")]
    let mut pg_param_idx = 1;

    let mut final_sql = String::from("");
    let mut saved_start = 0;
    for m in SQL_PARAM_REGEX.find_iter(sql) {
        let this_match = &sql[m.start()..m.end()];
        final_sql.push_str(&sql[saved_start..m.start()]);
        if this_match == SQL_PARAM {
            match *kind {
                #[cfg(feature = "sqlx")]
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
