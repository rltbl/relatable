use rltbl::{
    core::RelatableError,
    sql::{
        begin, connect, json_to_string, lock_connection, query, query_one, query_tx, query_value,
        query_value_tx, DbConnection, JsonRow, VecInto,
    },
};
use std::fmt::Display;
use std::{io::Write, path::Path as FilePath};

use anyhow::Result;
use async_std::sync::Arc;
use axum::http::header;
use axum::{
    body::Body,
    extract::{Json as ExtractJson, Path, Query, State},
    http::{HeaderMap, Response, StatusCode},
    response::{Html, IntoResponse, Json, Redirect},
    routing::{get, post},
    Form, Router,
};
use axum_session::{Session, SessionConfig, SessionLayer, SessionNullPool, SessionStore};
use clap::{ArgAction, Parser, Subcommand};
use clap_verbosity_flag::Verbosity;
use futures::executor::block_on;
use indexmap::IndexMap;
use minijinja::{context, path_loader, Environment};
use rand::rngs::StdRng;
use rand::seq::IteratorRandom as _;
use rand::Rng as _;
use rand::SeedableRng as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{from_str, json, to_string_pretty, to_value, Value as JsonValue};
use tabwriter::TabWriter;
use tokio::net::TcpListener;
use tower_service::Service;

#[derive(Debug)]
pub struct Relatable {
    pub connection: DbConnection,
    // pub minijinja: Environment<'static>,
    pub default_limit: usize,
    pub max_limit: usize,
}

impl Relatable {
    pub async fn default() -> Result<Self> {
        // Set up database connection.
        let path = ".relatable/relatable.db";
        let connection = connect(path).await?;
        Ok(Self {
            connection,
            // minijinja: env,
            default_limit: 100,
            max_limit: 1000,
        })
    }

    pub fn render<T: Serialize>(&self, template: &str, context: T) -> Result<String> {
        // TODO: Optionally we should set up the environment once and store it,
        // but during development it's very convenient to rebuild every time.
        let mut env = Environment::new();

        // Load default template strings at compile time.
        let templates = IndexMap::from([("table.html", include_str!("templates/table.html"))]);

        // Load templates dynamically if src/templates/ exists,
        // otherwise use strings from compile time.
        // TODO: This should be a configuration option.
        let dir = "src/templates/";
        if FilePath::new(dir).is_dir() {
            env.set_loader(path_loader(dir));
        };
        for (name, content) in templates {
            match env.get_template(name) {
                Ok(_) => (),
                Err(_) => env.add_template(name, content).unwrap(),
            }
        }

        env.get_template(template)?
            .render(context)
            .map_err(|e| e.into())
    }

    pub async fn get_table(&self, table_name: &str) -> Result<Table> {
        let statement = r#"SELECT max(change_id) FROM history WHERE "table" = ?"#;
        let params = json!([table_name]);
        let change_id = match query_value(&self.connection, &statement, Some(&params)).await? {
            Some(value) => value.as_u64().unwrap_or_default() as usize,
            None => 0,
        };
        Ok(Table {
            name: table_name.to_string(),
            change_id,
            ..Default::default()
        })
    }

    pub fn from(&self, table_name: &str) -> Select {
        Select {
            table_name: table_name.to_string(),
            limit: self.default_limit,
            ..Default::default()
        }
    }

    pub async fn fetch_columns(&self, table_name: &str) -> Result<Vec<Column>> {
        // WARN: SQLite only!
        let statement = format!(r#"SELECT * FROM pragma_table_info("{table_name}");"#);
        Ok(query(&self.connection, &statement, None)
            .await?
            .iter()
            .map(|row| Column {
                name: row.get_string("name"),
            })
            .filter(|c| !c.name.starts_with("_"))
            .collect())
    }

    pub async fn fetch(&self, select: &Select) -> Result<ResultSet> {
        let table = self.get_table(&select.table_name).await?;
        let columns = self.fetch_columns(&select.table_name).await?;
        let (statement, params) = select.to_sqlite()?;
        tracing::debug!("SQL {statement}");
        let params = json!(params);
        let json_rows = query(&self.connection, &statement, Some(&params)).await?;

        let count = json_rows.len();
        let total = match json_rows.get(0) {
            Some(row) => row
                .content
                .get("_total")
                .and_then(|x| x.as_u64())
                .unwrap_or(0) as usize,
            None => 0,
        };

        let rows: Vec<Row> = json_rows.vec_into();
        Ok(ResultSet {
            range: Range {
                count,
                total,
                start: select.offset + 1,
                end: select.offset + count,
            },
            select: select.clone(),
            table,
            columns,
            rows,
        })
    }

    pub async fn fetch_json_rows(&self, select: &Select) -> Result<Vec<JsonRow>> {
        let (statement, params) = select.to_sqlite()?;
        let params = json!(params);
        query(&self.connection, &statement, Some(&params)).await
    }

    pub async fn set_values(&self, changeset: &ChangeSet) -> Result<()> {
        let action = changeset.action.to_string();
        let user = changeset.user.clone();
        let table = changeset.table.clone();
        let description = changeset.description.clone();

        // Get the connection and begin a transaction:
        let mut locked_conn = lock_connection(&self.connection).await;
        let mut tx = begin(&self.connection, &mut locked_conn).await?;

        // Make sure the user is present.
        let color = random_color::RandomColor::new().to_hex();
        let statement = r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#;
        let params = json!([user, color]);
        query_tx(&mut tx, &statement, Some(&params)).await?;

        // Update the user's cursor position.
        let cursor = changeset.to_cursor()?;
        let statement =
            r#"UPDATE user SET "cursor" = ?, "datetime" = CURRENT_TIMESTAMP WHERE "name" = ?"#;
        let params = json!([to_value(cursor).unwrap_or_default(), user]);
        query_value_tx(&mut tx, &statement, Some(&params)).await?;

        let statement = r#"INSERT INTO change('user', 'action', 'table', 'description', 'content')
                           VALUES (?, ?, ?, ?, ?)
                           RETURNING change_id"#;
        let content = to_value(&changeset.changes).unwrap_or_default();
        let params = json!([user, action, table, description, content]);
        let change_id = query_value_tx(&mut tx, &statement, Some(&params)).await?;
        let change_id = change_id
            .expect("a change_id")
            .as_u64()
            .expect("an integer");

        for change in &changeset.changes {
            tracing::debug!("CHANGE {change:?}");
            match change {
                Change::Update { row, column, value } => {
                    let statement = r#"INSERT INTO history
                                       ('change_id', 'table', 'row', 'before', 'after')
                                       VALUES (?, ?, ?, 'TODO', 'TODO')
                                       RETURNING history_id"#;
                    let params = json!([change_id, table, row]);
                    query_value_tx(&mut tx, &statement, Some(&params)).await?;

                    // WARN: This just sets text!
                    let statement =
                        format!(r#"UPDATE "{table}" SET "{column}" = ? WHERE _id = ?"#,);
                    // TODO: Render JSON to SQL properly.
                    let value = json_to_string(value);
                    let params = json!([value, row]);
                    query_tx(&mut tx, &statement, Some(&params)).await?;
                }
            }
        }

        // Commit the transaction:
        tx.commit().await?;

        Ok(())
    }

    pub async fn get_user(&self, username: &str) -> Account {
        let statement = format!(r#"SELECT * FROM user WHERE name = '{username}' LIMIT 1"#);
        let user = query_one(&self.connection, &statement).await;
        if let Ok(user) = user {
            if let Some(user) = user {
                return Account {
                    name: username.to_string(),
                    color: user.get_string("color"),
                };
            }
        }
        Account {
            ..Default::default()
        }
    }

    pub async fn get_users(&self) -> Result<IndexMap<String, UserCursor>> {
        let mut users = IndexMap::new();
        // let statement = format!(
        //     r#"SELECT * FROM user WHERE cursor IS NOT NULL
        //        AND "datetime" >= DATETIME('now', '-10 minutes')"#
        // );
        let statement = format!(r#"SELECT * FROM user WHERE cursor IS NOT NULL"#);
        let rows = query(&self.connection, &statement, None).await?;
        for row in rows {
            let name = row.get_string("name");
            users.insert(
                name.clone(),
                UserCursor {
                    name: name.clone(),
                    color: row.get_string("color"),
                    cursor: from_str(&row.get_string("cursor"))?,
                    datetime: row.get_string("datetime"),
                },
            );
        }
        Ok(users)
    }

    pub async fn get_tables(&self) -> Result<IndexMap<String, Table>> {
        let mut tables = IndexMap::new();
        let statement = format!(
            r#"SELECT *,
                 (SELECT max(change_id)
                  FROM history
                  WHERE history."table" = "table"."table"
                 ) AS _change_id
               FROM 'table'"#
        );

        let rows = query(&self.connection, &statement, None).await?;
        for row in rows {
            let name = row.get_string("table");
            tables.insert(
                name.clone(),
                Table {
                    name: name.clone(),
                    change_id: row
                        .content
                        .get("_change_id")
                        .and_then(|i| i.as_u64())
                        .unwrap_or_default() as usize,
                    ..Default::default()
                },
            );
        }
        Ok(tables)
    }

    pub async fn get_site(&self, username: &str) -> Site {
        let mut users = self.get_users().await.unwrap_or_default();
        users.shift_remove(username);
        Site {
            title: "RLTBL".to_string(),
            root: "".to_string(),
            user: self.get_user(username).await,
            users,
            tables: self.get_tables().await.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeSet {
    action: ChangeAction,
    table: String,
    user: String,
    description: String,
    changes: Vec<Change>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChangeAction {
    Do,
    Undo,
    Redo,
}

impl Display for ChangeAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeAction::Do => write!(f, "do"),
            ChangeAction::Undo => write!(f, "undo"),
            ChangeAction::Redo => write!(f, "redo"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Change {
    Update {
        row: usize,
        column: String,
        value: JsonValue,
    },
    // Add
    // Delete
    // Move
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    value: JsonValue,
    text: String,
}

impl From<&JsonValue> for Cell {
    fn from(value: &JsonValue) -> Self {
        Self {
            value: value.clone(),
            text: match value {
                JsonValue::String(value) => value.to_string(),
                value => format!("{value}"),
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    id: usize,
    order: usize,
    change_id: usize,
    cells: IndexMap<String, Cell>,
}

// Ignore columns that start with "_"
impl From<JsonRow> for Row {
    fn from(row: JsonRow) -> Self {
        Self {
            id: row
                .content
                .get("_id")
                .and_then(|i| i.as_u64())
                .unwrap_or_default() as usize,
            order: row
                .content
                .get("_order")
                .and_then(|i| i.as_u64())
                .unwrap_or_default() as usize,
            change_id: row
                .content
                .get("_change_id")
                .and_then(|i| i.as_u64())
                .unwrap_or_default() as usize,
            cells: row
                .content
                .iter()
                .filter(|(k, _)| !k.starts_with("_"))
                .map(|(k, v)| (k.clone(), v.into()))
                .collect(),
        }
    }
}

impl From<Row> for Vec<String> {
    fn from(row: Row) -> Self {
        row.to_strings()
    }
}

impl Row {
    fn to_strings(&self) -> Vec<String> {
        self.cells.values().map(|cell| cell.text.clone()).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    name: String,
    // sqltype: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Table {
    name: String,
    // The history_id of the most recent update to this table.
    change_id: usize,
    editable: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Range {
    count: usize,
    total: usize,
    start: usize,
    end: usize,
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rows {}-{} of {}", self.start, self.end, self.total)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ResultSet {
    select: Select,
    range: Range,
    table: Table,
    columns: Vec<Column>,
    rows: Vec<Row>,
}

impl std::fmt::Display for ResultSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tw = TabWriter::new(vec![]);
        tw.write(format!("{}\n", self.range).as_bytes())
            .unwrap_or_default();
        let header = &self
            .columns
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<String>>();
        tw.write(format!("{}\n", header.join("\t")).as_bytes())
            .unwrap_or_default();
        for row in &self.rows {
            tw.write(format!("{}\n", row.to_strings().join("\t")).as_bytes())
                .unwrap_or_default();
        }
        tw.flush().expect("TabWriter to flush");
        let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
        write!(f, "{written}")
    }
}

// Web Site Stuff

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
    title: String,
    root: String,
    user: Account,
    users: IndexMap<String, UserCursor>,
    tables: IndexMap<String, Table>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Account {
    name: String,
    color: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cursor {
    table: String,
    row: usize,
    column: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserCursor {
    name: String,
    color: String,
    cursor: Cursor,
    datetime: String,
}

impl ChangeSet {
    fn to_cursor(&self) -> Result<Cursor> {
        let table = self.table.clone();
        match self.changes.first() {
            Some(change) => match change {
                Change::Update {
                    row,
                    column,
                    value: _,
                } => Ok(Cursor {
                    table,
                    row: *row,
                    column: column.to_string(),
                }),
            },
            None => Err(RelatableError::ChangeError("No changes in set".into()).into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Filter {
    Equal { column: String, value: JsonValue },
    GreaterThan { column: String, value: JsonValue },
    GreaterThanOrEqual { column: String, value: JsonValue },
    LessThan { column: String, value: JsonValue },
    LessThanOrEqual { column: String, value: JsonValue },
    Is { column: String, value: JsonValue },
    In { column: String, value: JsonValue },
}

fn render_in_not_in<S: Into<String>>(
    lhs: S,
    options: &Vec<JsonValue>,
    positive: bool,
) -> Result<String> {
    let negation;
    if !positive {
        negation = " NOT";
    } else {
        negation = "";
    }

    let mut values = vec![];
    let mut is_string_list = false;
    for (i, option) in options.iter().enumerate() {
        match option {
            JsonValue::String(s) => {
                if i == 0 {
                    is_string_list = true;
                } else if !is_string_list {
                    return Err(RelatableError::InputError(format!(
                        "{:?} contains both text and numeric types.",
                        options
                    ))
                    .into());
                }
                values.push(format!("{s}"))
            }
            JsonValue::Number(n) => {
                if i == 0 {
                    is_string_list = false;
                } else if is_string_list {
                    return Err(RelatableError::InputError(format!(
                        "{:?} contains both text and numeric types.",
                        options
                    ))
                    .into());
                }
                values.push(format!("{n}"))
            }
            _ => {
                return Err(RelatableError::InputError(format!(
                    "{:?} is not an array of strings or numbers.",
                    options
                ))
                .into())
            }
        };
    }
    let value_list = format!("({})", values.join(", "));
    let filter_sql = format!("{}{} IN {}", lhs.into(), negation, value_list);
    Ok(filter_sql)
}

impl Display for Filter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: This should be factored out.
        fn json_to_string(value: &JsonValue) -> String {
            match value {
                JsonValue::Null => "NULL".to_string(),
                JsonValue::Bool(value) => value.to_string(),
                JsonValue::Number(value) => value.to_string(),
                JsonValue::String(value) => format!("'{value}'"),
                JsonValue::Array(value) => format!("'{value:?}'"),
                JsonValue::Object(value) => format!("'{value:?}'"),
            }
        }
        let result = match self {
            Filter::Equal { column, value } => {
                let value = json_to_string(&value);
                format!(r#""{column}" = {value}"#)
            }
            Filter::GreaterThan { column, value } => {
                let value = json_to_string(&value);
                format!(r#""{column}" > {value}"#)
            }
            Filter::GreaterThanOrEqual { column, value } => {
                let value = json_to_string(&value);
                format!(r#""{column}" >= {value}"#)
            }
            Filter::LessThan { column, value } => {
                let value = json_to_string(&value);
                format!(r#""{column}" < {value}"#)
            }
            Filter::LessThanOrEqual { column, value } => {
                let value = json_to_string(&value);
                format!(r#""{column}" <= {value}"#)
            }
            Filter::Is { column, value } => {
                // Note that we are presupposing SQLite syntax which is not universal for IS:
                let value = json_to_string(&value);
                format!(r#""{column}" IS {value}"#)
            }
            Filter::In { column, value } => {
                if let JsonValue::Array(values) = value {
                    let filter_str = match render_in_not_in(column, values, true) {
                        Err(_) => return Err(std::fmt::Error),
                        Ok(filter_str) => filter_str,
                    };
                    format!("{filter_str}")
                } else {
                    return Err(std::fmt::Error);
                }
            }
        };
        write!(f, "{result}")
    }
}

pub type QueryParams = IndexMap<String, String>;

pub enum Format {
    Html,
    Json,
    PrettyJson,
    Default,
}

impl TryFrom<&String> for Format {
    fn try_from(path: &String) -> Result<Self> {
        let path = path.to_lowercase();
        let format = if path.ends_with(".pretty.json") {
            Format::PrettyJson
        } else if path.ends_with(".json") {
            Format::Json
        } else if path.ends_with(".html") || path.ends_with(".htm") {
            Format::Html
        } else if path.contains(".") {
            return Err(
                RelatableError::FormatError(format!("Unknown format for path {path}")).into(),
            );
        } else {
            Format::Default
        };
        Ok(format)
    }

    type Error = anyhow::Error;
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Select {
    pub table_name: String,
    pub limit: usize,
    pub offset: usize,
    pub filters: Vec<Filter>,
}

impl Select {
    pub fn from_path_and_query(rltbl: &Relatable, path: &str, query_params: &QueryParams) -> Self {
        let table_name = path.split(".").next().unwrap_or_default().to_string();
        let mut filters = Vec::new();
        for (column, pattern) in query_params {
            if pattern.starts_with("eq.") {
                let column = column.to_string();
                let value = serde_json::from_str(&pattern.replace("eq.", ""));
                match value {
                    Ok(value) => filters.push(Filter::Equal { column, value }),
                    Err(_) => tracing::warn!("invalid filter value {pattern}"),
                }
            } else if pattern.starts_with("gt.") {
                let column = column.to_string();
                let value = serde_json::from_str(&pattern.replace("gt.", ""));
                match value {
                    Ok(value) => filters.push(Filter::GreaterThan { column, value }),
                    Err(_) => tracing::warn!("invalid filter value {pattern}"),
                }
            } else if pattern.starts_with("gte.") {
                let column = column.to_string();
                let value = serde_json::from_str(&pattern.replace("gte.", ""));
                match value {
                    Ok(value) => filters.push(Filter::GreaterThanOrEqual { column, value }),
                    Err(_) => tracing::warn!("invalid filter value {pattern}"),
                }
            } else if pattern.starts_with("lt.") {
                let column = column.to_string();
                let value = serde_json::from_str(&pattern.replace("lt.", ""));
                match value {
                    Ok(value) => filters.push(Filter::LessThan { column, value }),
                    Err(_) => tracing::warn!("invalid filter value {pattern}"),
                }
            } else if pattern.starts_with("lte.") {
                let column = column.to_string();
                let value = serde_json::from_str(&pattern.replace("lte.", ""));
                match value {
                    Ok(value) => filters.push(Filter::LessThanOrEqual { column, value }),
                    Err(_) => tracing::warn!("invalid filter value {pattern}"),
                }
            } else if pattern.starts_with("is.") {
                let column = column.to_string();
                let value = pattern.replace("is.", "");
                match value.to_lowercase().as_str() {
                    "null" => filters.push(Filter::Is {
                        column,
                        value: JsonValue::Null,
                    }),
                    _ => match serde_json::from_str(&value) {
                        Ok(value) => filters.push(Filter::Is { column, value }),
                        Err(_) => tracing::warn!("invalid filter value {pattern}"),
                    },
                };
            } else if pattern.starts_with("in.") {
                let column = column.to_string();
                let value = pattern.replace("in.", "");
                println!("COLUMN: {column}, VALUE: {value}");
                todo!()
            }
        }
        let limit: usize = query_params
            .get("limit")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or(rltbl.default_limit)
            .min(rltbl.max_limit);
        let offset: usize = query_params
            .get("offset")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or_default();
        Self {
            table_name,
            limit,
            offset,
            filters,
            ..Default::default()
        }
    }

    pub fn limit(mut self, limit: &usize) -> Self {
        self.limit = *limit;
        self
    }

    pub fn offset(mut self, offset: &usize) -> Self {
        self.offset = *offset;
        self
    }

    pub fn filters(mut self, filters: &Vec<String>) -> Result<Self> {
        let eq = Regex::new(r#"^(\w+)\s*=\s*"?(\w+)"?$"#).unwrap();
        let gt = Regex::new(r"^(\w+)\s*>\s*(\w+)$").unwrap();
        let gte = Regex::new(r"^(\w+)\s*>=\s*(\w+)$").unwrap();
        let lt = Regex::new(r"^(\w+)\s*<\s*(\w+)$").unwrap();
        let lte = Regex::new(r"^(\w+)\s*<=\s*(\w+)$").unwrap();
        let is = Regex::new(r#"^(\w+)\s+(IS|is)\s+"?(\w+)"?$"#).unwrap();
        let is_in = Regex::new(r#"^(\w+)\s+(IN|in)\s+\((\w+(,\s*\w+)*)\)$"#).unwrap();
        // Used for text types:
        let maybe_quote_value = |value: &str| -> Result<JsonValue> {
            if value.starts_with("\"") {
                let value = serde_json::from_str(&value)?;
                Ok(value)
            } else {
                let value = serde_json::from_str(&format!(r#""{value}""#))?;
                Ok(value)
            }
        };
        for filter in filters {
            if eq.is_match(&filter) {
                let captures = eq.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::Equal { column, value });
            } else if gt.is_match(&filter) {
                let captures = gt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = serde_json::from_str(&value)?;
                self.filters.push(Filter::GreaterThan { column, value });
            } else if gte.is_match(&filter) {
                let captures = gte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = serde_json::from_str(&value)?;
                self.filters
                    .push(Filter::GreaterThanOrEqual { column, value });
            } else if lt.is_match(&filter) {
                let captures = lt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = serde_json::from_str(&value)?;
                self.filters.push(Filter::LessThan { column, value });
            } else if lte.is_match(&filter) {
                let captures = lte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = serde_json::from_str(&value)?;
                self.filters.push(Filter::LessThanOrEqual { column, value });
            } else if is.is_match(&filter) {
                let captures = is.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(3).unwrap().as_str();
                let value = match value.to_lowercase().as_str() {
                    "null" => JsonValue::Null,
                    _ => maybe_quote_value(&value)?,
                };
                self.filters.push(Filter::Is { column, value });
            } else if is_in.is_match(&filter) {
                let captures = is_in.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let values = &captures.get(3).unwrap().as_str();
                let separator = Regex::new(r"\s*,\s*").unwrap();
                let values = separator
                    .split(values)
                    .map(|v| serde_json::from_str::<JsonValue>(v).unwrap_or(json!(v.to_string())))
                    .collect::<Vec<_>>();
                self.filters.push(Filter::In {
                    column,
                    value: json!(values),
                });
            } else {
                return Err(RelatableError::ConfigError(format!("invalid filter {filter}")).into());
            }
        }
        Ok(self)
    }

    pub fn eq<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::Equal {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn gt<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::GreaterThan {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn gte<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::GreaterThanOrEqual {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn lt<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::LessThan {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn lte<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::LessThanOrEqual {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn is<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::Is {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn is_in<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        self.filters.push(Filter::In {
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    pub fn to_sqlite(&self) -> Result<(String, Vec<JsonValue>)> {
        tracing::debug!("to_sqlite: {self:?}");
        let table = &self.table_name;
        let mut lines = Vec::new();
        lines.push("SELECT *,".to_string());
        // WARN: The _total count should probably be optional.
        lines.push("  COUNT(1) OVER() AS _total,".to_string());
        lines.push(format!(
            r#"  (SELECT MAX(change_id) FROM history
                   WHERE "table" = ?
                     AND "row" = _id
                 ) AS _change_id"#
        ));
        lines.push(format!(r#"FROM "{}""#, self.table_name));
        for (i, filter) in self.filters.iter().enumerate() {
            let keyword = if i == 0 { "WHERE" } else { "  AND" };
            lines.push(format!("{keyword} {filter}"));
        }
        if self.limit > 0 {
            lines.push(format!("LIMIT {}", self.limit));
        }
        if self.offset > 0 {
            lines.push(format!("OFFSET {}", self.offset));
        }

        Ok((lines.join("\n"), vec![json!(table)]))
    }
}

// ### CLI Module

static COLUMN_HELP: &str = "A column name or label";
static ROW_HELP: &str = "A row number";
static TABLE_HELP: &str = "A table name";
static VALUE_HELP: &str = "A value for a cell";

#[derive(Parser, Debug)]
#[command(version,
          about = "Relatable (rltbl): Connect your data!",
          long_about = None)]
pub struct Cli {
    #[arg(long, default_value="", action = ArgAction::Set)]
    user: String,

    #[command(flatten)]
    verbose: Verbosity,

    // Subcommand:
    #[command(subcommand)]
    pub command: Command,
}

// Note that the subcommands are declared below in the order in which we want them to appear
// in the usage statement that is printed when valve is run with the option `--help`.
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Get data from the database
    Get {
        #[command(subcommand)]
        subcommand: GetSubcommand,
    },

    /// Set data in the database
    Set {
        #[command(subcommand)]
        subcommand: SetSubcommand,
    },

    /// Run a Relatable server
    Serve {
        /// Server host address
        #[arg(long, default_value="0.0.0.0", action = ArgAction::Set)]
        host: String,

        /// Server port
        #[arg(long, default_value="0", action = ArgAction::Set)]
        port: u16,
    },

    /// Generate a demonstration database
    Demo {
        /// Output format: text, JSON, TSV
        #[arg(long, action = ArgAction::SetTrue)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum GetSubcommand {
    /// Get the column header and rows from a given table.
    Table {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        /// Zero or more filters
        #[arg(value_name = "FILTERS", action = ArgAction::Set)]
        filters: Vec<String>,

        /// Output format: text, JSON, TSV
        #[arg(long, default_value="", action = ArgAction::Set)]
        format: String,

        /// Limit to this many rows
        #[arg(long, default_value="100", action = ArgAction::Set)]
        limit: usize,

        /// Offset by this many rows
        #[arg(long, default_value="0", action = ArgAction::Set)]
        offset: usize,
    },

    /// Get the rows from a given table.
    Rows {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        /// Limit to this many rows
        #[arg(long, default_value="100", action = ArgAction::Set)]
        limit: usize,

        /// Offset by this many rows
        #[arg(long, default_value="0", action = ArgAction::Set)]
        offset: usize,
    },

    /// Get the value of a given column of a given row from a given table.
    Value {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum SetSubcommand {
    /// Set the value of a given column of a given row from a given table.
    Value {
        #[arg(value_name = "TABLE", action = ArgAction::Set, help = TABLE_HELP)]
        table: String,

        #[arg(value_name = "ROW", action = ArgAction::Set, help = ROW_HELP)]
        row: usize,

        #[arg(value_name = "COLUMN", action = ArgAction::Set, help = COLUMN_HELP)]
        column: String,

        #[arg(value_name = "VALUE", action = ArgAction::Set, help = VALUE_HELP)]
        value: String,
    },
}

/// Given a vector of vectors of strings,
/// print text with "elastic tabstops".
pub fn print_text(rows: &Vec<Vec<String>>) -> Result<()> {
    let mut tw = TabWriter::new(vec![]);
    for row in rows {
        tw.write(format!("{}\n", row.join("\t")).as_bytes())?;
    }
    tw.flush()?;
    let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
    print!("{written}");
    Ok(())
}

pub fn print_tsv(rows: Vec<Vec<String>>) -> Result<()> {
    for row in rows {
        println!("{}", row.join("\t"));
    }
    Ok(())
}

// Print a table with its column header.
pub async fn print_table(
    _cli: &Cli,
    table_name: &str,
    filters: &Vec<String>,
    format: &str,
    limit: &usize,
    offset: &usize,
) -> Result<()> {
    tracing::debug!("print_table {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl
        .from(table_name)
        .filters(filters)?
        .limit(limit)
        .offset(offset);
    match format.to_lowercase().as_str() {
        "json" => {
            let json = json!(rltbl.fetch(&select).await?);
            print!("{}", to_string_pretty(&json)?);
        }
        "text" | "" => {
            print!("{}", rltbl.fetch(&select).await?.to_string());
        }
        _ => unimplemented!("output format {format}"),
    }

    Ok(())
}

// Print rows of a table, without column header.
pub async fn print_rows(_cli: &Cli, table_name: &str, limit: &usize, offset: &usize) -> Result<()> {
    tracing::debug!("print_rows {table_name}");
    let rltbl = Relatable::default().await?;
    let select = rltbl.from(table_name).limit(limit).offset(offset);
    let rows = rltbl.fetch_json_rows(&select).await?.vec_into();
    print_text(&rows)?;
    Ok(())
}

pub async fn print_value(cli: &Cli, table: &str, row: usize, column: &str) -> Result<()> {
    tracing::debug!("print_value({cli:?}, {table}, {row}, {column})");
    let rltbl = Relatable::default().await?;
    let statement = format!(r#"SELECT "{column}" FROM "{table}" WHERE _id = ?"#);
    let params = json!([row]);
    if let Some(value) = query_value(&rltbl.connection, &statement, Some(&params)).await? {
        let text = match value {
            JsonValue::String(value) => value.to_string(),
            value => format!("{value}"),
        };
        println!("{text}");
    }
    Ok(())
}

// Get the user from the CLI, RLTBL_USER environment variable,
// or the general environment.
pub fn get_cli_user(cli: &Cli) -> String {
    let mut username = cli.user.clone();
    if username == "" {
        username = std::env::var("RLTBL_USER").unwrap_or_default();
    }
    if username == "" {
        username = whoami::username();
    }
    username
}

pub async fn set_value(
    cli: &Cli,
    table: &str,
    row: usize,
    column: &str,
    value: &str,
) -> Result<()> {
    tracing::debug!("set_value({cli:?}, {table}, {row}, {column}, {value})");
    let rltbl = Relatable::default().await?;
    rltbl
        .set_values(&ChangeSet {
            user: get_cli_user(&cli),
            action: ChangeAction::Do,
            table: table.to_string(),
            description: "Set one value".to_string(),
            changes: vec![Change::Update {
                row,
                column: column.to_string(),
                value: to_value(value).unwrap_or_default(),
            }],
        })
        .await?;
    Ok(())
}

pub async fn build_demo(cli: &Cli, force: &bool) -> Result<()> {
    tracing::debug!("build_demo({cli:?}");
    let path = ".relatable/relatable.db";
    let dir = FilePath::new(path)
        .parent()
        .expect("parent should be defined");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
        tracing::info!("Created '{dir:?}' directory");
    }
    let file = FilePath::new(path);
    if file.exists() {
        if *force {
            std::fs::remove_file(&file)?;
            tracing::info!("Removed '{file:?}' file");
        } else {
            print!("File {file:?} already exists. Use --force to overwrite");
            return Err(
                RelatableError::ConfigError("Database file already exists".to_string()).into(),
            );
        }
    }

    // Create and populate the table table
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'table' (
      _id INTEGER UNIQUE,
      _order INTEGER UNIQUE,
      'table' TEXT PRIMARY KEY
    )"#;
    query(&rltbl.connection, sql, None).await?;

    let sql = "INSERT INTO 'table' VALUES (1, 1000, 'table'), (2, 2000, 'penguin')";
    query(&rltbl.connection, sql, None).await?;

    // Create the change and history tables
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'user' (
      'name' TEXT PRIMARY KEY,
      'color' TEXT,
      'cursor' TEXT,
      'datetime' TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Create the change and history tables
    let rltbl = Relatable::default().await?;
    let sql = r#"CREATE TABLE 'change' (
      change_id INTEGER PRIMARY KEY,
      'datetime' TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
      'user' TEXT NOT NULL,
      'action' TEXT NOT NULL,
      'table' TEXT NOT NULL,
      'description' TEXT,
      'content' TEXT,
      FOREIGN KEY ("user") REFERENCES user("name")
    )"#;
    query(&rltbl.connection, sql, None).await?;

    let sql = r#"CREATE TABLE 'history' (
      history_id INTEGER PRIMARY KEY,
      change_id INTEGER NOT NULL,
      'table' TEXT NOT NULL,
      'row' INTEGER NOT NULL,
      'before' TEXT,
      'after' TEXT,
      FOREIGN KEY ("change_id") REFERENCES change("change_id"),
      FOREIGN KEY ("table") REFERENCES "table"("table")
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Create the penguin table.
    let sql = r#"CREATE TABLE penguin (
      _id INTEGER UNIQUE,
      _order INTEGER UNIQUE,
      study_name TEXT,
      sample_number INTEGER,
      species TEXT,
      island TEXT,
      individual_id TEXT,
      culmen_length REAL,
      body_mass INTEGER
    )"#;
    query(&rltbl.connection, sql, None).await?;

    // Populate the penguin table with random data.
    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);
    let count = 1000;
    for i in 1..=count {
        let id = i;
        let order = i * 1000;
        let island = islands.iter().choose(&mut rng).unwrap();
        let culmen_length = rng.gen_range(300..500) as f64 / 10.0;
        let body_mass = rng.gen_range(1000..5000);
        let sql = r#"INSERT INTO 'penguin'
                     VALUES (?, ?, 'FAKE123', ?, 'Pygoscelis adeliae', ?, ?, ?, ?)"#;
        let params = json!([
            id,
            order,
            id,
            island,
            format!("N{id}"),
            culmen_length,
            body_mass,
        ]);
        query(&rltbl.connection, &sql, Some(&params)).await?;
    }

    Ok(())
}

fn get_404(error: &anyhow::Error) -> Response<Body> {
    (
        StatusCode::NOT_FOUND,
        Html(format!("404 Not Found: {error}")),
    )
        .into_response()
}

fn get_500(error: &anyhow::Error) -> Response<Body> {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Html(format!("500 Internal Server Error: {error}")),
    )
        .into_response()
}

async fn get_root() -> impl IntoResponse {
    tracing::info!("request root");
    Redirect::permanent("/table/table")
}

async fn main_js() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/javascript".parse().unwrap());
    (headers, include_str!("resources/main.js"))
}

async fn main_css() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
    (headers, include_str!("resources/main.css"))
}

async fn render_html(rltbl: &Relatable, username: &str, result: &ResultSet) -> Result<String> {
    let site = rltbl.get_site(username).await;
    rltbl.render("table.html", context! {site, result})
}

async fn render_response(rltbl: &Relatable, username: &str, result: &ResultSet) -> Response<Body> {
    match render_html(rltbl, username, result).await {
        Ok(html) => Html(html).into_response(),
        Err(error) => {
            tracing::error!("{error:?}");
            return get_500(&error);
        }
    }
}

async fn respond(
    rltbl: &Relatable,
    username: &str,
    select: &Select,
    format: &Format,
) -> Response<Body> {
    let result = match rltbl.fetch(&select).await {
        Ok(result) => result,
        Err(error) => return get_500(&error),
    };

    // format!(
    //     "get_table:\nPath: {path}, {table_name}, {extension:?}, {format}\nQuery Parameters: \
    //      {query_params:?}\nResult Set: {pretty}"
    // );
    let response = match format {
        Format::Html | Format::Default => render_response(&rltbl, &username, &result).await,
        Format::PrettyJson => {
            let site = rltbl.get_site(username).await;
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (
                headers,
                to_string_pretty(&json!({"site": site, "result": result})).unwrap_or_default(),
            )
                .into_response()
        }
        Format::Json => {
            let site = rltbl.get_site(username).await;
            Json(&json!({"site": site, "result": result})).into_response()
        }
    };
    response
}

async fn get_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    Query(query_params): Query<QueryParams>,
    session: Session<SessionNullPool>,
) -> Response<Body> {
    // tracing::info!("get_table({rltbl:?}, {path}, {query_params:?})");
    tracing::info!("get_table([rltbl], {path}, {query_params:?})");
    // tracing::info!("SESSION {:?}", session.get_session_id().inner());
    // tracing::info!("SESSIONS {}", session.count().await);

    let username: String = session.get("username").unwrap_or_default();
    tracing::info!("USERNAME {username}");
    let select = Select::from_path_and_query(&rltbl, &path, &query_params);
    let format = match Format::try_from(&path) {
        Ok(format) => format,
        Err(error) => return get_404(&error),
    };
    respond(&rltbl, &username, &select, &format).await
}

async fn post_table(
    State(rltbl): State<Arc<Relatable>>,
    Path(path): Path<String>,
    _session: Session<SessionNullPool>,
    ExtractJson(changeset): ExtractJson<ChangeSet>,
) -> Response<Body> {
    tracing::info!("post_table([rltbl], {path}, {changeset:?})");

    let table = changeset.table.clone();
    if path != table {
        return get_500(
            &RelatableError::InputError(format!(
                "Changeset table '{table}' does not match URL path {path}"
            ))
            .into(),
        );
    }

    // WARN: We need to check that the user matches!
    // let user = changeset.user.clone();
    // let username: String = session.get("username").unwrap_or_default();
    // if username != user {
    //     return get_500(
    //         &RelatableError::InputError(format!(
    //             "Changeset user '{user}' does not match session username {username}"
    //         ))
    //         .into(),
    //     );
    // }

    // Axum is complaining when we replace the call to block_on() here with
    // .await. Why?
    match block_on(rltbl.set_values(&changeset)) {
        Ok(_) => "POST successful".into_response(),
        Err(error) => get_500(&error),
    }
}

async fn post_sign_in(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Form(form): Form<IndexMap<String, String>>,
) -> Response<Body> {
    tracing::info!("post_login({form:?})");
    let username = String::new();
    let username = form.get("username").unwrap_or(&username);
    session.set("username", username);

    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    query(&rltbl.connection, &statement, Some(&params))
        .await
        .expect("Update user");

    match form.get("redirect") {
        Some(url) => Redirect::to(url).into_response(),
        None => Html(format!(
            r#"<p>Logged in as {username}</p>
            <form method="post">
            <input name="username" value="{username}"/>
            <input type="submit"/>
            </form>"#
        ))
        .into_response(),
    }
}

async fn post_sign_out(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    Form(form): Form<IndexMap<String, String>>,
) -> Response<Body> {
    tracing::debug!("post_login({form:?})");
    let username = String::new();
    let username = form.get("username").unwrap_or(&username);
    session.set("username", username);

    let color = random_color::RandomColor::new().to_hex();
    let statement = format!(r#"INSERT OR IGNORE INTO user("name", "color") VALUES (?, ?)"#);
    let params = json!([username, color]);
    query(&rltbl.connection, &statement, Some(&params))
        .await
        .expect("Update user");

    Html(format!(
        r#"<p>Logged in as {username}</p>
        <form method="post">
        <input name="username" value="{username}"/>
        <input type="submit"/>
        </form>"#
    ))
    .into_response()
}

async fn post_cursor(
    State(rltbl): State<Arc<Relatable>>,
    session: Session<SessionNullPool>,
    ExtractJson(cursor): ExtractJson<Cursor>,
) -> Response<Body> {
    // tracing::info!("post_cursor({cursor:?})");
    let username: String = session.get("username").unwrap_or_default();
    tracing::info!("post_cursor({cursor:?}, {username})");
    // TODO: sanitize the cursor JSON.
    let statement = format!(
        r#"UPDATE user
           SET "cursor" = ?,
               "datetime" = CURRENT_TIMESTAMP
           WHERE "name" = ?"#,
    );
    let cursor = to_value(cursor).unwrap_or_default();
    let params = json!([cursor, username]);
    match query(&rltbl.connection, &statement, Some(&params)).await {
        Ok(_) => "Cursor updated".into_response(),
        Err(_) => "Cursor update failed".into_response(),
    }
}

pub async fn build_app(shared_state: Arc<Relatable>) -> Router {
    let session_config = SessionConfig::default();
    let session_store = SessionStore::<SessionNullPool>::new(None, session_config)
        .await
        .unwrap();
    Router::new()
        .route("/", get(get_root))
        .route("/static/main.js", get(main_js))
        .route("/static/main.css", get(main_css))
        .route("/sign-in", post(post_sign_in))
        .route("/sign-out", post(post_sign_out))
        .route("/cursor", post(post_cursor))
        .route("/table/{*path}", get(get_table).post(post_table))
        .layer(SessionLayer::new(session_store))
        .with_state(shared_state)
}

#[tokio::main]
pub async fn app(rltbl: Relatable, host: &str, port: &u16) -> Result<String> {
    let shared_state = Arc::new(rltbl);

    let app = build_app(shared_state).await;

    // Create a `TcpListener` using tokio.
    let addr = format!("{host}:{port}");
    let listener = TcpListener::bind(&addr).await.expect("valid TCP address");
    println!(
        "Running Relatable server at http://{}",
        listener.local_addr()?
    );
    println!("Press Control-C to quit.");

    // Run the server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    Ok("Stopping Relatable server...".into())
}

pub async fn serve(_cli: &Cli, host: &str, port: &u16) -> Result<()> {
    tracing::debug!("serve({host}, {port})");
    let rltbl = Relatable::default().await?;
    app(rltbl, host, port)?;
    Ok(())
}

// From https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown/src/main.rs
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

// Read CGI variables from the environment,
// and read STDIN in the case of POST,
// then handle the request,
// and send the HTTP response to STDOUT.
pub async fn serve_cgi() -> Result<()> {
    let request_method = std::env::var("REQUEST_METHOD").unwrap_or("GET".to_string());
    let path_info = std::env::var("PATH_INFO").unwrap_or("/".to_string());
    let query_string = std::env::var("QUERY_STRING").unwrap_or_default();
    let query_string = query_string.trim();
    let uri = if query_string == "" {
        path_info
    } else {
        format!("{path_info}?{query_string}")
    };
    let body = if request_method == "POST" {
        std::io::stdin()
            .lines()
            .map(|l| l.unwrap())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };

    let request = axum::http::Request::builder()
        .method(request_method.as_str())
        .uri(uri)
        .body(body)
        .unwrap();
    tracing::debug!("REQUEST {request:?}");

    let rltbl = Relatable::default().await?;
    let shared_state = Arc::new(rltbl);
    let mut router = build_app(shared_state).await;
    let response = router.call(request).await;
    tracing::debug!("RESPONSE {response:?}");

    let result = serialize_response(response.unwrap()).await;
    std::io::stdout()
        .write_all(&result)
        .expect("Write to STDOUT");
    Ok(())
}

// From https://github.com/amandasaurus/rust-cgi/blob/main/src/lib.rs
// Turn a Response into an HTTP response as bytes.
async fn serialize_response(response: Response<Body>) -> Vec<u8> {
    let mut output = String::new();
    output.push_str("Status: ");
    output.push_str(response.status().as_str());
    if let Some(reason) = response.status().canonical_reason() {
        output.push_str(" ");
        output.push_str(reason);
    }
    output.push_str("\n");

    {
        let headers = response.headers();
        let mut keys: Vec<&http::header::HeaderName> = headers.keys().collect();
        keys.sort_by_key(|h| h.as_str());
        for key in keys {
            output.push_str(key.as_str());
            output.push_str(": ");
            output.push_str(headers.get(key).unwrap().to_str().unwrap());
            output.push_str("\n");
        }
    }

    output.push_str("\n");

    let mut output = output.into_bytes();

    let (_, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("Read from response body");
    output.append(&mut bytes.to_vec());

    output
}

#[async_std::main]
async fn main() -> Result<()> {
    // Handle a CGI request, instead of normal CLI input.
    match std::env::var_os("GATEWAY_INTERFACE").and_then(|p| Some(p.into_string())) {
        Some(Ok(s)) if s == "CGI/1.1" => {
            return serve_cgi().await;
        }
        _ => (),
    };

    let cli = Cli::parse();

    // Initialize tracing using --verbose flags
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(cli.verbose.tracing_level())
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    tracing::debug!("CLI {cli:?}");

    match &cli.command {
        Command::Get { subcommand } => match subcommand {
            GetSubcommand::Table {
                table,
                filters,
                format,
                limit,
                offset,
            } => print_table(&cli, table, filters, format, limit, offset).await,
            GetSubcommand::Rows {
                table,
                limit,
                offset,
            } => print_rows(&cli, table, limit, offset).await,
            GetSubcommand::Value { table, row, column } => {
                print_value(&cli, table, *row, column).await
            }
        },
        Command::Set { subcommand } => match subcommand {
            SetSubcommand::Value {
                table,
                row,
                column,
                value,
            } => set_value(&cli, table, *row, column, value).await,
        },
        Command::Serve { host, port } => serve(&cli, host, port).await,
        Command::Demo { force } => build_demo(&cli, force).await,
    }
}
