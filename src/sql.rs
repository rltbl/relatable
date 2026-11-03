//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[sql](crate::sql)).
//!
//! This module contains functions for connecting to and querying the database, and implements
//! elements of the API that are particularly database-specific.

//////////////////////////////////////////
// Internal imports
//////////////////////////////////////////
use crate as rltbl;
use rltbl::{
    column::Column,
    core::{meta_column_ddl, RelatableError},
    datatype::Datatypes,
    table::Table,
};

//////////////////////////////////////////
// External imports
//////////////////////////////////////////
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;

//////////////////////////////////////////
// The rest of the code
//////////////////////////////////////////

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
// pub static MAX_PARAMS_POSTGRES: usize = 65535;
// WARN: tokio-postgres seems to have a much lower limit than Postgres itself,
// but this is already big enough.
pub static MAX_PARAMS_POSTGRES: usize = 32766;

/// Used to generate database-specific parameter placeholder strings for binding to SQL statements
#[derive(Clone, Debug)]
pub struct SqlParam {
    /// The kind of database the parameters will be generated for
    pub kind: String,
    /// The current parameter index, if applicable
    pub index: usize,
}

impl SqlParam {
    /// Create a new parameter for the given database kind
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            index: 0,
        }
    }

    /// Generate one parameter. If the database syntax involves an index, this is incremented
    /// automatically.
    pub fn next(&mut self) -> String {
        match self.kind.as_str() {
            "sqlite" => "?".to_string(),
            _ => {
                self.index += 1;
                format!("${}", self.index)
            }
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
pub fn is_clause(db_kind: &str) -> String {
    tracing::trace!("is_clause({db_kind:?})");
    match db_kind {
        "sqlite" => "IS".into(),
        "postgres" => "IS NOT DISTINCT FROM".into(),
        _ => unimplemented!(),
    }
}

/// Helper function to deal with alternative "IS NOT" syntax for different SQL flavours
pub fn is_not_clause(db_kind: &str) -> String {
    tracing::trace!("is_not_clause({db_kind:?})");
    match db_kind {
        "sqlite" => "IS NOT".into(),
        "postgres" => "IS DISTINCT FROM".into(),
        _ => unimplemented!(),
    }
}

/// Generates a SQL clause to cast the column as text using the syntax appropriate for the given
/// database kind.
pub fn cast_column_as_text(column: &str, db_kind: &str) -> String {
    tracing::trace!("cast_column_as_text({column}, {db_kind:?})");
    match db_kind {
        "postgres" => format!(r#""{column}"::TEXT"#),
        _ => format!(r#"CAST("{column}" AS TEXT)"#),
    }
}

/// Generates an SQL clause for a regular expression match on the given column
pub fn regexp_match(column: &str, sql_param: &mut SqlParam) -> String {
    tracing::trace!("regexp_match({column}, {sql_param:?})");
    let casted_column = cast_column_as_text(column, &sql_param.kind);
    match sql_param.kind.as_str() {
        "sqlite" => format!(r#"regexp({}, {}) = 1"#, sql_param.next(), casted_column),
        "postgres" => format!(r#"{} ~ {}"#, casted_column, sql_param.next()),
        _ => unimplemented!(),
    }
}

/// Generates an SQL clause for a regular expression mismatch on the given column
pub fn regexp_mismatch(column: &str, sql_param: &mut SqlParam) -> String {
    tracing::trace!("regexp_mismatch({column}, {sql_param:?})");
    // let casted_column = cast_column_as_text(column, &sql_param.kind);
    match sql_param.kind.as_str() {
        "sqlite" => format!(r#"regexp_match("{column}", {}) = 0"#, sql_param.next()),
        "postgres" => format!(r#""{column}" !~ {}"#, sql_param.next()),
        _ => unimplemented!(),
    }
}

/////////////////
// Functions for generating DDL
////////////////

/// Generate DDL to create the given table in the database. If `force` is set, drop the table
/// first.
// TODO: Replace this
pub fn generate_table_ddl(
    table: &Table,
    columns: &[&Column],
    datatypes: &Datatypes,
    force: bool,
    db_kind: &str,
) -> Result<Vec<String>> {
    if table.has_meta {
        for col in columns {
            let cname = col.column.to_string();
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
    for col in columns {
        let cname = col.column.to_string();
        if col.table != table.name {
            return Err(RelatableError::InputError(format!(
                "Table name mismatch: '{}' != '{}'",
                col.table, table.name,
            ))
            .into());
        }
        let sql_type = col.sql_type(&datatypes);
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
            "postgres" => ddl.push(format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, table.name)),
            "sqlite" => ddl.push(format!(r#"DROP TABLE IF EXISTS "{}""#, table.name)),
            _ => unimplemented!(),
        }
    }

    let mut sql = format!(r#"CREATE TABLE "{}" ( "#, table.name);
    if table.has_meta {
        sql.push_str(&meta_column_ddl(db_kind));
        sql.push_str(",");
    };
    sql.push_str(&format!(" {})", column_clauses.join(", ")));
    ddl.push(sql);

    Ok(ddl)
}
