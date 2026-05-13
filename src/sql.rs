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
use rltbl_db::db_kind::DbKind;

//////////////////////////////////////////
// External imports
//////////////////////////////////////////
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value as JsonValue;

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
            DbKind::PostgreSQL => {
                self.index += 1;
                format!("${}", self.index)
            }
            DbKind::SQLite => "?".to_string(),
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

/// Given a SQL string (whose syntax is appropriate for the given database kind) that may
/// include placeholders representing bound parameters, and (optionally) a vector with the
/// parameter values corresponding to each placeholder, combine this information into an
/// interpolated string that is then returned.
pub fn interpolate_sql(sql: &str, params: Option<&JsonValue>, kind: &DbKind) -> Result<String> {
    tracing::trace!("interpolate_sql({sql}, {params:?}, {kind:?})");
    let params = match params {
        Some(JsonValue::Array(params)) => params.iter().collect::<Vec<_>>(),
        None => vec![],
        Some(params) => {
            tracing::warn!("Invalid parameter list: {params:?}");
            vec![]
        }
    };

    let mut final_sql = String::from("");
    let mut saved_start = 0;

    let quotes = r#"('[^'\\]*(?:\\.[^'\\]*)*'|"[^"\\]*(?:\\.[^"\\]*)*")"#;
    let rx = match kind {
        DbKind::SQLite => Regex::new(&format!(r#"{}|\B[?]\B"#, quotes))?,
        DbKind::PostgreSQL => Regex::new(&format!(r#"{}|\B[$]\d+\b"#, quotes))?,
    };

    let mut param_index = 0;
    for m in rx.find_iter(&sql) {
        let this_match = &sql[m.start()..m.end()];
        final_sql.push_str(&sql[saved_start..m.start()]);
        if !((this_match.starts_with("\"") && this_match.ends_with("\""))
            || (this_match.starts_with("'") && this_match.ends_with("'")))
        {
            let param = params.get(param_index);
            match param {
                None => {
                    return Err(RelatableError::InputError(format!(
                        "No parameter at index {param_index}"
                    ))
                    .into())
                }
                Some(param) => {
                    match param {
                        JsonValue::String(param) => final_sql.push_str(&format!("'{param}'")),
                        JsonValue::Number(param) => final_sql.push_str(&format!("{param}")),
                        JsonValue::Bool(param) => final_sql.push_str(&param.to_string()),
                        JsonValue::Array(param) => final_sql.push_str(&format!("{param:?}")),
                        JsonValue::Object(param) => final_sql.push_str(&format!("{param:?}")),
                        // We should never get a NULL in the parameter list, actually, but we
                        // handle it anyway.
                        JsonValue::Null => final_sql.push_str(&"NULL".to_string()),
                    };
                }
            };
            param_index += 1;
        } else {
            final_sql.push_str(&format!("{}", this_match));
        }
        saved_start = m.start() + this_match.len();
    }
    final_sql.push_str(&sql[saved_start..]);
    Ok(final_sql)
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
        DbKind::SQLite => "IS".into(),
        DbKind::PostgreSQL => "IS NOT DISTINCT FROM".into(),
    }
}

/// Helper function to deal with alternative "IS NOT" syntax for different SQL flavours
pub fn is_not_clause(db_kind: &DbKind) -> String {
    tracing::trace!("is_not_clause({db_kind:?})");
    match db_kind {
        DbKind::SQLite => "IS NOT".into(),
        DbKind::PostgreSQL => "IS DISTINCT FROM".into(),
    }
}

/// Generates a SQL clause to cast the column as text using the syntax appropriate for the given
/// database kind.
pub fn cast_column_as_text(column: &str, db_kind: &DbKind) -> String {
    tracing::trace!("cast_column_as_text({column}, {db_kind:?})");
    match db_kind {
        DbKind::SQLite => format!(r#"CAST("{column}" AS TEXT)"#),
        DbKind::PostgreSQL => format!(r#""{column}"::TEXT"#),
    }
}

/// Generates an SQL clause for a regular expression match on the given column
pub fn regexp_match(column: &str, sql_param: &mut SqlParam) -> String {
    tracing::trace!("regexp_match({column}, {sql_param:?})");
    let casted_column = cast_column_as_text(column, &sql_param.kind);
    match &sql_param.kind {
        DbKind::SQLite => format!(r#"regexp({}, {}) = 1"#, sql_param.next(), casted_column),
        DbKind::PostgreSQL => format!(r#"{} ~ {}"#, casted_column, sql_param.next()),
    }
}

/// Generates an SQL clause for a regular expression mismatch on the given column
pub fn regexp_mismatch(column: &str, sql_param: &mut SqlParam) -> String {
    tracing::trace!("regexp_mismatch({column}, {sql_param:?})");
    let casted_column = cast_column_as_text(column, &sql_param.kind);
    match &sql_param.kind {
        DbKind::SQLite => format!(r#"regexp({}, {}) = 0"#, sql_param.next(), casted_column),
        DbKind::PostgreSQL => format!(r#"{} !~ {}"#, casted_column, sql_param.next()),
    }
}

/////////////////
// Functions for generating DDL
////////////////

/// Generate DDL to create the given table in the database. If `force` is set, drop the table
/// first.
pub fn generate_table_ddl(
    table: &Table,
    columns: &[&Column],
    datatypes: &Datatypes,
    force: bool,
    db_kind: &DbKind,
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
            DbKind::PostgreSQL => {
                ddl.push(format!(r#"DROP TABLE IF EXISTS "{}" CASCADE"#, table.name))
            }
            DbKind::SQLite => ddl.push(format!(r#"DROP TABLE IF EXISTS "{}""#, table.name)),
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

/// Generate the DDL for creating the default view on the given table,
pub(crate) fn generate_default_view_ddl(
    table_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<&Column>,
    kind: &DbKind,
) -> Vec<String> {
    let view_name = format!("{table_name}_default_view");
    // Note that '?' parameters are not allowed in views so we must hard code them:
    match kind {
        DbKind::SQLite => vec![
            format!(r#"DROP VIEW IF EXISTS "{}""#, view_name),
            format!(
                r#"CREATE VIEW "{view}" AS
                     SELECT
                       {id_col} AS _id,
                       {order_col} AS _order,
                       (SELECT "change_id"
                        FROM "history"
                        WHERE "table" = '{table}'
                        AND "row" = {id_col}
                        ORDER BY "change_id" DESC
                        LIMIT 1
                       ) AS _change_id,
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
                    .map(|c| format!(r#""{}""#, c.column))
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        ],
        DbKind::PostgreSQL => vec![format!(
            r#"CREATE OR REPLACE VIEW "{view}" AS
                 SELECT
                   "{id_col}" AS _id,
                   "{order_col}" AS _order,
                   (
                     SELECT "change_id"
                     FROM "history"
                     WHERE "table" = '{table}'
                     AND "row" = {id_col}
                     ORDER BY "change_id" DESC
                     LIMIT 1
                   ) AS _change_id,
                   (
                     SELECT ('['::TEXT || string_agg(h.after, ','::TEXT)) || ']'::TEXT
                     FROM ( SELECT "history"."after"
                            FROM "history"
                            WHERE "history"."table" = '{table}'
                            AND "after" IS DISTINCT FROM NULL
                            AND "row" = "{id_col}"
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
                     WHERE "message"."table" = '{table}' AND "message"."row" = "{id_col}"
                     ORDER BY "message"."column", "message"."message_id") m
                   ) AS "_message",
                   {columns}
                     FROM "{table}""#,
            table = table_name,
            view = view_name,
            columns = columns
                .iter()
                .map(|c| format!(r#""{}""#, c.column))
                .collect::<Vec<_>>()
                .join(", "),
        )],
    }
}

/// Use the given components of a sprintf-style format string (<https://sqlite.org/printf.html>) to
/// construct and return a (numeric) format string suitable for use with PostgreSQL's
/// [to_char()](https://www.postgresql.org/docs/9.0/functions-formatting.html).
pub fn sprintf_to_pg_char(
    flag_opt: &str,
    width_opt: &str,
    precision_opt: &str,
    format_type: &str,
) -> String {
    tracing::trace!("sprintf_to_pg_char({flag_opt}, {width_opt}, {precision_opt}, {format_type})");

    // We only deal with numeric formats:
    if format_type == "s" {
        if flag_opt != "" || width_opt != "" || precision_opt != "" {
            tracing::warn!(
                "Ignoring options: flag: '{flag_opt}', width: '{width_opt}', precision: \
                 '{precision_opt}' for format type '{format_type}'"
            );
        }
        return "".to_string();
    }

    let default_width = 99;
    let default_precision = 6;
    let mut zero_pad = false;
    let mut pm_sign = false;
    let mut comma_sep = false;

    match flag_opt {
        "" => (),
        "0" => zero_pad = true,
        "+" => pm_sign = true,
        "," => comma_sep = true,
        "-" => tracing::warn!("Flag '-' is unsupported"),
        " " => tracing::warn!("Flag ' ' is unsupported"),
        "#" => tracing::warn!("Flag '#' is unsupported"),
        "!" => tracing::warn!("Flag '!' is unsupported"),
        invalid => tracing::warn!("Invalid flag: '{invalid}'"),
    };

    let width = match width_opt {
        "" => default_width,
        width => match width.parse::<usize>() {
            Ok(width) => width,
            Err(err) => {
                tracing::warn!("Could not parse width: {err}");
                default_width
            }
        },
    };

    let precision = match precision_opt {
        "" => default_precision,
        precision => match precision.parse::<usize>() {
            Ok(precision) => precision,
            Err(err) => {
                tracing::warn!("Could not parse precision: {err}");
                default_precision
            }
        },
    };

    let mut to_char_format = "".to_string();
    if zero_pad {
        to_char_format.push_str("0");
    } else if pm_sign {
        to_char_format.push_str("SG");
    }

    let mut digits = "".to_string();
    for (i, _) in (0..width).enumerate() {
        digits.push_str("9");
        let i = i + 1;
        if comma_sep && i != width {
            if i % 3 == 0 {
                digits.push_str(",");
            }
        }
    }
    let digits = digits.chars().rev().collect::<String>();
    to_char_format.push_str(&digits);

    if precision > 0 {
        to_char_format.push_str(".");
        for _ in 0..precision {
            to_char_format.push_str("9");
        }
    }

    tracing::debug!("Using to_char() format for PostgreSQL: '{to_char_format}'");
    to_char_format
}

/// Split the given sprintf-style format string (<https://sqlite.org/printf.html>) into its various
/// components, returning the optional flag, width, precision, and conversion specifications that
/// make up the format string. If no format is given, "%s" is assumed, which yields the returned
/// tuple: ("", "", "", "s").
pub fn split_sprintf_format(sprintf_format: &str) -> (String, String, String, String) {
    tracing::trace!("split_sprintf_format({sprintf_format:?})");

    let sprintf_regex = Regex::new(r#"^%([\-+ 0#,!])?([1-9]+)?((.)([0-9]+))?(\w)$"#).unwrap();
    let valid_format_types = ["d", "i", "c", "o", "u", "x", "e", "f", "g", "a", "s"];

    match sprintf_format {
        "" => (
            "".to_string(),
            "".to_string(),
            "".to_string(),
            "s".to_string(),
        ),
        sprintf_format => match sprintf_regex.captures(sprintf_format) {
            None => {
                tracing::warn!("Illegal format: '{}'", sprintf_format);
                (
                    "".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "s".to_string(),
                )
            }
            Some(captures) => {
                let flag_opt = captures
                    .get(1)
                    .and_then(|c| Some(c.as_str().to_string()))
                    .unwrap_or("".to_string());
                let width_opt = captures
                    .get(2)
                    .and_then(|c| Some(c.as_str().to_string()))
                    .unwrap_or("".to_string());
                let precision_opt = captures
                    .get(5)
                    .and_then(|c| Some(c.as_str().to_string()))
                    .unwrap_or("".to_string());
                let mut format_type = &captures[6];
                if !valid_format_types.contains(&format_type) {
                    tracing::warn!("Invalid format type: '{format_type}'");
                    format_type = "s";
                }

                (flag_opt, width_opt, precision_opt, format_type.to_string())
            }
        },
    }
}

/// Generate the DDL for creating the text view on the given table,
pub(crate) fn generate_text_view_ddl(
    datatypes: &Datatypes,
    table_name: &str,
    id_col: &str,
    order_col: &str,
    columns: &Vec<&Column>,
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
            let format = datatypes
                .get(&column.datatype)
                .and_then(|dt| Some(dt.format.clone()))
                .unwrap_or_default();
            let column_cast = {
                let (flag_opt, width_opt, precision_opt, format_type) =
                    split_sprintf_format(&format);
                if *kind == DbKind::SQLite {
                    let dt_format = format!(
                        "%{flag_opt}{width_opt}{precision_opt}{format_type}",
                        precision_opt = match precision_opt.as_str() {
                            "" => "".to_string(),
                            _ => format!(".{precision_opt}"),
                        }
                    );
                    tracing::debug!("Formatting column '{}' using '{dt_format}'", column.column);
                    format!(r#"FORMAT('{}', "{}")"#, dt_format, column.column)
                } else {
                    match sprintf_to_pg_char(&flag_opt, &width_opt, &precision_opt, &format_type)
                        .as_str()
                    {
                        "" => format!(r#""{}"::TEXT"#, column.column),
                        dt_format => {
                            format!(r#"LTRIM(TO_CHAR("{}", '{dt_format}'), ' ')"#, column.column)
                        }
                    }
                }
            };
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
                column = column.column,
                is_clause = is_clause(kind)
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
        .map(|column| format!(r#"t."{}""#, column.column))
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
