//! # rltbl/relatable
//!
//! This is [relatable](crate) (rltbl::[select](crate::select)).

use crate::{
    core::{Page, Relatable, RelatableError, Tab, DEFAULT_LIMIT},
    sql::{self, DbKind, SqlParam},
    table::Table,
};
use anyhow::Result;
use enquote::unquote;
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value as JsonValue};
use std::collections::{BTreeSet, HashSet};

/// Represents a SELECT statement.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
        tracing::trace!("Select::from_path_and_query({path:?}, {query_params:?})");
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

        fn value_as_type(datatype: &Option<String>, column: &str, value: &str) -> JsonValue {
            fn try_parse_as_int(value: &str) -> JsonValue {
                match value.parse::<i64>() {
                    Ok(signed) => json!(signed),
                    _ => {
                        tracing::warn!("Could not parse {value} as integer. Treating as string");
                        JsonValue::String(value.to_string())
                    }
                }
            }

            fn try_parse_as_decimal(value: &str) -> JsonValue {
                match value.parse::<f64>() {
                    Ok(signed) => json!(signed),
                    _ => {
                        tracing::warn!("Could not parse {value} as decimal. Treating as string");
                        JsonValue::String(value.to_string())
                    }
                }
            }

            if ["_id", "_order", "_change_id"].contains(&column) {
                try_parse_as_int(value)
            } else if ["_history", "_message"].contains(&column) {
                JsonValue::String(value.to_string())
            } else {
                match datatype {
                    Some(datatype) if datatype == "integer" => try_parse_as_int(value),
                    Some(datatype) if datatype == "decimal" => try_parse_as_decimal(value),
                    Some(datatype) if datatype == "text" => JsonValue::String(value.to_string()),
                    Some(datatype) => {
                        tracing::warn!(
                            "Unsupported datatype: {datatype}. Treating {value} as string"
                        );
                        JsonValue::String(value.to_string())
                    }
                    None => JsonValue::String(value.to_string()),
                }
            }
        }

        let base_table_name = path.split(".").next().unwrap_or_default();
        for (lhs, pattern) in query_params {
            let (table, column) = match lhs.split_once(".") {
                Some((table, column)) => (table.to_string(), column.to_string()),
                None => (String::new(), lhs),
            };
            let table_config = {
                let table_name = match table.as_str() {
                    "" => base_table_name,
                    table => &table,
                };
                Table::get_table(table_name, &rltbl)
                    .await
                    .expect("Can't get table '{table_name}'")
            };
            if pattern.starts_with("like.") {
                let value = &pattern.replace("like.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::Like {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::Like {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else {
                let datatype = table_config.get_configured_column_attribute(&column, "datatype");
                if pattern.starts_with("eq.") {
                    let value = &pattern.replace("eq.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::Equal {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("not_eq.") {
                    let value = &pattern.replace("not_eq.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::NotEqual {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("gt.") {
                    let value = &pattern.replace("gt.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::GreaterThan {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("gte.") {
                    let value = &pattern.replace("gte.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::GreaterThanOrEqual {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("lt.") {
                    let value = &pattern.replace("lt.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::LessThan {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("lte.") {
                    let value = &pattern.replace("lte.", "");
                    let value = value_as_type(&datatype, &column, value);
                    filters.push(Filter::LessThanOrEqual {
                        table,
                        column,
                        value,
                    })
                } else if pattern.starts_with("is.") {
                    let value = pattern.replace("is.", "");
                    if value.to_lowercase() == "null" {
                        filters.push(Filter::Is {
                            table,
                            column,
                            value: JsonValue::Null,
                        })
                    } else {
                        let value = value_as_type(&datatype, &column, &value);
                        filters.push(Filter::Is {
                            table,
                            column,
                            value,
                        })
                    }
                } else if pattern.starts_with("is_not.") {
                    let value = pattern.replace("is_not.", "");
                    if value.to_lowercase() == "null" {
                        filters.push(Filter::IsNot {
                            table,
                            column,
                            value: JsonValue::Null,
                        })
                    } else {
                        let value = value_as_type(&datatype, &column, &value);
                        filters.push(Filter::IsNot {
                            table,
                            column,
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
                        .map(|v| value_as_type(&datatype, &column, v))
                        .collect::<Vec<_>>();
                    filters.push(Filter::In {
                        table,
                        column,
                        value: json!(values),
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
                        .map(|v| value_as_type(&datatype, &column, v))
                        .collect::<Vec<_>>();
                    filters.push(Filter::NotIn {
                        table,
                        column,
                        value: json!(values),
                    })
                }
            }
        }

        Self {
            table_name: base_table_name.to_string(),
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
            match join {
                Join::LeftJoin {
                    left_table,
                    right_table,
                    ..
                } => {
                    insert_when_non_empty(&mut tables, &left_table);
                    insert_when_non_empty(&mut tables, &right_table);
                }
            };
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
    pub async fn select_all(&mut self, rltbl: &Relatable, table: &str) -> Result<&Self> {
        for column in rltbl.fetch_all_columns(&table).await? {
            self.select.push(SelectField::Column {
                table: String::new(),
                column: column.name,
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
        self.joins.push(Join::LeftJoin {
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

        fn parse_as_value(value: &str) -> Result<JsonValue> {
            fn maybe_quote(value: &str) -> Result<JsonValue> {
                if value.starts_with("\"") {
                    let value = serde_json::from_str(&value)?;
                    Ok(value)
                } else {
                    let value = serde_json::from_str(&format!(r#""{value}""#))?;
                    Ok(value)
                }
            }

            match value.parse::<i64>() {
                Ok(signed) => Ok(json!(signed)),
                _ => match value.parse::<f64>() {
                    Ok(float) => Ok(json!(float)),
                    _ => maybe_quote(value),
                },
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
                    "null" => JsonValue::Null,
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
                    "null" => JsonValue::Null,
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
                    .map(|v| serde_json::from_str::<JsonValue>(v).unwrap_or(json!(v.to_string())))
                    .collect::<Vec<_>>();
                self.filters.push(Filter::In {
                    table: "".to_string(),
                    column,
                    value: json!(values),
                });
            } else if is_not_in.is_match(&filter) {
                let captures = is_not_in.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let values = &captures.get(3).unwrap().as_str();
                let separator = Regex::new(r"\s*,\s*").unwrap();
                let values = separator
                    .split(values)
                    .map(|v| serde_json::from_str::<JsonValue>(v).unwrap_or(json!(v.to_string())))
                    .collect::<Vec<_>>();
                self.filters.push(Filter::NotIn {
                    table: "".to_string(),
                    column,
                    value: json!(values),
                });
            } else {
                return Err(RelatableError::ConfigError(format!("invalid filter {filter}")).into());
            }
        }
        Ok(self)
    }

    /// Add a like filter for the given column on the given value, which may include '%' wildcards
    pub fn like<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::like({column:?}, value)");
        self.filters.push(Filter::Like {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an equals filter on the given column and value.
    pub fn eq<T>(&mut self, column: &str, value: &T) -> Result<&Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::eq({column:?}, value)");
        self.filters.push(Filter::Equal {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an equals filter on the given column and value.
    pub fn table_eq<T>(&mut self, table: &str, column: &str, value: &T) -> Result<&Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::table_eq({column:?}, value)");
        self.filters.push(Filter::Equal {
            table: table.to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add a not-equals filter on the given column and value.
    pub fn not_eq<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::not_eq({column:?}, value)");
        self.filters.push(Filter::NotEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an greater-than filter on the given column and value.
    pub fn gt<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::gt({column:?}, value)");
        self.filters.push(Filter::GreaterThan {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an greater-than-or-equals filter on the given column and value.
    pub fn gte<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::gte({column:?}, value)");
        self.filters.push(Filter::GreaterThanOrEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an less-than filter on the given column and value.
    pub fn lt<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::lt({column:?}, value)");
        self.filters.push(Filter::LessThan {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an less-than-or-equals filter on the given column and value.
    pub fn lte<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::lte({column:?}, value)");
        self.filters.push(Filter::LessThanOrEqual {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an is filter on the given column and value.
    pub fn is<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::is({column:?}, value)");
        self.filters.push(Filter::Is {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an is not filter on the given column and value.
    pub fn is_not<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::is_not({column:?}, value)");
        self.filters.push(Filter::IsNot {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add an in filter on the given column and value.
    pub fn is_in<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::is_in({column:?}, value)");
        self.filters.push(Filter::In {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
        });
        Ok(self)
    }

    /// Add a not in filter on the given column and value.
    pub fn is_not_in<T>(mut self, column: &str, value: &T) -> Result<Self>
    where
        T: Serialize,
    {
        tracing::trace!("Select::is_not_in({column:?}, value)");
        self.filters.push(Filter::NotIn {
            table: "".to_string(),
            column: column.to_string(),
            value: to_value(value)?,
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
    pub fn to_sql(&self, kind: &DbKind) -> Result<(String, Vec<JsonValue>)> {
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
                   ) AS _change_id"#,
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
                let (_, c, _, _) = filter.parts();
                if c == "_change_id" {
                    lines.push(format!(", {}", get_change_sql(&mut sql_param_gen)));
                    params.push(json!(self.table_name));
                }
            }
        } else {
            lines.push("SELECT".to_string());
            for filter in &self.filters {
                let (_, c, _, _) = filter.parts();
                if c == "_change_id" {
                    lines.push(get_change_sql(&mut sql_param_gen));
                    params.push(json!(self.table_name));
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
            let (t, _, _, _) = filter.parts();
            if self.view_name != "" && t == self.table_name {
                filter.set_table(&self.view_name);
            }
            let (filter_sql, mut filter_params) = filter.to_sql(&mut sql_param_gen)?;
            lines.push(format!("{keyword} {filter_sql}"));
            params.append(&mut filter_params);
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
    pub fn to_sql_count(&self, kind: &DbKind) -> Result<(String, Vec<JsonValue>)> {
        tracing::trace!("Select::to_sql_count({self:?}, {kind:?})");
        let target = match self.view_name.as_str() {
            "" => &self.table_name,
            _ => &self.view_name,
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
            let (t, _, _, _) = filter.parts();
            if self.view_name != "" && t == self.table_name {
                filter.set_table(&self.view_name);
            }
            let (s, p) = filter.to_sql_count(kind)?;
            lines.push(format!("{keyword} {s}"));
            params.append(&mut p.clone());
        }
        Ok((lines.join("\n"), params))
    }

    /// Converts this select's filters to a map from column names to URL representations of their
    /// associated filters represented as [JsonValue]s
    pub fn to_params(&self) -> Result<IndexMap<String, JsonValue>> {
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
                let (table, column, _, _) = filter.parts();

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
                params.insert(lhs, filter.to_url()?.into());
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
                let s = match value {
                    serde_json::Value::String(s) => s.as_str().into(),
                    _ => format!("{}", value),
                };
                parts.push(format!("{column}={s}"));
            }
            Ok(format!("{}?{}", path, parts.join("&")))
        } else {
            Ok(path.to_string())
        }
    }

    pub fn to_page(&self, root: &str, path: &str, tabs: &Vec<String>) -> Result<Page> {
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
        let tabs = tabs
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
            tabs,
        })
    }
}

/// A field in a [Select] clause.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Join {
    LeftJoin {
        left_table: String,
        left_column: String,
        right_table: String,
        right_column: String,
    },
}

impl Join {
    pub fn to_sql(&self) -> String {
        match self {
            Join::LeftJoin {
                left_table,
                left_column,
                right_table,
                right_column,
            } => {
                let (t, lt, lc, rt, rc) = (
                    &right_table,
                    &left_table,
                    &left_column,
                    &right_table,
                    &right_column,
                );
                format!(r#"LEFT JOIN "{t}" ON "{lt}"."{lc}" = "{rt}"."{rc}""#)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Filter {
    Like {
        table: String,
        column: String,
        value: JsonValue,
    },
    Equal {
        table: String,
        column: String,
        value: JsonValue,
    },
    NotEqual {
        table: String,
        column: String,
        value: JsonValue,
    },
    GreaterThan {
        table: String,
        column: String,
        value: JsonValue,
    },
    GreaterThanOrEqual {
        table: String,
        column: String,
        value: JsonValue,
    },
    LessThan {
        table: String,
        column: String,
        value: JsonValue,
    },
    LessThanOrEqual {
        table: String,
        column: String,
        value: JsonValue,
    },
    Is {
        table: String,
        column: String,
        value: JsonValue,
    },
    IsNot {
        table: String,
        column: String,
        value: JsonValue,
    },
    In {
        table: String,
        column: String,
        value: JsonValue,
    },
    NotIn {
        table: String,
        column: String,
        value: JsonValue,
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

    pub fn parts(&self) -> (String, String, String, JsonValue) {
        tracing::trace!("Filter::parts()");
        let (table, column, operator, value) = match self {
            Filter::Like {
                table,
                column,
                value,
            } => (table, column, "like", value),
            Filter::Equal {
                table,
                column,
                value,
            } => (table, column, "eq", value),
            Filter::NotEqual {
                table,
                column,
                value,
            } => (table, column, "not_eq", value),
            Filter::GreaterThan {
                table,
                column,
                value,
            } => (table, column, "gt", value),
            Filter::GreaterThanOrEqual {
                table,
                column,
                value,
            } => (table, column, "gte", value),
            Filter::LessThan {
                table,
                column,
                value,
            } => (table, column, "lt", value),
            Filter::LessThanOrEqual {
                table,
                column,
                value,
            } => (table, column, "lte", value),
            Filter::Is {
                table,
                column,
                value,
            } => (table, column, "is", value),
            Filter::IsNot {
                table,
                column,
                value,
            } => (table, column, "is_not", value),
            Filter::In {
                table,
                column,
                value,
            } => (table, column, "in", value),
            Filter::NotIn {
                table,
                column,
                value,
            } => (table, column, "not_in", value),
            Filter::InSubquery {
                table,
                column,
                subquery,
            } => (table, column, "in", &json!(subquery)),
            Filter::NotInSubquery {
                table,
                column,
                subquery,
            } => (table, column, "not_in", &json!(subquery)),
        };
        (
            table.to_string(),
            column.to_string(),
            operator.to_string(),
            json!(value),
        )
    }

    pub fn get_table(&self) -> String {
        self.parts().0
    }

    pub fn get_column(&self) -> String {
        self.parts().1
    }

    pub fn get_operator(&self) -> String {
        self.parts().2
    }

    pub fn get_value(&self) -> JsonValue {
        self.parts().3
    }

    pub fn to_url(&self) -> Result<String> {
        tracing::trace!("Filter::to_url()");

        fn handle_string_value(token: &str) -> String {
            let reserved = vec![':', ',', '.', '(', ')'];
            if token.chars().all(char::is_numeric) || reserved.iter().any(|&c| token.contains(c)) {
                if token.contains(char::is_whitespace) {
                    format!("\"{}\"", token)
                } else {
                    token.to_string()
                }
            } else {
                token.to_string()
            }
        }

        let (_, _, operator, value) = self.parts();
        let rhs = match &value {
            JsonValue::Null => "null".to_string(),
            JsonValue::String(string) => handle_string_value(&string),
            JsonValue::Number(number) => format!("{number}"),
            JsonValue::Array(vector) => {
                let mut list = vec![];
                for item in vector {
                    match item {
                        JsonValue::String(string) => list.push(handle_string_value(&string)),
                        JsonValue::Number(number) => list.push(number.to_string()),
                        _ => {
                            return Err(RelatableError::DataError(format!(
                                "Not all list items in {vector:?} are strings or numbers.",
                            ))
                            .into());
                        }
                    };
                }
                format!("({})", list.join(","))
            }
            _ => {
                if let Filter::InSubquery { .. } | Filter::NotInSubquery { .. } = self {
                    tracing::error!("Subquery filters are unsupported: {self:?}");
                }
                return Err(RelatableError::DataError(format!(
                    "RHS of Filter: {:?} is not a string, number, or list",
                    self
                ))
                .into());
            }
        };

        Ok(format!("{operator}.{rhs}"))
    }

    pub fn to_sql(&self, sql_param: &mut SqlParam) -> Result<(String, Vec<JsonValue>)> {
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
                let value = match value {
                    JsonValue::Bool(value) => value.to_string(),
                    JsonValue::Number(value) => value.to_string(),
                    JsonValue::String(value) => value.to_string(),
                    JsonValue::Null => "NULL".to_string(),
                    JsonValue::Array(value) => format!("{value:?}"),
                    JsonValue::Object(value) => format!("{value:?}"),
                };
                let value = value.replace("*", "%");
                Ok((
                    format!(
                        r#"{lhs} LIKE {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::Equal {
                table,
                column,
                value,
            } => Ok((
                format!(
                    r#"{lhs} = {sql_param}"#,
                    lhs = generate_lhs(table, column),
                    sql_param = sql_param.next()
                ),
                vec![json!(value)],
            )),
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
                vec![json!(value)],
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
                vec![json!(value)],
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
                vec![json!(value)],
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
                vec![json!(value)],
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
                vec![json!(value)],
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
                vec![json!(value)],
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
                vec![json!(value)],
            )),
            Filter::In {
                table,
                column,
                value,
            } => {
                if let JsonValue::Array(values) = value {
                    let lhs = generate_lhs(table, column);
                    match render_values(values, sql_param) {
                        Err(e) => {
                            return Err(RelatableError::DataError(format!(
                                "Error rendering 'in' filter: {e}"
                            ))
                            .into());
                        }
                        Ok((rhs, values)) => Ok((format!("{lhs} IN {rhs}"), values)),
                    }
                } else {
                    Err(RelatableError::DataError(format!("Invalid 'in' value: {value}")).into())
                }
            }
            Filter::NotIn {
                table,
                column,
                value,
            } => {
                if let JsonValue::Array(values) = value {
                    let lhs = generate_lhs(table, column);
                    match render_values(values, sql_param) {
                        Err(e) => {
                            return Err(RelatableError::DataError(format!(
                                "Error rendering 'not in' filter: {e}"
                            ))
                            .into());
                        }
                        Ok((rhs, values)) => Ok((format!("{lhs} NOT IN {rhs}"), values)),
                    }
                } else {
                    Err(
                        RelatableError::DataError(format!("Invalid 'not in' value: {value}"))
                            .into(),
                    )
                }
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
    pub fn to_sql_count(&self, kind: &DbKind) -> Result<(String, Vec<JsonValue>)> {
        tracing::trace!("Filter::to_sql_count({self:?}, {kind:?})");
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
                let (sql, params) = subquery.to_sql(kind)?;
                let lines: Vec<&str> = sql
                    .split("\n")
                    .filter(|x| !x.starts_with("ORDER BY"))
                    .filter(|x| !x.starts_with("LIMIT"))
                    .filter(|x| !x.starts_with("OFFSET"))
                    .collect();
                let sql = lines.join("\n  ");
                Ok((format!("{lhs} IN (\n  {sql}\n)"), params))
            }
            _ => self.to_sql(&mut SqlParam::new(kind)),
        }
    }
}

/// Represents an ORDER BY clause in a SELECT statement.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
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
    options: &Vec<JsonValue>,
    sql_param_gen: &mut SqlParam,
) -> Result<(String, Vec<JsonValue>)> {
    let mut sql_params = vec![];
    let mut values = vec![];
    let mut is_string_list = false;
    for (i, option) in options.iter().enumerate() {
        match option {
            JsonValue::String(str_opt) => {
                if i == 0 {
                    is_string_list = true;
                } else if !is_string_list {
                    return Err(RelatableError::InputError(format!(
                        "{:?} contains both text and numeric types.",
                        options
                    ))
                    .into());
                }
                sql_params.push(sql_param_gen.next());
                let value = unquote(str_opt).unwrap_or(str_opt.clone());
                values.push(format!("{value}").into())
            }
            JsonValue::Number(_) => {
                if i == 0 {
                    is_string_list = false;
                } else if is_string_list {
                    return Err(RelatableError::InputError(format!(
                        "{:?} contains both text and numeric types.",
                        options
                    ))
                    .into());
                }
                sql_params.push(sql_param_gen.next());
                values.push(option.clone())
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
    Ok((format!("({})", sql_params.join(", ")), values))
}

pub async fn joined_query(
    rltbl: &Relatable,
    tableset_name: &str,
    select: &Select,
) -> Result<Select> {
    let tables = select.get_tables();
    if tables.len() == 1 {
        return Ok(select.clone());
    }

    let tables: Vec<JsonValue> = tables.into_iter().map(|x| json!(x)).collect();
    let mut sql_param = sql::SqlParam::new(&rltbl.connection.kind());
    let (value_string, value_list) = render_values(&tables, &mut sql_param).unwrap();

    let sql = format!(
        r#"WITH RECURSIVE ancestors(_order, left_table, left_column, right_table, right_column) AS (
      SELECT _order, left_table, left_column, right_table, right_column
      FROM tableset
      WHERE right_table IN {value_string}
      UNION
      SELECT t._order, t.left_table, t.left_column, t.right_table, t.right_column
      FROM ancestors AS a
      JOIN tableset AS t ON a.left_table = t.right_table
      WHERE t.tableset = '{tableset_name}'
    )
    SELECT *
    FROM ancestors
    WHERE _order >= (SELECT MIN(_order) FROM ancestors WHERE left_table IN {value_string})
      AND _order <= (SELECT MAX(_order) FROM ancestors WHERE right_table IN {value_string})
    ORDER BY _order"#
    );
    tracing::info!("SQL {sql}");
    let mut three_value_lists = value_list.clone();
    three_value_lists.extend(value_list.clone());
    three_value_lists.extend(value_list.clone());
    let params = json!(three_value_lists);
    tracing::info!("PARAMS {params:?}");
    let json_rows = rltbl.connection.query(&sql, Some(&params)).await?;
    tracing::info!(
        "TABLESET {} {json_rows:?}",
        select.to_url("", &Format::Default)?
    );
    println!(
        "TABLESET {} {json_rows:?}",
        select.to_url("", &Format::Default)?
    );
    if json_rows.len() == 0 {
        return Err(RelatableError::ConfigError(format!("empty tableset")).into());
    }

    let limit = select.limit;
    let mut inner = select.clone();
    inner.select = vec![];
    let table_name = inner.table_name.clone();

    let pkey = "_id".to_string();
    inner.select_table_column(&table_name, &pkey);

    // Find the primary key for this table.
    // let mut pkey = String::new();
    // for json_row in json_rows.clone() {
    //     if table_name == json_row.get_string("left_table").unwrap() {
    //         pkey = json_row.get_string("left_column").unwrap();
    //         inner.select_table_column(&table_name, &pkey);
    //         // println!("FOUND PKEY in {json_row:?}");
    //     }
    // }
    // if table_name == json_rows.last().unwrap().get_string("right_table").unwrap() {
    // }
    //     inner.order_by(&pkey);
    // } else {
    //     inner.limit = 0;
    // }
    inner.order_by = vec![];
    inner.limit = 0;
    let json_row = json_rows.first().unwrap();
    inner.table_name = json_row.get_string("left_table").unwrap();
    let mut joined = HashSet::new();
    for json_row in json_rows.iter() {
        let left_table = json_row.get_string("left_table").unwrap();
        let right_table = json_row.get_string("right_table").unwrap();
        if &left_table == "" || &right_table == "" {
            continue;
        }
        if joined.contains(&right_table) {
            continue;
        }
        joined.insert(right_table.clone());
        inner.left_join(
            &left_table,
            &json_row.get_string("left_column").unwrap(),
            &right_table,
            &json_row.get_string("right_column").unwrap(),
        );
    }
    let (sql, params) = inner.to_sql(&rltbl.connection.kind()).unwrap();
    tracing::warn!("SQL {sql} PARAMS {params:?}");
    Ok(Select {
        table_name,
        filters: vec![Filter::InSubquery {
            table: String::new(),
            column: pkey.clone(),
            subquery: inner.clone(),
        }],
        limit,
        ..Default::default()
    })
}

// Tests

#[cfg(test)]
mod tests {
    use crate::sql::{is_clause, is_not_clause, CachingStrategy};
    use async_std::task::block_on;
    use pretty_assertions::assert_eq;
    use serde_json::from_value;

    use super::*;

    #[test]
    fn test_select_from_path_and_query() {
        let rltbl = block_on(Relatable::build_demo(
            Some("build/test_select_from_path_and_query.db"),
            &true,
            0,
            &CachingStrategy::Trigger,
        ))
        .unwrap();
        let sql_param = SqlParam::new(&rltbl.connection.kind()).next();
        let base = "http://example.com";
        let empty: Vec<JsonValue> = vec![];

        // A basic URL
        let url = "http://example.com/penguin";
        let query_params = from_value(json!({})).unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT *
FROM "penguin"
ORDER BY "penguin"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
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
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Json).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!(5)]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" = {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!(5)]);

        // A URL with a filter on a string column and a value with a space
        let url = "http://example.com/penguin?penguin.study_name=eq.FAKE 123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.FAKE 123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("FAKE 123")]);

        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("FAKE 123")]);

        // A URL with an IS NULL filter
        let url = "http://example.com/penguin?penguin.study_name=is.null&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is.null",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_clause = is_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![JsonValue::Null]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}"#,
                is_clause = is_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![JsonValue::Null]);

        // A URL with an IS NOT NULL filter
        let url = "http://example.com/penguin?penguin.study_name=is_not.null&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is_not.null",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_not_clause = is_not_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![JsonValue::Null]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}"#,
                is_not_clause = is_not_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![JsonValue::Null]);

        // A URL with an IN filter
        let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.sample_number=in.(123,345)&limit=1";
        let query_params = from_value(json!({
           "penguin.sample_number": "in.(123,345)",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!(123), json!(345)]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![json!(123), json!(345)]);

        // A URL with a NOT IN filter
        let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.sample_number=not_in.(123,345)&limit=1";
        let query_params = from_value(json!({
           "penguin.sample_number": "not_in.(123,345)",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!(123), json!(345)]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."sample_number" NOT IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![json!(123), json!(345)]);

        // A URL with a filter on a string column and a value that looks like an integer (eq):
        let url = "http://example.com/penguin?penguin.study_name=eq.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like a real (eq):
        let url = "http://example.com/penguin?penguin.study_name=eq.123.345&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "eq.123.345",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123.345")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" = {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123.345")]);

        // A URL with a filter on a string column and a value that looks like an integer (not_eq):
        let url = "http://example.com/penguin?penguin.study_name=not_eq.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "not_eq.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" <> {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (like):
        let url = "http://example.com/penguin?penguin.study_name=like.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "like.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" LIKE {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (gt):
        let url = "http://example.com/penguin?penguin.study_name=gt.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "gt.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" > {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (gte):
        let url = "http://example.com/penguin?penguin.study_name=gte.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "gte.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" >= {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (lt):
        let url = "http://example.com/penguin?penguin.study_name=lt.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "lt.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" < {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (gte):
        let url = "http://example.com/penguin?penguin.study_name=lte.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "lte.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" <= {sql_param}"#
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (is):
        let url = "http://example.com/penguin?penguin.study_name=is.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_clause = is_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_clause} {sql_param}"#,
                is_clause = is_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (is_not):
        let url = "http://example.com/penguin?penguin.study_name=is_not.123&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "is_not.123",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#,
                is_not_clause = is_not_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![json!("123")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" {is_not_clause} {sql_param}"#,
                is_not_clause = is_not_clause(&rltbl.connection.kind()),
            )
        );
        assert_eq!(params, vec![json!("123")]);

        // A URL with a filter on a string column and a value that looks like an integer (in):
        let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.study_name=in.(123,345)&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "in.(123,345)",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123"), json!("345")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![json!("123"), json!("345")]);

        // A URL with a filter on a string column and a value that looks like an integer (not_in):
        let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
        let sql_param_1 = sql_param_gen.next();
        let sql_param_2 = sql_param_gen.next();
        let url = "http://example.com/penguin?penguin.study_name=not_in.(123,345)&limit=1";
        let query_params = from_value(json!({
           "penguin.study_name": "not_in.(123,345)",
           "limit": "1",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("123"), json!("345")]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "penguin"."study_name" NOT IN ({sql_param_1}, {sql_param_2})"#
            )
        );
        assert_eq!(params, vec![json!("123"), json!("345")]);

        // A URL with a filter on the change ID
        let url = "http://example.com/penguin?_change_id=gt.5";
        let query_params = from_value(json!({
           "_change_id": "gt.5",
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
, (SELECT MAX(change_id) FROM history
                    WHERE "table" = {sql_param}
                      AND "row" = "penguin"._id
                   ) AS _change_id
FROM "penguin"
WHERE "_change_id" > {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 100"#
            ),
        );
        assert_eq!(params, vec![json!("penguin"), json!(5)]);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "_change_id" > {sql_param}"#
            ),
        );
        assert_eq!(params, vec![json!(5)]);

        // A URL that includes an expression
        let url = "http://example.com/penguin?select=sample_number,count()";
        let query_params = from_value(json!({
            "select": "sample_number,count()"
        }))
        .unwrap();
        let select = block_on(Select::from_path_and_query(
            "penguin",
            &query_params,
            &rltbl,
        ));
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin""#
        );
        assert_eq!(params, empty);
    }

    #[test]
    fn test_select_methods() {
        let rltbl = block_on(Relatable::init(
            &true,
            Some("build/test_select_methods.db"),
            &CachingStrategy::Trigger,
        ))
        .unwrap();
        let drop_sql = r#"DROP TABLE IF EXISTS "penguin_test""#;
        let create_sql = r#"CREATE TABLE "penguin_test" (
    _id INTEGER,
    _order INTEGER,
    study_name TEXT,
    sample_number INTEGER,
    species TEXT,
    island TEXT,
    individual_id TEXT,
    culmen_length REAL,
    culmen_depth NUMERIC,
    body_mass BIGINT
)"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        let empty: Vec<JsonValue> = vec![];

        // select_columns
        let mut select = Select::from("penguin_test");
        select.select_table_columns("penguin_test", &vec!["species", "island"]);
        select.select_columns(&vec!["study_name", "body_mass"]);

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_alias
        let mut select = Select::from("penguin_test");
        select.select_alias("penguin_test", "island", "location");

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  "penguin_test"."island" AS "location"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_expression
        let mut select = Select::from("penguin_test");
        select.select_expression("CASE WHEN island = 'Biscoe' THEN 'BISCOE' END", "location");

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT
  CASE WHEN island = 'Biscoe' THEN 'BISCOE' END AS "location"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        // select_all
        let mut select = Select::from("penguin_test");
        block_on(select.select_all(&rltbl, "penguin_test")).unwrap();

        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
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
  "culmen_length",
  "culmen_depth",
  "body_mass"
FROM "penguin_test"
ORDER BY "penguin_test"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);

        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "penguin_test""#
        );
        assert_eq!(params, empty);

        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
    }

    #[test]
    fn test_subquery() {
        let rltbl = block_on(Relatable::init(
            &true,
            Some("build/test_subquery.db"),
            &CachingStrategy::Trigger,
        ))
        .unwrap();
        let sql_param = SqlParam::new(&rltbl.connection.kind()).next();

        // Subquery select, filtered on a string:
        let mut inner_select = Select::from("penguin").limit(&0);
        inner_select.select_table_column("penguin", "individual_id");
        inner_select.left_join("penguin", "individual_id", "egg", "individual_id");
        inner_select
            .table_eq("penguin", "individual_id", &"N1")
            .unwrap();
        let mut outer_select = Select::from("penguin").limit(&0);
        outer_select.is_in_subquery("individual_id", &inner_select);

        let tables = outer_select.get_tables().into_iter().collect::<Vec<_>>();
        assert_eq!(tables, vec!["egg", "penguin"]);

        let (sql, params) = outer_select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("N1")]);

        let (sql, params) = outer_select.to_sql_count(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!("N1")]);

        // Subquery select, filtered on an integer:
        let mut inner_select = Select::from("penguin").limit(&0);
        inner_select.select_table_column("penguin", "sample_number");
        inner_select.left_join("penguin", "sample_number", "egg", "sample_number");
        inner_select
            .table_eq("penguin", "sample_number", &27)
            .unwrap();
        let mut outer_select = Select::from("penguin").limit(&0);
        outer_select.is_in_subquery("sample_number", &inner_select);

        let tables = outer_select.get_tables().into_iter().collect::<Vec<_>>();
        assert_eq!(tables, vec!["egg", "penguin"]);

        let (sql, params) = outer_select.to_sql(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!(27)]);

        let (sql, params) = outer_select.to_sql_count(&rltbl.connection.kind()).unwrap();
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
        assert_eq!(params, vec![json!(27)]);
    }

    #[test]
    fn test_filters() {
        let rltbl = block_on(Relatable::init(
            &true,
            Some("build/test_filters.db"),
            &CachingStrategy::Trigger,
        ))
        .unwrap();
        let mut sql_param_generator = SqlParam::new(&rltbl.connection.kind());
        let sql_param_1 = sql_param_generator.next();
        let sql_param_2 = sql_param_generator.next();
        let is_for_kind = is_clause(&rltbl.connection.kind());
        let is_not_for_kind = is_not_clause(&rltbl.connection.kind());

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
            let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "study_name" {output_symbol} {sql_param_1}
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![json!("FAKE123")]);

            let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "study_name" {output_symbol} {sql_param_1}"#
                )
            );
            assert_eq!(params, vec![json!("FAKE123")]);
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
            let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "sample_number" {output_symbol} {sql_param_1}
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![json!(2)]);

            let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" {output_symbol} {sql_param_1}"#
                )
            );
            assert_eq!(params, vec![json!(2)]);
        }

        // Test list string filters
        for (input_symbol, output_symbol) in [("in", "IN"), ("not in", "NOT IN")] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!(
                    "study_name {input_symbol} (MIKE123, RICK123)"
                )])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "study_name" {output_symbol} ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![json!("MIKE123"), json!("RICK123")]);

            let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "study_name" {output_symbol} ({sql_param_1}, {sql_param_2})"#
                )
            );
            assert_eq!(params, vec![json!("MIKE123"), json!("RICK123")]);
        }

        // Test list integer filters
        for (input_symbol, output_symbol) in [("in", "IN"), ("not in", "NOT IN")] {
            let select = Select::from("penguin")
                .limit(&0)
                .filters(&vec![format!("sample_number {input_symbol} (1, 2)")])
                .unwrap();
            let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT *
FROM "penguin"
WHERE "sample_number" {output_symbol} ({sql_param_1}, {sql_param_2})
ORDER BY "penguin"._order ASC"#
                )
            );
            assert_eq!(params, vec![json!(1), json!(2)]);

            let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
            assert_eq!(
                sql,
                format!(
                    r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" {output_symbol} ({sql_param_1}, {sql_param_2})"#
                )
            );
            assert_eq!(params, vec![json!(1), json!(2)]);
        }
    }

    #[test]
    fn test_tablesets() {
        let rltbl = block_on(Relatable::init(
            &true,
            Some("build/test_tablesets.db"),
            &CachingStrategy::None,
        ))
        .unwrap();
        let sql_param = SqlParam::new(&rltbl.connection.kind()).next();
        let base = "http://example.com/combined";
        let empty: Vec<JsonValue> = vec![];

        // Create five tables:
        //   / B \
        // A       B2C - D
        //   \ C /
        let insert_sql = r#"INSERT INTO "table"("table") VALUES
('A'), ('B'), ('C'), ('B2C'), ('D')"#;
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        let drop_sql = r#"DROP TABLE IF EXISTS "A""#;
        let create_sql = r#"CREATE TABLE "A" (
    _id INTEGER,
    _order INTEGER,
    "a" TEXT PRIMARY KEY
)"#;
        let insert_sql = r#"INSERT INTO "A" VALUES
(1, 1000, '1'),
(2, 2000, '2'),
(3, 3000, '3')"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        let drop_sql = r#"DROP TABLE IF EXISTS "B""#;
        let create_sql = r#"CREATE TABLE "B" (
    _id INTEGER,
    _order INTEGER,
    "a" TEXT,
    "b" TEXT PRIMARY KEY
)"#;
        let insert_sql = r#"INSERT INTO "B" VALUES
(1, 1000, '1', 'i'),
(2, 2000, '2', 'ii'),
(3, 3000, '3', 'iii')"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        let drop_sql = r#"DROP TABLE IF EXISTS "C""#;
        let create_sql = r#"CREATE TABLE "C" (
    _id INTEGER,
    _order INTEGER,
    "a" TEXT,
    "c" TEXT PRIMARY KEY
)"#;
        let insert_sql = r#"INSERT INTO "C" VALUES
(1, 1000, '1', 'x'),
(2, 2000, '2', 'y'),
(3, 3000, '3', 'z')"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        let drop_sql = r#"DROP TABLE IF EXISTS "B2C""#;
        let create_sql = r#"CREATE TABLE "B2C" (
    _id INTEGER,
    _order INTEGER,
    "b" TEXT,
    "c" TEXT
)"#;
        let insert_sql = r#"INSERT INTO "B2C" VALUES
(1, 1000, 'i', 'x'),
(2, 2000, 'ii', 'y'),
(3, 3000, 'iii', 'z')"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        let drop_sql = r#"DROP TABLE IF EXISTS "D""#;
        let create_sql = r#"CREATE TABLE "D" (
    _id INTEGER,
    _order INTEGER,
    "b" TEXT,
    "d" TEXT PRIMARY KEY
)"#;
        let insert_sql = r#"INSERT INTO "D" VALUES
(1, 1000, 'i', 'a'),
(2, 2000, 'ii', 'b'),
(3, 3000, 'iii', 'c')"#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        // Create the tablset table.
        let drop_sql = r#"DROP TABLE IF EXISTS "tableset""#;
        let create_sql = r#"CREATE TABLE tableset (
              _id INTEGER PRIMARY KEY AUTOINCREMENT,
              _order INTEGER UNIQUE,
              tableset TEXT,
              left_table TEXT,
              left_column TEXT,
              right_table TEXT,
              right_column TEXT
            )"#;
        let insert_sql = r#"INSERT INTO "tableset" VALUES
              (1, 1000, 'combined', NULL, NULL, 'A', 'a'),
              (2, 2000, 'combined', 'A', 'a', 'B', 'a'),
              (3, 3000, 'combined', 'A', 'a', 'C', 'a'),
              (4, 4000, 'combined', 'B', 'b', 'B2C', 'b'),
              (5, 5000, 'combined', 'C', 'c', 'B2C', 'c'),
              (7, 7000, 'combined', 'B', 'b', 'D', 'b')
            "#;
        block_on(rltbl.connection.query(drop_sql, None)).unwrap();
        block_on(rltbl.connection.query(create_sql, None)).unwrap();
        block_on(rltbl.connection.query(insert_sql, None)).unwrap();

        // Just query for the B table.
        let url = "http://example.com/combined/B";
        let query_params = from_value(json!({})).unwrap();
        let inner = block_on(Select::from_path_and_query("B", &query_params, &rltbl));
        let select = block_on(joined_query(&rltbl, "combined", &inner)).unwrap();
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT *
FROM "B"
ORDER BY "B"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, empty);
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "B""#
        );
        assert_eq!(params, empty);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();

        // Filter the B table by one of its own columns.
        let url = "http://example.com/combined/B?B.b=eq.i";
        let query_params = from_value(json!({"B.b": "eq.i"})).unwrap();
        let inner = block_on(Select::from_path_and_query("B", &query_params, &rltbl));

        let select = block_on(joined_query(&rltbl, "combined", &inner)).unwrap();
        assert_eq!(url, select.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT *
FROM "B"
WHERE "B"."b" = ?
ORDER BY "B"._order ASC
LIMIT 100"#
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            r#"SELECT COUNT(1) AS "count"
FROM "B"
WHERE "B"."b" = ?"#
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();

        // Filter the A table by one of the columns from B.
        let url = "http://example.com/combined/A?B.b=eq.i";
        let query_params = from_value(json!({"B.b": "eq.i"})).unwrap();
        let inner = block_on(Select::from_path_and_query("A", &query_params, &rltbl));
        let select = block_on(joined_query(&rltbl, "combined", &inner)).unwrap();
        assert_eq!(url, inner.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "A"
WHERE "_id" IN (
  SELECT
    "A"."_id"
  FROM "A"
  LEFT JOIN "B" ON "A"."a" = "B"."a"
  WHERE "B"."b" = {sql_param}
)
ORDER BY "A"._order ASC
LIMIT 100"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "A"
WHERE "_id" IN (
  SELECT
    "A"."_id"
  FROM "A"
  LEFT JOIN "B" ON "A"."a" = "B"."a"
  WHERE "B"."b" = {sql_param}
)"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();

        // Filter the B2C table by one of the columns from B.
        let url = "http://example.com/combined/B2C?B.b=eq.i";
        let query_params = from_value(json!({"B.b": "eq.i"})).unwrap();
        let inner = block_on(Select::from_path_and_query("B2C", &query_params, &rltbl));
        let select = block_on(joined_query(&rltbl, "combined", &inner)).unwrap();
        assert_eq!(url, inner.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "B2C"
WHERE "_id" IN (
  SELECT
    "B2C"."_id"
  FROM "B"
  LEFT JOIN "B2C" ON "B"."b" = "B2C"."b"
  WHERE "B"."b" = {sql_param}
)
ORDER BY "B2C"._order ASC
LIMIT 100"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "B2C"
WHERE "_id" IN (
  SELECT
    "B2C"."_id"
  FROM "B"
  LEFT JOIN "B2C" ON "B"."b" = "B2C"."b"
  WHERE "B"."b" = {sql_param}
)"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();

        // Filter the D table by one of the columns from B.
        let url = "http://example.com/combined/D?B.b=eq.i";
        let query_params = from_value(json!({"B.b": "eq.i"})).unwrap();
        let inner = block_on(Select::from_path_and_query("D", &query_params, &rltbl));
        let select = block_on(joined_query(&rltbl, "combined", &inner)).unwrap();
        assert_eq!(url, inner.to_url(&base, &Format::Default).unwrap());
        let (sql, params) = select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "D"
WHERE "_id" IN (
  SELECT
    "D"."_id"
  FROM "B"
  LEFT JOIN "D" ON "B"."b" = "D"."b"
  WHERE "B"."b" = {sql_param}
)
ORDER BY "D"._order ASC
LIMIT 100"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();
        let (sql, params) = select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "D"
WHERE "_id" IN (
  SELECT
    "D"."_id"
  FROM "B"
  LEFT JOIN "D" ON "B"."b" = "D"."b"
  WHERE "B"."b" = {sql_param}
)"#
            )
        );
        assert_eq!(params, vec![json!("i")]);
        block_on(rltbl.connection.query(&sql, Some(&json!(params)))).unwrap();

        // Filter the C table by one of the columns from B,
        // This should cause joined_query() to return an error.
        // let url = "http://example.com/combined/C?B.b=eq.i";
        let query_params = from_value(json!({"B.b": "eq.i"})).unwrap();
        let inner = block_on(Select::from_path_and_query("C", &query_params, &rltbl));
        let select = block_on(joined_query(&rltbl, "combined", &inner));
        assert_eq!(select.is_err(), true);
    }
}
