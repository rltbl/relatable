use crate::core::{Relatable, RelatableError, DEFAULT_LIMIT};
use crate::sql::{self, DbKind, SqlParam};
use anyhow::Result;
use enquote::unquote;
use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, to_value, Value as JsonValue};

/// Represents a SELECT statement.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

impl Default for Select {
    fn default() -> Self {
        let table_name = "";
        Self {
            // By default, the view name and table name are the same.
            table_name: table_name.to_string(),
            view_name: table_name.to_string(),
            select: Vec::default(),
            joins: Vec::default(),
            limit: usize::default(),
            offset: usize::default(),
            filters: Vec::default(),
            order_by: Vec::default(),
        }
    }
}

impl Select {
    pub fn from(table_name: &str) -> Self {
        tracing::trace!("Select::from({table_name:?})");
        Self {
            table_name: table_name.to_string(),
            limit: DEFAULT_LIMIT,
            ..Default::default()
        }
    }

    /// Construct a [Select] for the given [relatable](crate) instance from the given path and
    /// query parameters.
    pub fn from_path_and_query(path: &str, query_params: &QueryParams) -> Self {
        tracing::trace!("Select::from_path_and_query({path:?}, {query_params:?})");
        let mut query_params = query_params.clone();
        let mut filters = Vec::new();
        let mut order_by = Vec::new();

        let mut select = vec![];
        if let Some(s) = query_params.get("select") {
            select.push(SelectField::Column {
                table: String::new(),
                column: s.to_string(),
                alias: String::new(),
            });
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

        for (lhs, pattern) in query_params {
            let (table, column) = match lhs.split_once(".") {
                Some((table, column)) => (table.to_string(), column.to_string()),
                None => (String::new(), lhs),
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
            } else if pattern.starts_with("eq.") {
                let value = &pattern.replace("eq.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::Equal {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::Equal {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("not_eq.") {
                let value = &pattern.replace("not_eq.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::NotEqual {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::NotEqual {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("gt.") {
                let value = &pattern.replace("gt.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::GreaterThan {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::GreaterThan {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("gte.") {
                let value = &pattern.replace("gte.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::GreaterThanOrEqual {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::GreaterThanOrEqual {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("lt.") {
                let value = &pattern.replace("lt.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::LessThan {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::LessThan {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("lte.") {
                let value = &pattern.replace("lte.", "");
                match serde_json::from_str(value) {
                    Ok(value) => filters.push(Filter::LessThanOrEqual {
                        table,
                        column,
                        value,
                    }),
                    Err(_) => filters.push(Filter::LessThanOrEqual {
                        table,
                        column,
                        value: JsonValue::String(value.to_string()),
                    }),
                }
            } else if pattern.starts_with("is.") {
                let value = pattern.replace("is.", "");
                match value.to_lowercase().as_str() {
                    "null" => filters.push(Filter::Is {
                        table,
                        column,
                        value: JsonValue::Null,
                    }),
                    _ => match serde_json::from_str(&value) {
                        Ok(value) => filters.push(Filter::Is {
                            table,
                            column,
                            value,
                        }),
                        Err(_) => tracing::warn!("invalid 'is' filter value {pattern}"),
                    },
                };
            } else if pattern.starts_with("is_not.") {
                let value = pattern.replace("is_not.", "");
                match value.to_lowercase().as_str() {
                    "null" => filters.push(Filter::IsNot {
                        table,
                        column,
                        value: JsonValue::Null,
                    }),
                    _ => match serde_json::from_str(&value) {
                        Ok(value) => filters.push(Filter::IsNot {
                            table,
                            column,
                            value,
                        }),
                        Err(_) => tracing::warn!("invalid 'is_not' filter value {pattern}"),
                    },
                };
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
                    .map(|v| serde_json::from_str::<JsonValue>(v).unwrap_or(json!(v.to_string())))
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
                    .map(|v| serde_json::from_str::<JsonValue>(v).unwrap_or(json!(v.to_string())))
                    .collect::<Vec<_>>();
                filters.push(Filter::NotIn {
                    table,
                    column,
                    value: json!(values),
                })
            }
        }

        let table_name = path.split(".").next().unwrap_or_default();
        Self {
            table_name: table_name.to_string(),
            select,
            limit,
            offset,
            order_by,
            filters,
            ..Default::default()
        }
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
        for column in rltbl.get_db_table_columns(&table).await? {
            self.select.push(SelectField::Column {
                table: String::new(),
                column: column.get_string("name")?,
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
    pub fn order_by(mut self, column: &str) -> Self {
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

        // Closure used for text types:
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
            tracing::trace!("Applying filter: {filter}");
            if like.is_match(&filter) {
                let captures = like.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::Like {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if eq.is_match(&filter) {
                let captures = eq.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::Equal {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if not_eq.is_match(&filter) {
                let captures = not_eq.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::NotEqual {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if gt.is_match(&filter) {
                let captures = gt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::GreaterThan {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if gte.is_match(&filter) {
                let captures = gte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(value)?;
                self.filters.push(Filter::GreaterThanOrEqual {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if lt.is_match(&filter) {
                let captures = lt.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
                self.filters.push(Filter::LessThan {
                    table: "".to_string(),
                    column,
                    value,
                });
            } else if lte.is_match(&filter) {
                let captures = lte.captures(&filter).unwrap();
                let column = captures.get(1).unwrap().as_str().to_string();
                let value = &captures.get(2).unwrap().as_str();
                let value = maybe_quote_value(&value)?;
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
                    _ => maybe_quote_value(&value)?,
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
                    _ => maybe_quote_value(&value)?,
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

    /// Add an in filter on the given column and value.
    pub fn is_in_subquery(&mut self, column: &str, subquery: &Select) -> &Self {
        tracing::trace!("Select::is_in_subquery({column:?}, {subquery:?})");
        self.filters.push(Filter::InSubquery {
            table: "".to_string(),
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
                self.table_name
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
                }
            }
        } else {
            lines.push("SELECT".to_string());
            for filter in &self.filters {
                let (_, c, _, _) = filter.parts();
                if c == "_change_id" {
                    lines.push(get_change_sql(&mut sql_param_gen));
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

                // Replace the table with the view name in the SelectField if necessary:
                let mut field = field.clone();
                if let SelectField::Column { ref mut table, .. } = field {
                    *table = target.to_string();
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
        let mut lines = Vec::new();
        let mut params = Vec::new();
        lines.push(r#"SELECT COUNT(1) AS "count""#.to_string());
        lines.push(format!(r#"FROM "{}""#, self.table_name));
        for join in self.joins.clone() {
            lines.push(join.to_sql());
        }
        for (i, filter) in self.filters.iter().enumerate() {
            let keyword = if i == 0 { "WHERE" } else { "  AND" };
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
                    SelectField::Expression { .. } => {
                        return Err(RelatableError::InputError(
                            "Expressions are not supported as input to to_params()".to_string(),
                        )
                        .into())
                    }
                };
            }
            params.insert("select".to_string(), select_cols.join(",").into());
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
                params.insert(lhs, format!("{}", filter.to_url()?).into());
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
                format!(r#"{expression} AS "{alias}""#)
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
            _ => todo!("Select Expressions are not supported"),
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
    pub fn get_table(&self) -> String {
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
            | Filter::NotInSubquery { table, .. } => table.to_string(),
        }
    }

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

    pub fn get_column(&self) -> String {
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
            | Filter::NotInSubquery { column, .. } => column.to_string(),
        }
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

    pub fn to_url(&self) -> Result<String> {
        tracing::trace!("Filter::to_url()");
        fn handle_string_value(token: &str) -> String {
            let reserved = vec![':', ',', '.', '(', ')'];
            if token.chars().all(char::is_numeric) || reserved.iter().any(|&c| token.contains(c)) {
                format!("\"{}\"", token)
            } else {
                token.to_string()
            }
        }

        let (_, _, operator, value) = self.parts();

        let rhs = match &value {
            JsonValue::String(s) => handle_string_value(&s),
            JsonValue::Number(n) => format!("{}", n),
            JsonValue::Array(v) => {
                let mut list = vec![];
                for item in v {
                    match item {
                        JsonValue::String(s) => {
                            list.push(handle_string_value(&s));
                        }
                        JsonValue::Number(n) => list.push(n.to_string()),
                        _ => {
                            return Err(RelatableError::DataError(format!(
                                "Not all list items in {:?} are strings or numbers.",
                                v
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
        let as_string = |value: &JsonValue| -> String {
            match value {
                JsonValue::Null => "NULL".to_string(),
                JsonValue::Bool(value) => value.to_string(),
                JsonValue::Number(value) => value.to_string(),
                JsonValue::String(value) => value.to_string(),
                JsonValue::Array(value) => format!("{value:?}"),
                JsonValue::Object(value) => format!("{value:?}"),
            }
        };

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
                let value = as_string(&value);
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
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} = {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::NotEqual {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} <> {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::GreaterThan {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} > {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::GreaterThanOrEqual {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} >= {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::LessThan {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} < {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::LessThanOrEqual {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} <= {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::Is {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} {is} {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        is = sql::is_clause(&sql_param.kind),
                        sql_param = sql_param.next()
                    ),
                    vec![json!(value)],
                ))
            }
            Filter::IsNot {
                table,
                column,
                value,
            } => {
                let value = as_string(&value);
                Ok((
                    format!(
                        r#"{lhs} {is_not} {sql_param}"#,
                        lhs = generate_lhs(table, column),
                        sql_param = sql_param.next(),
                        is_not = sql::is_not_clause(&sql_param.kind)
                    ),
                    vec![json!(value)],
                ))
            }
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
    Json,
    PrettyJson,
    Default,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO: This should be factored out.
        let result = match self {
            Format::Html => ".html",
            Format::Json => ".json",
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

pub fn render_values(
    options: &Vec<JsonValue>,
    sql_param_gen: &mut SqlParam,
) -> Result<(String, Vec<JsonValue>)> {
    let mut sql_params = vec![];
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
                sql_params.push(sql_param_gen.next());
                let value = unquote(s).unwrap_or(s.clone());
                values.push(format!("{value}").into())
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
                sql_params.push(sql_param_gen.next());
                values.push(format!("{n}").into())
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

// Tests

#[cfg(test)]
mod tests {
    use crate::sql::CachingStrategy;
    use async_std::task::block_on;

    use super::*;

    #[test]
    fn test_select_from_path_and_query() {
        let rltbl = block_on(Relatable::init(
            &true,
            Some("build/test_select_from_path_and_query.db"),
            &CachingStrategy::Trigger,
        ))
        .unwrap();
        let sql_param = SqlParam::new(&rltbl.connection.kind()).next();

        fn test(
            rltbl: &Relatable,
            expected_url: &str,
            table: &str,
            format: &Format,
            query_params: &JsonValue,
            expected_sql: &str,
            expected_sql_count: &str,
            expected_params: Vec<JsonValue>,
        ) {
            let base = "http://example.com";

            let query_params = serde_json::from_value(query_params.clone()).unwrap();
            let select = Select::from_path_and_query(&table, &query_params);
            println!("SELECT {select:?}");

            let actual_url = select.to_url(&base, &format).unwrap();
            assert_eq!(actual_url, expected_url);

            let (actual_sql, actual_params) = select.to_sql(&rltbl.connection.kind()).unwrap();
            assert_eq!(actual_sql, expected_sql);
            assert_eq!(actual_params, expected_params);

            let (actual_sql_count, actual_params) =
                select.to_sql_count(&rltbl.connection.kind()).unwrap();
            assert_eq!(actual_sql_count, expected_sql_count);
            assert_eq!(actual_params, expected_params);
        }

        test(
            &rltbl,
            "http://example.com/penguin",
            "penguin",
            &Format::Default,
            &json!({}),
            &format!(
                r#"SELECT *
FROM "penguin"
ORDER BY "penguin"._order ASC
LIMIT 100"#
            ),
            &format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin""#
            ),
            vec![],
        );

        test(
            &rltbl,
            "http://example.com/penguin.json?sample_number=eq.5&limit=1&offset=2",
            "penguin",
            &Format::Json,
            &json!({
               "sample_number": "eq.5",
               "limit": "1",
               "offset": "2",
            }),
            &format!(
                r#"SELECT *
FROM "penguin"
WHERE "sample_number" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1
OFFSET 2"#
            ),
            &format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "sample_number" = {sql_param}"#
            ),
            vec![json!("5")],
        );

        test(
            &rltbl,
            "http://example.com/penguin?foo.bar=eq.5&limit=1",
            "penguin",
            &Format::Default,
            &json!({
               "foo.bar": "eq.5",
               "limit": "1",
            }),
            &format!(
                r#"SELECT *
FROM "penguin"
WHERE "foo"."bar" = {sql_param}
ORDER BY "penguin"._order ASC
LIMIT 1"#
            ),
            &format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "foo"."bar" = {sql_param}"#
            ),
            vec![json!("5")],
        );
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
    sample_number TEXT,
    species TEXT,
    island TEXT,
    individual_id TEXT,
    culmen_length TEXT,
    body_mass TEXT
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
  "penguin_test"."study_name",
  "penguin_test"."body_mass"
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
  "penguin_test"."_id",
  "penguin_test"."_order",
  "penguin_test"."study_name",
  "penguin_test"."sample_number",
  "penguin_test"."species",
  "penguin_test"."island",
  "penguin_test"."individual_id",
  "penguin_test"."culmen_length",
  "penguin_test"."body_mass"
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

        let mut inner_select = Select::from("penguin").limit(&0);
        inner_select.select_table_column("penguin", "individual_id");
        inner_select.left_join("penguin", "individual_id", "egg", "individual_id");
        inner_select.eq("individual_id", &"N1").unwrap();
        let mut outer_select = Select::from("penguin").limit(&0);
        outer_select.is_in_subquery("individual_id", &inner_select);

        let (sql, params) = outer_select.to_sql(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT *
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "individual_id" = {sql_param}
)
ORDER BY "penguin"._order ASC"#
            )
        );
        assert_eq!(format!("{params:?}"), r#"[String("N1")]"#);

        let (sql, params) = outer_select.to_sql_count(&rltbl.connection.kind()).unwrap();
        assert_eq!(
            sql,
            format!(
                r#"SELECT COUNT(1) AS "count"
FROM "penguin"
WHERE "individual_id" IN (
  SELECT
    "penguin"."individual_id"
  FROM "penguin"
  LEFT JOIN "egg" ON "penguin"."individual_id" = "egg"."individual_id"
  WHERE "individual_id" = {sql_param}
)"#
            )
        );
        assert_eq!(format!("{params:?}"), r#"[String("N1")]"#);
    }
}
