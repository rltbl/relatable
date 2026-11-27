//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[select](crate::select)).

use crate::{self as rltbl, structure::Structure};
use rltbl::{
    core::{Relatable, RelatableError, DEFAULT_LIMIT},
    site::{Page, Tab},
    sql::{self, SqlParam},
};
use rltbl_db::{IntoValue, JsonValue, Value as DbValue};

use anyhow::{anyhow, Result};
use enquote::unquote;
use indexmap::IndexMap;
use petgraph::{algo::astar, graphmap::UnGraphMap};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fmt::Display,
};

/// Represents a SELECT statement.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Select {
    pub table_name: String,
    pub view_name: String,
    pub select: Vec<SelectField>,
    pub joins: Vec<Join>,
    pub limit: usize,
    pub offset: usize,
    pub filters: Vec<Filter>,
    pub order_by: Vec<(String, Order)>,
}

impl Select {
    /// Construct a Select with the given table_name and return it
    pub fn from(table_name: &str) -> Self {
        tracing::trace!("Select::from({table_name:?})");
        Self {
            table_name: table_name.to_string(),
            limit: DEFAULT_LIMIT,
            ..Default::default()
        }
    }

    /// Construct a [Select] for the given [relatable](crate) instance from the given path and
    /// query parameters. Note that this function may panic!
    pub async fn from_path_and_query(
        path: &str,
        query_params: &QueryParams,
        rltbl: &Relatable,
    ) -> Self {
        let mut query_params = query_params.clone();
        let mut filters = Vec::new();
        let mut order_by = Vec::new();
        let mut select = vec![];
        if let Some(selects) = query_params.get("select") {
            for s in selects.split(",") {
                match s {
                    "count()" => select.push(SelectField::Expression {
                        expression: s.to_string(),
                        alias: String::new(),
                    }),
                    _ => select.push(SelectField::Column {
                        table: String::new(),
                        column: s.to_string(),
                        alias: String::new(),
                    }),
                }
            }
        }

        let limit: usize = query_params
            .get("limit")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or(DEFAULT_LIMIT);
        let offset: usize = query_params
            .get("offset")
            .and_then(|x| x.parse::<usize>().ok())
            .unwrap_or_default();
        if let Some(order) = query_params.get("order") {
            for item in order.split(",") {
                if item.ends_with(".asc") {
                    let column = item.replace(".asc", "");
                    order_by.push((column, Order::ASC));
                } else if item.ends_with(".desc") {
                    let column = item.replace(".desc", "");
                    order_by.push((column, Order::DESC));
                } else {
                    order_by.push((item.to_string(), Order::ASC));
                }
            }
        }

        query_params.shift_remove("limit");
        query_params.shift_remove("offset");
        query_params.shift_remove("order");

        fn value_as_type(sql_type: &str, column: &str, value: &str) -> DbValue {
            tracing::debug!("value_as_type: {sql_type} {column} {value}");
            fn try_parse_as_int(value: &str) -> DbValue {
                match value.parse::<i64>() {
                    Ok(signed) => DbValue::from(signed),
                    _ => {
                        tracing::warn!("Could not parse {value} as integer. Treating as string");
                        DbValue::Text(value.to_string())
                    }
                }
            }

            fn try_parse_as_decimal(value: &str) -> DbValue {
                match value.parse::<f64>() {
                    Ok(signed) => DbValue::from(signed),
                    _ => {
                        tracing::warn!("Could not parse {value} as decimal. Treating as string");
                        DbValue::Text(value.to_string())
                    }
                }
            }

            if ["_id", "_order", "_change_id"].contains(&column) {
                try_parse_as_int(value)
            } else if ["_history", "_message"].contains(&column) {
                DbValue::Text(value.to_string())
            } else {
                match sql_type.to_lowercase().as_str() {
                    "text" | "" => DbValue::Text(value.to_string()),
                    "integer" => try_parse_as_int(value),
                    "decimal" => try_parse_as_decimal(value),
                    other => {
                        tracing::warn!("Unsupported datatype: {other}. Treating {value} as string");
                        DbValue::Text(value.to_string())
                    }
                }
            }
        }

        let schema = rltbl.schema().await.unwrap_or_default();

        let base_table_name = path.split(".").next().unwrap_or_default();
        let base_view_name = schema
            .table(base_table_name)
            .cloned()
            .unwrap_or_default()
            .view;

        for (lhs, pattern) in query_params {
            let (table_name, column_name) = match lhs.split_once(".") {
                Some((table_name, column)) => (table_name.to_string(), column.to_string()),
                None => (String::new(), lhs),
            };
            if pattern.starts_with("like.") {
                let value = &pattern.replace("like.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::Like {
                        table: table_name,
                        column: column_name,
                        value,
                    }),
                    Err(_) => filters.push(Filter::Like {
                        table: table_name,
                        column: column_name,
                        value: DbValue::Text(value.to_string()),
                    }),
                }
            } else {
                let inner_table_name = match table_name.as_str() {
                    "" => base_table_name,
                    _ => &table_name,
                };
                let sql_type = schema
                    .column(inner_table_name, &column_name)
                    .cloned()
                    .unwrap_or_default()
                    .sql_type;
                if pattern.starts_with("eq.") {
                    let value = &pattern.replace("eq.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::Equal {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("not_eq.") {
                    let value = &pattern.replace("not_eq.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::NotEqual {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("gt.") {
                    let value = &pattern.replace("gt.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::GreaterThan {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("gte.") {
                    let value = &pattern.replace("gte.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::GreaterThanOrEqual {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("lt.") {
                    let value = &pattern.replace("lt.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::LessThan {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("lte.") {
                    let value = &pattern.replace("lte.", "");
                    let value = value_as_type(&sql_type, &column_name, value);
                    filters.push(Filter::LessThanOrEqual {
                        table: table_name,
                        column: column_name,
                        value,
                    })
                } else if pattern.starts_with("is.") {
                    let value = pattern.replace("is.", "");
                    if value.to_lowercase() == "null" {
                        filters.push(Filter::Is {
                            table: table_name,
                            column: column_name,
                            value: DbValue::Null,
                        })
                    } else {
                        let value = value_as_type(&sql_type, &column_name, &value);
                        filters.push(Filter::Is {
                            table: table_name,
                            column: column_name,
                            value,
                        })
                    }
                } else if pattern.starts_with("is_not.") {
                    let value = pattern.replace("is_not.", "");
                    if value.to_lowercase() == "null" {
                        filters.push(Filter::IsNot {
                            table: table_name,
                            column: column_name,
                            value: DbValue::Null,
                        })
                    } else {
                        let value = value_as_type(&sql_type, &column_name, &value);
                        filters.push(Filter::IsNot {
                            table: table_name,
                            column: column_name,
                            value,
                        })
                    }
                } else if pattern.starts_with("in.") {
                    let separator = Regex::new(r"\s*,\s*").unwrap();
                    let values = pattern.replace("in.", "");
                    let values = match values.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
                        None => {
                            tracing::warn!("invalid 'in' filter value {pattern}");
                            ""
                        }
                        Some(s) => s,
                    };
                    let values = separator
                        .split(values)
                        .map(|v| value_as_type(&sql_type, &column_name, v))
                        .collect::<Vec<_>>();
                    filters.push(Filter::In {
                        table: table_name,
                        column: column_name,
                        values,
                    })
                } else if pattern.starts_with("not_in.") {
                    let separator = Regex::new(r"\s*,\s*").unwrap();
                    let values = pattern.replace("not_in.", "");
                    let values = match values.strip_prefix("(").and_then(|s| s.strip_suffix(")")) {
                        None => {
                            tracing::warn!("invalid 'not_in' filter value {pattern}");
                            ""
                        }
                        Some(s) => s,
                    };
                    let values = separator
                        .split(values)
                        .map(|v| value_as_type(&sql_type, &column_name, v))
                        .collect::<Vec<_>>();
                    filters.push(Filter::NotIn {
                        table: table_name,
                        column: column_name,
                        values,
                    })
                }
            }
        }

        Self {
            table_name: base_table_name.to_string(),
            view_name: base_view_name,
            select,
            limit,
            offset,
            order_by,
            filters,
            ..Default::default()
        }
    }

    /// Get all the tables that are implicated in this select:
    pub fn get_tables(&self) -> BTreeSet<String> {
        let mut tables = BTreeSet::new();

        fn insert_when_non_empty(tables: &mut BTreeSet<String>, table: &str) {
            if table != "" {
                tables.insert(table.to_string());
            }
        }

        insert_when_non_empty(&mut tables, &self.table_name);
        for field in &self.select {
            match field {
                SelectField::Column { table, .. } => {
                    insert_when_non_empty(&mut tables, table);
                }
                SelectField::Expression { .. } => (),
            };
        }
        for join in &self.joins {
            insert_when_non_empty(&mut tables, &join.left_table);
            insert_when_non_empty(&mut tables, &join.right_table);
        }
        for filter in &self.filters {
            insert_when_non_empty(&mut tables, &filter.get_table());
            match filter {
                Filter::InSubquery { subquery, .. } | Filter::NotInSubquery { subquery, .. } => {
                    for table in subquery.get_tables() {
                        insert_when_non_empty(&mut tables, &table);
                    }
                }
                _ => (),
            };
        }

        tables
    }

    /// Add a single column to the SELECT clause of this select
    pub fn select_column(&mut self, column: &str) -> &Self {
        self.select.push(SelectField::Column {
            table: String::new(),
            column: column.to_string(),
            alias: String::new(),
        });
        self
    }

    /// Add multiple columns to the SELECT clause of this select
    pub fn select_columns(&mut self, columns: &Vec<&str>) -> &Self {
        for column in columns {
            self.select_column(column);
        }
        self
    }

    /// Add a qualified column to the SELECT clause of this select
    pub fn select_table_column(&mut self, table: &str, column: &str) -> &Self {
        self.select.push(SelectField::Column {
            table: table.to_string(),
            column: column.to_string(),
            alias: String::new(),
        });
        self
    }

    /// Add multiple qualified columns to the SELECT clause of this select
    pub fn select_table_columns(&mut self, table: &str, columns: &Vec<&str>) -> &Self {
        for column in columns {
            self.select_table_column(table, column);
        }
        self
    }

    /// Add a column with an alias to the SELECT clause of this select
    pub fn select_alias(&mut self, table: &str, column: &str, alias: &str) -> &Self {
        self.select.push(SelectField::Column {
            table: table.to_string(),
            column: column.to_string(),
            alias: alias.to_string(),
        });
        self
    }

    /// Add an aliased expression to the SELECT clause of this select
    pub fn select_expression(&mut self, expression: &str, alias: &str) -> &Self {
        self.select.push(SelectField::Expression {
            expression: expression.to_string(),
            alias: alias.to_string(),
        });
        self
    }

    /// Add all of the given table's columns to the SELECT clause of this select
    pub async fn select_all(&mut self, rltbl: &Relatable, table_name: &str) -> Result<&Self> {
        for column in rltbl.column_table().await?.get(&[table_name]).await?.iter() {
            self.select.push(SelectField::Column {
                table: String::new(),
                column: column.column.to_string(),
                alias: String::new(),
            });
        }
        Ok(self)
    }

    /// Add a LEFT JOIN clause to this select with the given properties
    pub fn left_join(
        &mut self,
        left_table: &str,
        left_column: &str,
        right_table: &str,
        right_column: &str,
    ) -> &Self {
        self.joins.push(Join {
            join_type: JoinType::LeftJoin,
            left_table: left_table.to_string(),
            left_column: left_column.to_string(),
            right_table: right_table.to_string(),
            right_column: right_column.to_string(),
        });
        self
    }

    /// Order (ascending) this select by the given column
    pub fn order_by(&mut self, column: &str) -> &Self {
        tracing::trace!("Select::order_by({column:?})");
        self.order_by = vec![(column.to_string(), Order::ASC)];
        self
    }

    /// Order (descending) this select by the given column
    pub fn order_by_desc(&mut self, column: &str) -> &Self {
        tracing::trace!("Select::order_by_desc({column:?})");
        self.order_by = vec![(column.to_string(), Order::DESC)];
        self
    }

    /// Limit the results by a given amount
    pub fn limit(mut self, limit: &usize) -> Self {
        tracing::trace!("Select::limit({limit})");
        self.limit = *limit;
        self
    }

    /// Offset the results by a given amount
    pub fn offset(mut self, offset: &usize) -> Self {
        tracing::trace!("Select::offset({offset})");
        self.offset = *offset;
        self
    }

    /// Add the given filters to the select.
    pub fn filters(mut self, filters: &Vec<String>) -> Result<Self> {
        tracing::trace!("Select::filters({filters:?})");
        let basic = r"[\w\-]";
        let wildcarded = r"[\w\-%]";

        // Symbolic operators:
        let like = Regex::new(&format!(r#"^({basic}+)\s*~=\s*"?({wildcarded}+)"?$"#)).unwrap();
        let eq = Regex::new(&format!(r#"^({basic}+)\s*=\s*"?({basic}+)"?$"#)).unwrap();
        let not_eq = Regex::new(&format!(r#"^({basic}+)\s*!=\s*"?({basic}+)"?$"#)).unwrap();
        let gt = Regex::new(&format!(r"^({basic}+)\s*>\s*({basic}+)$")).unwrap();
        let gte = Regex::new(&format!(r"^({basic}+)\s*>=\s*({basic}+)$")).unwrap();
        let lt = Regex::new(&format!(r"^({basic}+)\s*<\s*({basic}+)$")).unwrap();
        let lte = Regex::new(&format!(r"^({basic}+)\s*<=\s*({basic}+)$")).unwrap();

        // Word-like operators:
        let is = Regex::new(&format!(r#"^({basic}+)\s+(IS|is)\s+"?({basic}+)"?$"#)).unwrap();
        let is_not = Regex::new(&format!(
            r#"^({basic}+)\s+(IS NOT|is not)\s+"?({basic}+)"?$"#
        ))
        .unwrap();
        let is_in = Regex::new(&format!(
            r#"^({basic}+)\s+(IN|in)\s+\(({basic}+(,\s*{basic}+)*)\)$"#
        ))
        .unwrap();
        let is_not_in = Regex::new(&format!(
            r#"^({basic}+)\s+(NOT IN|not in)\s+\(({basic}+(,\s*{basic}+)*)\)$"#
        ))
        .unwrap();

        fn parse_as_value(value: &str) -> Result<DbValue> {
            // TODO: Move this to rltbl_db?
            if let Ok(signed) = value.parse::<i64>() {
                Ok(DbValue::from(signed))
            } else if let Ok(float) = value.parse::<f64>() {
                Ok(DbValue::from(float))
            } else if value.starts_with("\"") {
                let value: JsonValue = serde_json::from_str(value)?;
                Ok(DbValue::from(value))
            } else {
                let value: JsonValue = serde_json::from_str(&format!(r#""{value}""#))?;
                Ok(DbValue::from(value))
            }
        }

        for filter in filters {
            tracing::trace!("Applying filter: {filter}");
            if like.is_match(&filter) {
                let captures = like.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::Like {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if eq.is_match(&filter) {
                let captures = eq.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::Equal {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if not_eq.is_match(&filter) {
                let captures = not_eq.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::NotEqual {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if gt.is_match(&filter) {
                let captures = gt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::GreaterThan {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if gte.is_match(&filter) {
                let captures = gte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::GreaterThanOrEqual {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if lt.is_match(&filter) {
                let captures = lt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::LessThan {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if lte.is_match(&filter) {
                let captures = lte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = parse_as_value(value)?;
                self.filters.push(Filter::LessThanOrEqual {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if is.is_match(&filter) {
                let captures = is.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(3).unwrap().as_str();
                let value = match value.to_lowercase().as_str() {
                    "null" => DbValue::Null,
                    _ => parse_as_value(value)?,
                };
                self.filters.push(Filter::Is {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if is_not.is_match(&filter) {
                let captures = is_not.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(3).unwrap().as_str();
                let value = match value.to_lowercase().as_str() {
                    "null" => DbValue::Null,
                    _ => parse_as_value(value)?,
                };
                self.filters.push(Filter::IsNot {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if is_in.is_match(&filter) {
                let captures = is_in.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let values = &captures.get(3).unwrap().as_str();
                let separator = Regex::new(r"\s*,\s*").unwrap();
                let values = separator
                    .split(values)
                    .filter_map(|v| parse_as_value(v).ok())
                    .map(|p| p.into())
                    .collect::<Vec<DbValue>>();
                self.filters.push(Filter::In {
                    table: "".to_string(),
                    column,
                    values,
                });
            } else if is_not_in.is_match(&filter) {
                let captures = is_not_in.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let values = &captures.get(3).unwrap().as_str();
                let separator = Regex::new(r"\s*,\s*").unwrap();
                let values = separator
                    .split(values)
                    .filter_map(|v| parse_as_value(v).ok())
                    .map(|p| p.into())
                    .collect::<Vec<DbValue>>();
                self.filters.push(Filter::NotIn {
                    table: "".to_string(),
                    column,
                    values,
                });
            } else {
                return Err(RelatableError::ConfigError(format!("invalid filter {filter}")).into());
            }
        }
        Ok(self)
    }

    /// Add a like filter for the given column on the given value, which may include '%' wildcards
    pub fn like(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::like({column:?}, value)");
        self.filters.push(Filter::Like {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an equals filter on the given column and value.
    pub fn where_eq(&mut self, column: &str, value: impl IntoValue) -> Result<&Self> {
        tracing::trace!("Select::eq({column:?}, value)");
        self.filters.push(Filter::Equal {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an equals filter on the given column and value.
    pub fn table_eq(&mut self, table: &str, column: &str, value: impl IntoValue) -> Result<&Self> {
        tracing::trace!("Select::table_eq({column:?}, value)");
        self.filters.push(Filter::Equal {
            table: table.to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add a not-equals filter on the given column and value.
    pub fn not_eq(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::not_eq({column:?}, value)");
        self.filters.push(Filter::NotEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an greater-than filter on the given column and value.
    pub fn gt(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::gt({column:?}, value)");
        self.filters.push(Filter::GreaterThan {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an greater-than-or-equals filter on the given column and value.
    pub fn gte(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::gte({column:?}, value)");
        self.filters.push(Filter::GreaterThanOrEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an less-than filter on the given column and value.
    pub fn lt(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::lt({column:?}, value)");
        self.filters.push(Filter::LessThan {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an less-than-or-equals filter on the given column and value.
    pub fn lte(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::lte({column:?}, value)");
        self.filters.push(Filter::LessThanOrEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an is filter on the given column and value.
    pub fn is(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::is({column:?}, value)");
        self.filters.push(Filter::Is {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an is not filter on the given column and value.
    pub fn is_not(mut self, column: &str, value: impl IntoValue) -> Result<Self> {
        tracing::trace!("Select::is_not({column:?}, value)");
        self.filters.push(Filter::IsNot {
            table: "".to_string(),
            column: column.to_string(),
            value: value.into_value(),
        });
        Ok(self)
    }

    /// Add an in filter on the given column and value.
    pub fn is_in(mut self, column: &str, values: &Vec<impl IntoValue + Clone>) -> Result<Self> {
        tracing::trace!("Select::is_in({column:?}, value)");
        self.filters.push(Filter::In {
            table: "".to_string(),
            column: column.to_string(),
            values: values.iter().map(|v| v.clone().into_value()).collect(),
        });
        Ok(self)
    }

    /// Add a not in filter on the given column and value.
    pub fn is_not_in<T>(
        mut self,
        column: &str,
        values: &Vec<impl IntoValue + Clone>,
    ) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::is_not_in({column:?}, value)");
        self.filters.push(Filter::NotIn {
            table: "".to_string(),
            column: column.to_string(),
            values: values.iter().map(|v| v.clone().into_value()).collect(),
        });
        Ok(self)
    }

    /// Add an in-subquery filter on the given column and value.
    pub fn is_in_subquery(&mut self, column: &str, subquery: &Select) -> &Self {
        tracing::trace!("Select::is_in_subquery({column:?}, {subquery:?})");
        let target = match self.view_name.as_str() {
            "" => &self.table_name,
            _ => &self.view_name,
        };
        self.filters.push(Filter::InSubquery {
            table: target.to_string(),
            column: column.to_string(),
            subquery: subquery.clone(),
        });
        self
    }

    /// Add an not-in-subquery filter on the given column and value.
    pub fn is_not_in_subquery(&mut self, column: &str, subquery: &Select) -> &Self {
        tracing::trace!("Select::is_not_in_subquery({column:?}, {subquery:?})");
        let target = match self.view_name.as_str() {
            "" => &self.table_name,
            _ => &self.view_name,
        };
        self.filters.push(Filter::NotInSubquery {
            table: target.to_string(),
            column: column.to_string(),
            subquery: subquery.clone(),
        });
        self
    }

    /// Convert the filter to a tuple consisting of an SQL string supported by the given database
    /// kind, and a vector of parameters that must be bound to the string before executing it.
    pub fn to_sql(&self, kind: &str) -> Result<(String, Vec<DbValue>)> {
        tracing::trace!("Select::to_sql({self:?}, {kind:?})");
        let mut sql_param_gen = SqlParam::new(kind);
        let mut lines = Vec::new();
        let mut params = Vec::new();
        let target = match self.view_name.as_str() {
            "" => &self.table_name,
            _ => &self.view_name,
        };

        let get_change_sql = |sql_param_gen: &mut SqlParam| -> String {
            format!(
                r#"(SELECT MAX(change_id) FROM history
                    WHERE "table" = {}
                      AND "row" = "{}"._id
                   )"#,
                sql_param_gen.next(),
                target
            )
        };

        // The SELECT clause:
        if self.select.len() == 0 {
            if self.joins.len() > 0 {
                lines.push(format!(r#"SELECT "{target}".*,"#));
            } else {
                lines.push("SELECT *".to_string());
            }
            for filter in &self.filters {
                let c = filter.get_column();
                if c == "_change_id" {
                    lines.push(format!(
                        ", {} AS _change_id",
                        get_change_sql(&mut sql_param_gen)
                    ));
                    params.push(self.table_name.clone().into());
                }
            }
        } else {
            lines.push("SELECT".to_string());
            for filter in &self.filters {
                let c = filter.get_column();
                if c == "_change_id" {
                    lines.push(format!(
                        "{} AS _change_id",
                        get_change_sql(&mut sql_param_gen)
                    ));
                    params.push(self.table_name.clone().into());
                }
            }
            for field in &self.select {
                if field.to_sql() == "" {
                    return Err(RelatableError::InputError("Empty field name".to_string()).into());
                }
                let mut t = ",";
                if field == self.select.last().unwrap() {
                    t = "";
                }

                lines.push(format!(r#"  {field}{t}"#, field = field.to_sql()));
            }
        }

        // The FROM clause:
        lines.push(format!(r#"FROM "{target}""#));
        for join in &self.joins {
            lines.push(join.to_sql());
        }

        // The WHERE clause:
        for (i, filter) in self.filters.iter().enumerate() {
            let keyword = if i == 0 { "WHERE" } else { "  AND" };
            let mut filter = filter.clone();
            let t = filter.get_table();
            let c = filter.get_column();
            if self.view_name != "" && t == self.table_name {
                filter.set_table(&self.view_name);
            }

            let (filter_sql, mut filter_params) = filter.to_sql(&mut sql_param_gen)?;
            // If the select is using the text view, the query parameters must all be changed
            // to text:
            if self.view_name == format!("{}_text", self.table_name) {
                filter_params = filter_params
                    .iter()
                    .map(|param| match param {
                        DbValue::Text(_) => param.clone(),
                        _ => DbValue::Text(param.clone().into()),
                    })
                    .collect::<Vec<_>>();
            }
            params.append(&mut filter_params);

            if c == "_change_id" {
                lines.push(format!(
                    "{keyword} {} {}",
                    get_change_sql(&mut sql_param_gen),
                    filter_sql.replace(&format!(r#""{c}" "#), "")
                ));
                params.push(self.table_name.clone().into());
            } else {
                lines.push(format!("{keyword} {filter_sql}"));
            }
        }
        if self.order_by.len() == 0 && self.joins.len() == 0 {
            lines.push(format!(r#"ORDER BY "{target}"._order ASC"#));
        }
        for (column, order) in &self.order_by {
            lines.push(format!(r#"ORDER BY "{column}" {order:?}"#));
        }
        if self.limit > 0 {
            lines.push(format!("LIMIT {}", self.limit));
        }
        if self.offset > 0 {
            lines.push(format!("OFFSET {}", self.offset));
        }

        // Return the generated SQL and parameter values:
        Ok((lines.join("\n"), params))
    }

    /// Generate a SQL statement consisting of a SELECT COUNT(*) over the data that will be returned
    /// by the given [Select]
    pub fn to_sql_count(&self, kind: &str) -> Result<(String, Vec<DbValue>)> {
        tracing::trace!("Select::to_sql_count({self:?}, {kind:?})");
        let mut sql_param_gen = SqlParam::new(kind);
        let target = match self.view_name.as_str() {
            "" => &self.table_name,
            _ => &self.view_name,
        };
        let get_change_sql = |sql_param_gen: &mut SqlParam| -> String {
            format!(
                r#"(SELECT MAX(change_id) FROM history
                    WHERE "table" = {}
                      AND "row" = "{}"._id
                   )"#,
                sql_param_gen.next(),
                target
            )
        };

        let mut lines = Vec::new();
        let mut params = Vec::new();
        lines.push(r#"SELECT COUNT(1) AS "count""#.to_string());
        lines.push(format!(r#"FROM "{target}""#));
        for join in self.joins.clone() {
            lines.push(join.to_sql());
        }
        for (i, filter) in self.filters.iter().enumerate() {
            let keyword = if i == 0 { "WHERE" } else { "  AND" };
            let mut filter = filter.clone();
            let t = filter.get_table();
            let c = filter.get_column();
            if self.view_name != "" && t == self.table_name {
                filter.set_table(&self.view_name);
            }
            let (filter_sql, filter_params) = filter.to_sql_count(&mut sql_param_gen)?;
            params.append(&mut filter_params.clone());

            if c == "_change_id" {
                lines.push(format!(
                    "{keyword} {} {}",
                    get_change_sql(&mut sql_param_gen),
                    filter_sql.replace(&format!(r#""{c}" "#), "")
                ));
                params.push(self.table_name.clone().into());
            } else {
                lines.push(format!("{keyword} {filter_sql}"));
            }
        }

        // If the select is using the text view, the query parameters must all be changed
        // to text:
        if self.view_name == format!("{}_text", self.table_name) {
            params = params
                .iter()
                .map(|param| match param {
                    DbValue::Text(_) => param.clone(),
                    _ => DbValue::Text(param.clone().into()),
                })
                .collect::<Vec<_>>();
        }

        Ok((lines.join("\n"), params))
    }

    /// Converts this select's filters to a map from column names to URL representations of their
    /// associated filters represented as [JsonValue]s
    pub fn to_params(&self) -> Result<IndexMap<String, DbValue>> {
        tracing::trace!("Select::to_params()");
        if self.table_name.is_empty() {
            return Err(RelatableError::InputError(
                "Missing required field: `table` in to_sql()".to_string(),
            )
            .into());
        }

        let mut params = IndexMap::new();
        if self.select.len() > 0 {
            let mut select_cols = vec![];
            for sfield in self.select.iter() {
                match sfield {
                    SelectField::Column { .. } => {
                        select_cols.push(sfield.to_url());
                    }
                    SelectField::Expression { expression, .. } => {
                        // Only include 'count()' expressions
                        if expression == "count()" {
                            select_cols.push(expression.to_string());
                        }
                    }
                };
            }
            if select_cols.len() > 0 {
                params.insert("select".to_string(), select_cols.join(",").into());
            }
        }
        if self.filters.len() > 0 {
            for filter in &self.filters {
                let table = filter.get_table();
                let column = filter.get_column();

                if table != "" {
                    if let Err(e) = sql::is_simple(&table) {
                        return Err(RelatableError::InputError(format!(
                            "While reading filters table name, got error: {}",
                            e
                        ))
                        .into());
                    }
                }
                if let Err(e) = sql::is_simple(&column) {
                    return Err(RelatableError::InputError(format!(
                        "While reading filters column name, got error: {}",
                        e
                    ))
                    .into());
                }
                let lhs = {
                    match table.as_str() {
                        "" => format!(r#"{column}"#),
                        _ => format!(r#"{table}.{column}"#),
                    }
                };
                params.insert(lhs, filter.to_param()?.into());
            }
        }
        if self.limit > 0 && self.limit != DEFAULT_LIMIT {
            params.insert("limit".into(), self.limit.into());
        }
        if self.offset > 0 {
            params.insert("offset".into(), self.offset.into());
        }
        Ok(params)
    }

    /// Convert the select to a URL
    pub fn to_url(&self, base: &str, format: &Format) -> Result<String> {
        tracing::trace!("Select::to_url({base:?}, format)");
        let table_name = self.table_name.to_string();
        if let Err(e) = sql::is_simple(&table_name) {
            return Err(RelatableError::InputError(format!(
                "While reading table name, got error: {}",
                e
            ))
            .into());
        }
        let path = format!("{base}/{table_name}{format}");

        if self.joins.len() > 0 {
            return Err(RelatableError::InputError(
                "Joins are unsupported in to_url()".to_string(),
            )
            .into());
        }

        let params = &self.to_params()?.clone();
        if params.len() > 0 {
            let mut parts = vec![];
            for (column, value) in params.iter() {
                let string: String = value.into();
                parts.push(format!("{column}={string}"));
            }
            Ok(format!("{}?{}", path, parts.join("&")))
        } else {
            Ok(path.to_string())
        }
    }

    pub fn to_page(
        &self,
        root: &str,
        path: &str,
        tableset: &str,
        group_name: &str,
        tab_groups: &[String],
        tabs: &[String],
    ) -> Result<Page> {
        tracing::trace!("Select::to_page({root}, {path})");
        let base = format!("{root}/{path}");
        let mut formats = IndexMap::new();
        formats.insert("HTML".to_string(), self.to_url(&base, &Format::Html)?);
        formats.insert("CSV".to_string(), self.to_url(&base, &Format::Csv)?);
        formats.insert("TSV".to_string(), self.to_url(&base, &Format::Tsv)?);
        formats.insert("JSON".to_string(), self.to_url(&base, &Format::Json)?);
        formats.insert(
            "JSON (Pretty)".to_string(),
            self.to_url(&base, &Format::PrettyJson)?,
        );

        let tab_groups: Vec<Tab> = tab_groups
            .iter()
            .map(|t| {
                let mut s = self.clone();
                s.table_name = t.clone();
                let mut c = s.clone();
                c.select_expression("count()", "count");
                Tab {
                    table: t.clone(),
                    active: (t.as_str() == group_name),
                    url: s.to_url(&base, &Format::Default).unwrap(),
                    count: c.to_url(&base, &Format::ValueJson).unwrap(),
                }
            })
            .collect();

        let tabs: Vec<Tab> = tabs
            .iter()
            .map(|t| {
                let mut s = self.clone();
                s.table_name = t.clone();
                let mut c = s.clone();
                c.select_expression("count()", "count");
                Tab {
                    table: t.clone(),
                    active: (t.as_str() == self.table_name),
                    url: s.to_url(&base, &Format::Default).unwrap(),
                    count: c.to_url(&base, &Format::ValueJson).unwrap(),
                }
            })
            .collect();

        Ok(Page {
            path: path.to_string(),
            formats,
            tableset: tableset.to_string(),
            tab_groups,
            tabs,
        })
    }
}

/// A field in a [Select] clause.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SelectField {
    Column {
        table: String,
        column: String,
        alias: String,
    },
    Expression {
        expression: String,
        alias: String,
    },
}

impl SelectField {
    fn to_sql(&self) -> String {
        match self {
            SelectField::Column {
                table,
                column,
                alias,
            } => {
                format!(
                    "{table}{column}{alias}",
                    table = match table.as_str() {
                        "" => "".to_string(),
                        _ => format!(r#""{table}"."#),
                    },
                    column = format!(r#""{column}""#),
                    alias = match alias.as_str() {
                        "" => "".to_string(),
                        _ => format!(r#" AS "{alias}""#),
                    }
                )
            }
            SelectField::Expression { expression, alias } => {
                format!(
                    "{expression}{alias}",
                    alias = match alias.as_str() {
                        "" => "".to_string(),
                        _ => format!(r#" AS "{alias}""#),
                    }
                )
            }
        }
    }

    fn to_url(&self) -> String {
        match self {
            SelectField::Column {
                table,
                column,
                alias,
            } => {
                if alias != "" {
                    tracing::warn!("Alias '{alias}' unsupported in to_url()");
                }
                format!(
                    "{table}{column}",
                    table = match table.as_str() {
                        "" => "".to_string(),
                        _ => format!("{table}."),
                    },
                    column = format!("{column}")
                )
            }
            _ => panic!("Select Expressions are not supported"),
        }
    }
}

/// Represents a database join
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum JoinType {
    #[default]
    Join,
    LeftJoin,
}

impl Display for JoinType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JoinType::Join => write!(f, "JOIN"),
            JoinType::LeftJoin => write!(f, "LEFT JOIN"),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Join {
    join_type: JoinType,
    left_table: String,
    left_column: String,
    right_table: String,
    right_column: String,
}

impl Join {
    pub fn to_sql(&self) -> String {
        format!(
            r#"{op} "{rt}" ON "{lt}"."{lc}" = "{rt}"."{rc}""#,
            op = self.join_type,
            lt = self.left_table,
            lc = self.left_column,
            rt = self.right_table,
            rc = self.right_column,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum Filter {
    Like {
        table: String,
        column: String,
        value: DbValue,
    },
    Equal {
        table: String,
        column: String,
        value: DbValue,
    },
    NotEqual {
        table: String,
        column: String,
        value: DbValue,
    },
    GreaterThan {
        table: String,
        column: String,
        value: DbValue,
    },
    GreaterThanOrEqual {
        table: String,
        column: String,
        value: DbValue,
    },
    LessThan {
        table: String,
        column: String,
        value: DbValue,
    },
    LessThanOrEqual {
        table: String,
        column: String,
        value: DbValue,
    },
    Is {
        table: String,
        column: String,
        value: DbValue,
    },
    IsNot {
        table: String,
        column: String,
        value: DbValue,
    },
    In {
        table: String,
        column: String,
        values: Vec<DbValue>,
    },
    NotIn {
        table: String,
        column: String,
        values: Vec<DbValue>,
    },
    InSubquery {
        table: String,
        column: String,
        subquery: Select,
    },
    NotInSubquery {
        table: String,
        column: String,
        subquery: Select,
    },
}
impl Filter {
    pub fn set_table(&mut self, new_name: &str) -> &Self {
        match self {
            Filter::Like { table, .. }
            | Filter::Equal { table, .. }
            | Filter::NotEqual { table, .. }
            | Filter::GreaterThan { table, .. }
            | Filter::GreaterThanOrEqual { table, .. }
            | Filter::LessThan { table, .. }
            | Filter::LessThanOrEqual { table, .. }
            | Filter::Is { table, .. }
            | Filter::IsNot { table, .. }
            | Filter::In { table, .. }
            | Filter::NotIn { table, .. }
            | Filter::InSubquery { table, .. }
            | Filter::NotInSubquery { table, .. } => *table = new_name.to_string(),
        };
        self
    }

    pub fn set_column(&mut self, new_name: &str) -> &Self {
        match self {
            Filter::Like { column, .. }
            | Filter::Equal { column, .. }
            | Filter::NotEqual { column, .. }
            | Filter::GreaterThan { column, .. }
            | Filter::GreaterThanOrEqual { column, .. }
            | Filter::LessThan { column, .. }
            | Filter::LessThanOrEqual { column, .. }
            | Filter::Is { column, .. }
            | Filter::IsNot { column, .. }
            | Filter::In { column, .. }
            | Filter::NotIn { column, .. }
            | Filter::InSubquery { column, .. }
            | Filter::NotInSubquery { column, .. } => *column = new_name.to_string(),
        };
        self
    }

    pub fn get_table(&self) -> String {
        let table = match self {
            Filter::Like { table, .. } => table,
            Filter::Equal { table, .. } => table,
            Filter::NotEqual { table, .. } => table,
            Filter::GreaterThan { table, .. } => table,
            Filter::GreaterThanOrEqual { table, .. } => table,
            Filter::LessThan { table, .. } => table,
            Filter::LessThanOrEqual { table, .. } => table,
            Filter::Is { table, .. } => table,
            Filter::IsNot { table, .. } => table,
            Filter::In { table, .. } => table,
            Filter::NotIn { table, .. } => table,
            Filter::InSubquery { table, .. } => table,
            Filter::NotInSubquery { table, .. } => table,
        };
        table.to_string()
    }

    pub fn get_column(&self) -> String {
        let column = match self {
            Filter::Like { column, .. } => column,
            Filter::Equal { column, .. } => column,
            Filter::NotEqual { column, .. } => column,
            Filter::GreaterThan { column, .. } => column,
            Filter::GreaterThanOrEqual { column, .. } => column,
            Filter::LessThan { column, .. } => column,
            Filter::LessThanOrEqual { column, .. } => column,
            Filter::Is { column, .. } => column,
            Filter::IsNot { column, .. } => column,
            Filter::In { column, .. } => column,
            Filter::NotIn { column, .. } => column,
            Filter::InSubquery { column, .. } => column,
            Filter::NotInSubquery { column, .. } => column,
        };
        column.to_string()
    }

    pub fn get_operator(&self) -> String {
        let operator = match self {
            Filter::Like { .. } => "like",
            Filter::Equal { .. } => "eq",
            Filter::NotEqual { .. } => "not_eq",
            Filter::GreaterThan { .. } => "gt",
            Filter::GreaterThanOrEqual { .. } => "gte",
            Filter::LessThan { .. } => "lt",
            Filter::LessThanOrEqual { .. } => "lte",
            Filter::Is { .. } => "is",
            Filter::IsNot { .. } => "is_not",
            Filter::In { .. } => "in",
            Filter::NotIn { .. } => "not_in",
            Filter::InSubquery { .. } => "in",
            Filter::NotInSubquery { .. } => "not_in",
        };
        operator.to_string()
    }

    pub fn get_values(&self) -> Vec<DbValue> {
        match self {
            Filter::Like { value, .. } => vec![value.clone()],
            Filter::Equal { value, .. } => vec![value.clone()],
            Filter::NotEqual { value, .. } => vec![value.clone()],
            Filter::GreaterThan { value, .. } => vec![value.clone()],
            Filter::GreaterThanOrEqual { value, .. } => vec![value.clone()],
            Filter::LessThan { value, .. } => vec![value.clone()],
            Filter::LessThanOrEqual { value, .. } => vec![value.clone()],
            Filter::Is { value, .. } => vec![value.clone()],
            Filter::IsNot { value, .. } => vec![value.clone()],
            Filter::In { values, .. } => values.clone(),
            Filter::NotIn { values, .. } => values.clone(),
            Filter::InSubquery { .. } => vec![],
            Filter::NotInSubquery { .. } => vec![],
        }
    }

    pub fn get_value(&self) -> Result<String> {
        fn handle_value(value: &DbValue) -> String {
            match value {
                DbValue::Null => "null".to_string(),
                DbValue::Text(token) => {
                    let reserved = vec![':', ',', '.', '(', ')'];
                    if token.chars().all(char::is_numeric)
                        || reserved.iter().any(|&c| token.contains(c))
                    {
                        if token.contains(char::is_whitespace) {
                            format!("\"{}\"", token)
                        } else {
                            token.to_string()
                        }
                    } else {
                        token.to_string()
                    }
                }
                _ => value.into(),
            }
        }

        match self {
            Filter::InSubquery { .. } | Filter::NotInSubquery { .. } => Err(
                RelatableError::DataError(format!("Subquery filters are unsupported: {self:?}"))
                    .into(),
            ),
            Filter::In { .. } | Filter::NotIn { .. } => Ok(self
                .get_values()
                .iter()
                .map(|v| handle_value(v))
                .collect::<Vec<String>>()
                .join(",")),
            _ => match self.get_values().first() {
                Some(value) => Ok(handle_value(value)),
                None => {
                    Err(RelatableError::DataError(format!("No value for filter: {self:?}")).into())
                }
            },
        }
    }

    pub fn to_param(&self) -> Result<String> {
        match self {
            Filter::In { .. } | Filter::NotIn { .. } => {
                Ok(format!("{}.({})", self.get_operator(), self.get_value()?))
            }
            _ => Ok(format!("{}.{}", self.get_operator(), self.get_value()?)),
        }
    }

    pub fn to_sql(&self, sql_param: &mut SqlParam) -> Result<(String, Vec<DbValue>)> {
        tracing::trace!("Filter::to_sql({sql_param:?})");

        fn generate_lhs(table: &str, column: &str) -> String {
            match table {
                "" => format!(r#""{column}""#),
                _ => format!(r#""{table}"."{column}""#),
            }
        }

        match self {
            Filter::Like {
                table,
                column,
                value,
            } => {
                let value: String = value.into();
                let value = value.replace("*", "%");
                Ok((
                    format!(
                        r#"{lhs} LIKE {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![value.into()],
                ))
            }
            Filter::Equal {
                table,
                column,
                value,
            } => {
                if value.is_bool() {
                    let value = match value.as_bool().unwrap() {
                        true => "TRUE",
                        false => "FALSE",
                    };
                    Ok((
                        format!(r#"{lhs} = {value}"#, lhs = generate_lhs(table, column)),
                        vec![],
                    ))
                } else {
                    Ok((
                        format!(
                            r#"{lhs} = {sql_param}"#,
                            lhs = generate_lhs(table, column),
                            sql_param = sql_param.next()
                        ),
                        vec![value.clone()],
                    ))
                }
            }
            Filter::NotEqual {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} <> {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::GreaterThan {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} > {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::GreaterThanOrEqual {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} >= {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::LessThan {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} < {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::LessThanOrEqual {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} <= {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::Is {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} {is} {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    is = sql::is_clause(&sql_param.kind),
                    sql_param = sql_param.next()
                ),
                vec![value.clone()],
            )),
            Filter::IsNot {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} {is_not} {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next(),
                    is_not = sql::is_not_clause(&sql_param.kind)
                ),
                vec![value.clone()],
            )),
            Filter::In {
                table,
                column,
                values,
            } => {
                let lhs = generate_lhs(table, column);
                Ok((
                    format!("{lhs} IN ({})", sql_param.get_as_list(values.len())),
                    values.clone(),
                ))
            }
            Filter::NotIn {
                table,
                column,
                values,
            } => {
                let lhs = generate_lhs(table, column);
                Ok((
                    format!("{lhs} NOT IN ({})", sql_param.get_as_list(values.len())),
                    values.clone(),
                ))
            }
            Filter::InSubquery {
                table,
                column,
                subquery,
            } => {
                let (sql, params) = subquery.to_sql(&sql_param.kind)?;
                let sql = sql.replace("\n", "\n  ");
                let lhs = generate_lhs(table, column);
                Ok((format!("{lhs} IN (\n  {sql}\n)"), params))
            }
            Filter::NotInSubquery {
                table,
                column,
                subquery,
            } => {
                let (sql, params) = subquery.to_sql(&sql_param.kind)?;
                let sql = sql.replace("\n", "\n  ");
                let lhs = generate_lhs(table, column);
                Ok((format!("{lhs} NOT IN (\n  {sql}\n)"), params))
            }
        }
    }

    /// Generate a SQL statement consisting of a SELECT COUNT(*) over the data that will bereturned
    /// by the given [Select]
    pub fn to_sql_count(&self, sql_param: &mut SqlParam) -> Result<(String, Vec<DbValue>)> {
        tracing::trace!("Filter::to_sql_count({self:?}, {sql_param:?})");
        match self {
            Filter::InSubquery {
                table,
                column,
                subquery,
            } => {
                if column == "" {
                    return Err(RelatableError::InputError("Empty column name".to_string()).into());
                }
                let lhs = match table.as_str() {
                    "" => format!(r#""{column}""#),
                    _ => format!(r#""{table}"."{column}""#),
                };
                let (sql, params) = subquery.to_sql(&sql_param.kind)?;
                let lines: Vec<&str> = sql
                    .split("\n")
                    .filter(|x| !x.starts_with("ORDER BY"))
                    .filter(|x| !x.starts_with("LIMIT"))
                    .filter(|x| !x.starts_with("OFFSET"))
                    .collect();
                let sql = lines.join("\n  ");
                Ok((format!("{lhs} IN (\n  {sql}\n)"), params))
            }
            _ => self.to_sql(sql_param),
        }
    }
}

/// Represents an ORDER BY clause in a SELECT statement.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Order {
    #[default]
    ASC,
    DESC,
}

pub type QueryParams = IndexMap<String, String>;

pub enum Format {
    Html,
    Csv,
    Tsv,
    Json,
    ValueJson,
    PrettyJson,
    Default,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: This should be factored out.
        let result = match self {
            Format::Html => ".html",
            Format::Csv => ".csv",
            Format::Tsv => ".tsv",
            Format::Json => ".json",
            Format::ValueJson => ".value.json",
            Format::PrettyJson => ".pretty.json",
            Format::Default => "",
        };
        write!(f, "{result}")
    }
}

impl TryFrom<&String> for Format {
    fn try_from(path: &String) -> Result<Self> {
        tracing::trace!("Format::try_from({path:?})");
        let path = path.to_lowercase();
        let format = if path.ends_with(".pretty.json") {
            Format::PrettyJson
        } else if path.ends_with(".value.json") {
            Format::ValueJson
        } else if path.ends_with(".json") {
            Format::Json
        } else if path.ends_with(".csv") {
            Format::Csv
        } else if path.ends_with(".tsv") {
            Format::Tsv
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

pub fn render_values(
    items: &Vec<DbValue>,
    sql_param_gen: &mut SqlParam,
) -> Result<(String, Vec<DbValue>)> {
    let mut sql_params = vec![];
    let mut values = vec![];
    for item in items.iter() {
        sql_params.push(sql_param_gen.next());
        let value = match item {
            DbValue::Null => "NULL".to_string(),
            DbValue::Boolean(bool) => bool.to_string(),
            DbValue::SmallInteger(number) => number.to_string(),
            DbValue::Integer(number) => number.to_string(),
            DbValue::BigInteger(number) => number.to_string(),
            DbValue::Real(number) => number.to_string(),
            DbValue::BigReal(number) => number.to_string(),
            DbValue::Numeric(decimal) => decimal.to_string(),
            DbValue::Text(text) => {
                let value = unquote(text).unwrap_or(text.clone());
                format!("{value}")
            }
            _ => todo!(),
        };
        values.push(value.into());
    }
    Ok((format!("({})", sql_params.join(", ")), values))
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Edge {
    left_table: String,
    left_column: String,
    right_table: String,
    right_column: String,
}

#[derive(Clone, Debug, Default)]
pub struct Edges {
    edges: HashSet<Edge>,
}

impl Edges {
    pub fn insert(
        &mut self,
        left_table: &str,
        left_column: &str,
        right_table: &str,
        right_column: &str,
    ) -> &mut Self {
        let edge = Edge {
            left_table: left_table.to_string(),
            left_column: left_column.to_string(),
            right_table: right_table.to_string(),
            right_column: right_column.to_string(),
        };
        self.edges.insert(edge);
        self
    }

    pub fn graph(&self) -> UnGraphMap<&str, ()> {
        UnGraphMap::<&str, ()>::from_edges(
            self.edges
                .iter()
                .map(|e| (e.left_table.as_str(), e.right_table.as_str())),
        )
    }

    pub fn join(&self, first_table: &str, second_table: &str) -> Option<Join> {
        for edge in &self.edges {
            if edge.left_table == first_table && edge.right_table == second_table {
                return Some(Join {
                    join_type: JoinType::Join,
                    left_table: edge.left_table.clone(),
                    left_column: edge.left_column.clone(),
                    right_table: edge.right_table.clone(),
                    right_column: edge.right_column.clone(),
                });
            } else if edge.left_table == second_table && edge.right_table == first_table {
                return Some(Join {
                    join_type: JoinType::Join,
                    left_table: edge.right_table.clone(),
                    left_column: edge.right_column.clone(),
                    right_table: edge.left_table.clone(),
                    right_column: edge.left_column.clone(),
                });
            }
        }
        None
    }
}

pub async fn joined_query(
    rltbl: &Relatable,
    tableset_name: &str,
    select: &Select,
) -> Result<Select> {
    tracing::info!("joined_query({tableset_name}, {select:?})");

    // If there's no tableset, return an error.
    let sql = r#"SELECT * FROM "tableset" WHERE tableset = $1"#;
    let db_rows = match rltbl.pool.query(&sql, [tableset_name]).await {
        Ok(rows) => rows,
        Err(error) => return Err(error.into()),
    };
    if db_rows.is_empty() {
        return Err(anyhow!("No entries for tableset '{tableset_name}'"));
    }

    // If there are no filters, there's nothing to join.
    if select.filters.is_empty() {
        return Ok(select.clone());
    }

    // If there are no other tables filtered, there's nothing to join.
    let filter_tables: HashSet<String> = select
        .filters
        .iter()
        .map(|f| f.get_table())
        .filter(|f| !f.is_empty() && f != &select.table_name)
        .collect();

    // If filter_tables is empty, there's nothing to join,
    // except maybe a view to its table.
    if filter_tables.is_empty() {
        if !select.view_name.is_empty() && select.table_name != select.view_name {
            // lines.push(format!("JOIN {} USING (_id)", select.table_name));
            let mut output = select.clone();
            output.joins = vec![Join {
                join_type: JoinType::Join,
                left_table: select.view_name.clone(),
                left_column: "_id".to_string(),
                right_table: select.table_name.clone(),
                right_column: "_id".to_string(),
            }];
            return Ok(output);
        } else {
            return Ok(select.clone());
        }
    }

    // Build an undirected graph using the "from" relations between tables.
    let schema = rltbl.schema().await?;
    let mut edges = Edges::default();
    for column in schema.columns.iter() {
        for structure in column.structure.iter() {
            match structure {
                Structure::From(from_table, from_column) => {
                    let from_table = from_table.clone().unwrap_or(column.table.clone());
                    edges.insert(&from_table, &from_column, &column.table, &column.column);
                }
                _ => (),
            }
        }
    }
    edges.insert("column", "table", "message", "table");
    edges.insert("column", "column", "message", "column");
    let graph = edges.graph();

    // Find a path from the display table to every filter table.
    let display_table = &select.table_name;

    let mut paths = Vec::new();
    for filter_table in &filter_tables {
        let node_map = astar(&graph, filter_table, |f| f == display_table, |_| 1, |_| 0);
        match node_map {
            Some(node_map) => paths.push(node_map.1),
            None => {
                return Err(anyhow!(
                    "No path from '{display_table}' to '{filter_table}'"
                ))
            }
        }
    }
    paths.sort();

    let mut joined: HashSet<String> = HashSet::new();
    let mut joins: Vec<Join> = Vec::new();

    // Maybe we join a view to its table.
    if !select.view_name.is_empty() && select.table_name != select.view_name {
        joins.push(Join {
            join_type: JoinType::Join,
            left_table: select.view_name.clone(),
            left_column: "_id".to_string(),
            right_table: select.table_name.clone(),
            right_column: "_id".to_string(),
        });
    };

    // For each path from display to filter,
    // try to add to the list of joins.
    for path in paths {
        let mut path = path;
        path.reverse();
        let mut prev = path.remove(0);
        for next in path {
            let join = edges.join(prev, next);
            if let Some(join) = join {
                if !joined.contains(&next.to_string()) {
                    joined.insert(next.to_string());
                    joins.push(join);
                }
            } else {
                return Err(anyhow!("failed to join '{prev}' to '{next}'"));
            }
            prev = next;
        }
    }

    let pkey = match display_table.as_str() {
        "message" => "message_id",
        _ => "_id",
    }
    .to_string();
    let mut inner = select.clone();
    inner.view_name = String::new();
    inner.select = vec![SelectField::Column {
        table: select.table_name.clone(),
        column: pkey.clone(),
        alias: String::new(),
    }];
    inner.joins = joins;
    let output = Select {
        table_name: select.table_name.clone(),
        view_name: select.view_name.clone(),
        select: select.select.clone(),
        filters: vec![Filter::InSubquery {
            table: select.table_name.clone(),
            column: pkey.clone(),
            subquery: inner,
        }],
        order_by: select.order_by.clone(),
        ..Default::default()
    };

    Ok(output)
}

// Tests

#[cfg(test)]
mod tests {
    use crate::{
        core::RelatableBuilder,
        sql::{is_clause, is_not_clause},
    };

    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use super::*;

    #[tokio::test]
    async fn test_select_from_path_and_query() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_select_from_path_and_query")
            .init(true)
            .await?;
        crate::demo::build_demo(&rltbl, &true, 0).await.unwrap();
        let sql_param = SqlParam::new(&rltbl.pool.syntax().name()).next();
        let base = "http://example.com";
        let empty: Vec<DbValue> = vec![];

        // A basic URL
        let url = "http://example.com/penguin";
        let query_params = from_value(json!({})).unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT *
FROM "penguin"
ORDER BY "penguin"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin""#
        );
        assert_eq!(params, empty);

        // A URL with a filter on an integer column:
        let url = "http://example.com/penguin.json?sample_number=eq.5&limit=1&offset=2";
        let query_params = from_value(json!({
           "sample_number": "eq.5",
           "limit": "1",
           "offset": "2",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Json).unwrap());
        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "sample_number" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1
OFFSET 2"#
            )
        );
        assert_eq!(params, vec![5i64.into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" = {sql_param}"#
            )
        );
        assert_eq!(params, vec![5i64.into()]);

        // A URL with a filter on a string column and a value with a space
        let url = "http://example.com/penguin?penguin.study_name=eq.FAKE 123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.FAKE 123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["FAKE 123".into()]);

        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec!["FAKE 123".into()]);

        // A URL with an IS NULL filter
        let url = "http://example.com/penguin?penguin.study_name=is.null&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is.null",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_clause = is_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec![DbValue::Null]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}"#,
                is_clause = is_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec![DbValue::Null]);

        // A URL with an IS NOT NULL filter
        let url = "http://example.com/penguin?penguin.study_name=is_not.null&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is_not.null",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_not_clause = is_not_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec![DbValue::Null]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}"#,
                is_not_clause = is_not_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec![DbValue::Null]);

        // A URL with an IN filter
        let mut sql_param_gen = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.sample_number=in.(123,456)&limit=1";
        let query_params = from_value(json!({
           "penguin.sample_number": "in.(123,456)",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."sample_number" IN ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec![123i64.into(), 456i64.into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![123i64.into(), 456i64.into()]);

        // A URL with a NOT IN filter
        let mut sql_param_gen = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.sample_number=not_in.(123,456)&limit=1";
        let query_params = from_value(json!({
           "penguin.sample_number": "not_in.(123,456)",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."sample_number" NOT IN ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec![123i64.into(), 456i64.into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" NOT IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![123i64.into(), 456i64.into()]);

        // A URL with a filter on a string column and a value that looks like an integer (eq):
        let url = "http://example.com/penguin?penguin.study_name=eq.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like a real (eq):
        let url = "http://example.com/penguin?penguin.study_name=eq.123.456&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.123.456",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123.456".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123.456".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (not_eq):
        let url = "http://example.com/penguin?penguin.study_name=not_eq.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "not_eq.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" <> {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" <> {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (like):
        let url = "http://example.com/penguin?penguin.study_name=like.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "like.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" LIKE {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" LIKE {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (gt):
        let url = "http://example.com/penguin?penguin.study_name=gt.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "gt.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" > {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" > {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (gte):
        let url = "http://example.com/penguin?penguin.study_name=gte.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "gte.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" >= {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" >= {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (lt):
        let url = "http://example.com/penguin?penguin.study_name=lt.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "lt.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" < {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" < {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (gte):
        let url = "http://example.com/penguin?penguin.study_name=lte.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "lte.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" <= {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" <= {sql_param}"#
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (is):
        let url = "http://example.com/penguin?penguin.study_name=is.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_clause = is_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}"#,
                is_clause = is_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (is_not):
        let url = "http://example.com/penguin?penguin.study_name=is_not.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is_not.123",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_not_clause = is_not_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec!["123".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}"#,
                is_not_clause = is_not_clause(&rltbl.pool.syntax().name()),
            )
        );
        assert_eq!(params, vec!["123".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (in):
        let mut sql_param_gen = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.study_name=in.(123,456)&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "in.(123,456)",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" IN ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into(), "456".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec!["123".into(), "456".into()]);

        // A URL with a filter on a string column and a value that looks like an integer (not_in):
        let mut sql_param_gen = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.study_name=not_in.(123,456)&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "not_in.(123,456)",
           "limit": "1",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" NOT IN ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC
LIMIT 1"#
            )
        );
        assert_eq!(params, vec!["123".into(), "456".into()]);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" NOT IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec!["123".into(), "456".into()]);

        // A URL with a filter on the change ID
        let mut sql_param_gen = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let sql_param_3 = sql_param_gen.next();
        let url = "http://example.com/penguin?_change_id=gt.5";
        let query_params = from_value(json!({
           "_change_id": "gt.5",
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        // WARN: SQLite does not handle $VVV queries in the order we expect.
        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
, (SELECT MAX(change_id) FROM history
                    WHERE "table" = {sql_param_1}
                      AND "row" = "penguin"._id
                   ) AS _change_id
FROM "penguin"
WHERE (SELECT MAX(change_id) FROM history
                    WHERE "table" = {sql_param_3}
                      AND "row" = "penguin"._id
                   ) > {sql_param_2}
ORDER BY "penguin"._order ASC
LIMIT 100"#
            ),
        );
        assert_eq!(
            params,
            vec!["penguin".into(), 5i64.into(), "penguin".into()]
        );
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE (SELECT MAX(change_id) FROM history
                    WHERE "table" = {sql_param_2}
                      AND "row" = "penguin"._id
                   ) > {sql_param_1}"#
            ),
        );
        assert_eq!(params, vec![5i64.into(), "penguin".into()]);

        // A URL that includes an expression
        let url = "http://example.com/penguin?select=sample_number,count()";
        let query_params = from_value(json!({
            "select": "sample_number,count()"
        }))
        .unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl).await;
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  "sample_number",
  count()
FROM "penguin"
ORDER BY "penguin"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin""#
        );
        assert_eq!(params, empty);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_select_methods() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_select_methods")
            .init(true)
            .await?;
        let drop_sql = r#"DROP TABLE IF EXISTS "penguin_test""#;
        let create_sql = r#"CREATE TABLE "penguin_test" (
    _id INTEGER,
    _order INTEGER,
    study_name TEXT,
    sample_number INTEGER,
    species TEXT,
    island TEXT,
    individual_id TEXT,
    bill_length REAL,
    bill_depth NUMERIC,
    body_mass BIGINT
)"#;
        rltbl.pool.execute(drop_sql, ()).await.unwrap();
        rltbl.pool.execute(create_sql, ()).await.unwrap();
        let empty: Vec<DbValue> = vec![];

        // select_columns
        let mut select = Select::from("penguin_test");
        select.select_table_columns("penguin_test", &vec!["species", "island"]);
        select.select_columns(&vec!["study_name", "body_mass"]);

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  "penguin_test"."species",
  "penguin_test"."island",
  "study_name",
  "body_mass"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);
        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_alias
        let mut select = Select::from("penguin_test");
        select.select_alias("penguin_test", "island", "location");

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  "penguin_test"."island" AS "location"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_expression
        let mut select = Select::from("penguin_test");
        select.select_expression("CASE WHEN island = 'Biscoe' THEN 'BISCOE' END", "location");

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  CASE WHEN island = 'Biscoe' THEN 'BISCOE' END AS "location"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_all
        let mut select = Select::from("penguin_test");
        select.select_all(&rltbl, "penguin_test").await.unwrap();

        let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  "_id",
  "_order",
  "study_name",
  "sample_number",
  "species",
  "island",
  "individual_id",
  "bill_length",
  "bill_depth",
  "body_mass"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        rltbl.pool.execute(drop_sql, ()).await.unwrap();

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_subquery() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_subquery").init(true).await?;
        let sql_param = SqlParam::new(&rltbl.pool.syntax().name()).next();

        // Subquery select, filtered on a string:
        let mut inner_select = Select::from("penguin").limit(&0);
        inner_select.select_table_column("penguin", "individual_id");
        inner_select.left_join("penguin", "individual_id", "egg", "individual_id");
        inner_select
            .table_eq("penguin", "individual_id", "N1")
            .unwrap();
        let mut outer_select = Select::from("penguin").limit(&0);
        outer_select.is_in_subquery("individual_id", &inner_select);

        let tables = outer_select.get_tables().into_iter().collect::<Vec<_>>();
        assert_eq!(tables, vec!["egg", "penguin"]);

        let (sql, params) = outer_select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "penguin"."individual_id" = {sql_param}
)
ORDER BY "penguin"._order ASC"#
            )
        );
        assert_eq!(params, vec!["N1".into()]);

        let (sql, params) = outer_select
            .to_sql_count(&rltbl.pool.syntax().name())
            .unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "penguin"."individual_id" = {sql_param}
)"#
            )
        );
        assert_eq!(params, vec!["N1".into()]);

        // Subquery select, filtered on an integer:
        let mut inner_select = Select::from("penguin").limit(&0);
        inner_select.select_table_column("penguin", "sample_number");
        inner_select.left_join("penguin", "sample_number", "egg", "sample_number");
        inner_select
            .table_eq("penguin", "sample_number", 27)
            .unwrap();
        let mut outer_select = Select::from("penguin").limit(&0);
        outer_select.is_in_subquery("sample_number", &inner_select);

        let tables = outer_select.get_tables().into_iter().collect::<Vec<_>>();
        assert_eq!(tables, vec!["egg", "penguin"]);

        let (sql, params) = outer_select.to_sql(&rltbl.pool.syntax().name()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."sample_number" IN (
  SELECT
    "penguin"."sample_number"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."sample_number" = "egg"."sample_number"
  WHERE "penguin"."sample_number" = {sql_param}
)
ORDER BY "penguin"._order ASC"#
            )
        );
        assert_eq!(params, vec![27.into()]);

        let (sql, params) = outer_select
            .to_sql_count(&rltbl.pool.syntax().name())
            .unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" IN (
  SELECT
    "penguin"."sample_number"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."sample_number" = "egg"."sample_number"
  WHERE "penguin"."sample_number" = {sql_param}
)"#
            )
        );
        assert_eq!(params, vec![27.into()]);

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_filters() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_filters").init(true).await?;
        let mut sql_param_generator = SqlParam::new(&rltbl.pool.syntax().name());
        let sql_param_1 = sql_param_generator.next();
        let sql_param_2 = sql_param_generator.next();
        let is_for_kind = is_clause(&rltbl.pool.syntax().name());
        let is_not_for_kind = is_not_clause(&rltbl.pool.syntax().name());

        // Test simple string filters
        for (input_symbol, output_symbol) in [
            ("~=", "LIKE"),
            ("=", "="),
            ("!=", "<>"),
            (">", ">"),
            (">=", ">="),
            ("<", "<"),
            ("<=", "<="),
            ("is", &is_for_kind),
            ("is not", &is_not_for_kind),
        ] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!("study_name {input_symbol} FAKE123")])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "study_name" {output_symbol} {sql_param_1}
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec!["FAKE123".into()]);

            let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "study_name" {output_symbol} {sql_param_1}"#
                )
            );
            assert_eq!(params, vec!["FAKE123".into()]);
        }

        // Test simple integer filters
        for (input_symbol, output_symbol) in [
            ("=", "="),
            ("!=", "<>"),
            (">", ">"),
            (">=", ">="),
            ("<", "<"),
            ("<=", "<="),
            ("is", &is_for_kind),
            ("is not", &is_not_for_kind),
        ] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!("sample_number {input_symbol} 2")])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "sample_number" {output_symbol} {sql_param_1}
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![2i64.into()]);

            let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" {output_symbol} {sql_param_1}"#
                )
            );
            assert_eq!(params, vec![2i64.into()]);
        }

        // Test list string filters
        for (input_symbol, output_symbol) in [("in", "IN"), ("not in", "NOT IN")] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!(
                    "study_name {input_symbol} (MIKE123, RICK123)"
                )])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "study_name" {output_symbol} ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec!["MIKE123".into(), "RICK123".into()]);

            let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "study_name" {output_symbol} ({sql_param_1}, {sql_param_2})"#
                )
            );
            assert_eq!(params, vec!["MIKE123".into(), "RICK123".into()]);
        }

        // Test list integer filters
        for (input_symbol, output_symbol) in [("in", "IN"), ("not in", "NOT IN")] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!("sample_number {input_symbol} (1, 2)")])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "sample_number" {output_symbol} ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![1i64.into(), 2i64.into()]);

            let (sql, params) = select.to_sql_count(&rltbl.pool.syntax().name()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" {output_symbol} ({sql_param_1}, {sql_param_2})"#
                )
            );
            assert_eq!(params, vec![1i64.into(), 2i64.into()]);
        }

        rltbl.drop_test().await
    }

    #[tokio::test]
    async fn test_tablesets() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_tablesets").init(true).await?;
        let dataset = crate::demo::vaccine::Dataset::generate(1, 1);
        dataset.build(&rltbl).await?;

        let select = Select::from("participant");
        let expected = select.clone();
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "nothing to filter");

        let mut select = Select::from("participant");
        select.where_eq("participant_id", 1)?;
        let expected = select.clone();
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "nothing to join");

        let mut select = Select::from("participant");
        select.view_name = "participant_text".to_string();
        select.where_eq("participant_id", 1)?;
        let mut expected = select.clone();
        expected.joins = vec![Join {
            join_type: JoinType::Join,
            left_table: "participant_text".to_string(),
            left_column: "_id".to_string(),
            right_table: "participant".to_string(),
            right_column: "_id".to_string(),
        }];
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "join text view to self");

        let mut select = Select::from("participant");
        select.table_eq("specimen", "specimen_id", 1)?;
        let mut inner = select.clone();
        inner.select_table_column("participant", "_id");
        inner.joins = vec![Join {
            join_type: JoinType::Join,
            left_table: "participant".to_string(),
            left_column: "participant_id".to_string(),
            right_table: "specimen".to_string(),
            right_column: "participant_id".to_string(),
        }];
        let mut expected = Select::from("participant");
        expected.is_in_subquery("_id", &inner);
        expected.limit = 0;
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "join participant to specimen");

        let mut select = Select::from("participant");
        select.table_eq("data", "data_id", 1)?;
        let mut inner = select.clone();
        inner.select_table_column("participant", "_id");
        inner.joins = vec![Join {
            join_type: JoinType::Join,
            left_table: "participant".to_string(),
            left_column: "participant_id".to_string(),
            right_table: "data".to_string(),
            right_column: "participant_id".to_string(),
        }];
        let mut expected = Select::from("participant");
        expected.is_in_subquery("_id", &inner);
        expected.limit = 0;
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "join participant to data");

        let mut select = Select::from("participant");
        select.table_eq("investigation", "investigation_id", 1)?;
        let mut inner = select.clone();
        inner.select_table_column("participant", "_id");
        inner.joins = vec![
            Join {
                join_type: JoinType::Join,
                left_table: "participant".to_string(),
                left_column: "arm_id".to_string(),
                right_table: "arm".to_string(),
                right_column: "arm_id".to_string(),
            },
            Join {
                join_type: JoinType::Join,
                left_table: "arm".to_string(),
                left_column: "investigation_id".to_string(),
                right_table: "investigation".to_string(),
                right_column: "investigation_id".to_string(),
            },
        ];
        let mut expected = Select::from("participant");
        expected.is_in_subquery("_id", &inner);
        expected.limit = 0;
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "join participant to investigation");

        let mut select = Select::from("participant");
        select.table_eq("arm", "arm_id", 1)?;
        select.table_eq("specimen", "specimen_id", 1)?;
        let mut inner = select.clone();
        inner.select_table_column("participant", "_id");
        inner.joins = vec![
            Join {
                join_type: JoinType::Join,
                left_table: "participant".to_string(),
                left_column: "arm_id".to_string(),
                right_table: "arm".to_string(),
                right_column: "arm_id".to_string(),
            },
            Join {
                join_type: JoinType::Join,
                left_table: "participant".to_string(),
                left_column: "participant_id".to_string(),
                right_table: "specimen".to_string(),
                right_column: "participant_id".to_string(),
            },
        ];
        let mut expected = Select::from("participant");
        expected.is_in_subquery("_id", &inner);
        expected.limit = 0;
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(expected, actual, "join participant to arm and specimen");

        let mut select = Select::from("participant");
        select.table_eq("investigation", "investigation_id", 1)?;
        select.table_eq("arm", "arm_id", 1)?;
        select.table_eq("specimen", "specimen_id", 1)?;
        let mut inner = select.clone();
        inner.select_table_column("participant", "_id");
        inner.joins = vec![
            Join {
                join_type: JoinType::Join,
                left_table: "participant".to_string(),
                left_column: "arm_id".to_string(),
                right_table: "arm".to_string(),
                right_column: "arm_id".to_string(),
            },
            Join {
                join_type: JoinType::Join,
                left_table: "arm".to_string(),
                left_column: "investigation_id".to_string(),
                right_table: "investigation".to_string(),
                right_column: "investigation_id".to_string(),
            },
            Join {
                join_type: JoinType::Join,
                left_table: "participant".to_string(),
                left_column: "participant_id".to_string(),
                right_table: "specimen".to_string(),
                right_column: "participant_id".to_string(),
            },
        ];
        let mut expected = Select::from("participant");
        expected.is_in_subquery("_id", &inner);
        expected.limit = 0;
        let actual = joined_query(&rltbl, "vaccine", &select).await?;
        assert_eq!(
            expected, actual,
            "join participant to investigation and arm and specimen"
        );

        Ok(())
    }
}
