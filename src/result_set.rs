use crate as rltbl;
use rltbl::{column::Column, datatype::Datatype, row::Row, select::Select, table::Table};
use rltbl_db::db_value::DbValue;

use colored::Colorize;
use csv::{QuoteStyle, Writer, WriterBuilder};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sprintf::sprintf;
use std::io::Write;
use tabwriter::TabWriter;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub count: usize,
    pub total: u64,
    pub start: u64,
    pub end: u64,
}

impl std::fmt::Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rows {}-{} of {}", self.start, self.end, self.total)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultSet {
    pub select: Select,
    pub statement: String,
    pub parameters: Vec<DbValue>,
    pub range: Range,
    pub table: Table,
    /// The columns (and only the columns) used in the Select statement
    pub columns: Vec<Column>,
    /// The datatypes used in the Select statement
    pub datatypes: Vec<Datatype>,
    pub rows: Vec<Row>,
}

impl ResultSet {
    /// Write the result set to CSV
    pub fn to_csv(&self) -> String {
        let writer = WriterBuilder::new().from_writer(vec![]);
        self.to_xsv(writer)
    }

    /// Write the result set to TSV
    pub fn to_tsv(&self) -> String {
        let writer = WriterBuilder::new()
            .delimiter(b'\t')
            .quote_style(QuoteStyle::Never)
            .from_writer(vec![]);
        self.to_xsv(writer)
    }

    /// Write the result set to XSV
    pub fn to_xsv(&self, mut writer: Writer<Vec<u8>>) -> String {
        let header_row = &self
            .columns
            .iter()
            .map(|c| c.column.clone())
            .collect::<Vec<String>>();
        writer.write_record(header_row.clone()).unwrap();
        for row in &self.rows {
            writer.write_record(row.to_strings()).unwrap();
        }
        String::from_utf8(writer.into_inner().unwrap()).unwrap()
    }

    /// Uses the given (unverified) printf-style format string and the given compiled regular
    /// expression (which is used to verify the given format) to format the given cell.
    fn format_cell_text_value(column_format: &str, format_regex: &Regex, cell: &str) -> String {
        // If the cell is an empty string, just return it as is:
        if cell == "" {
            return "".to_string();
        }

        let conversion_spec = match format_regex.captures(column_format) {
            Some(c) => c[1].to_lowercase(),
            None => {
                tracing::warn!("Illegal format: '{}'", column_format);
                "s".to_string()
            }
        };
        let generic_error = format!("Error applying format '{}' to '{}'", column_format, cell);
        match conversion_spec.as_str() {
            "d" | "i" | "c" => match cell.parse::<isize>() {
                Ok(cell) => match sprintf!(&column_format, cell) {
                    Ok(cell) => {
                        // For some reason sprintf converts signed ints to unsigned ints before
                        // converting them to a string. So we have to workaround this here:
                        let cell = cell.parse::<usize>().unwrap();
                        let cell = cell as isize;
                        cell.to_string()
                    }
                    Err(e) => {
                        tracing::warn!("{}: {}", generic_error, e);
                        cell.to_string()
                    }
                },
                Err(e) => {
                    tracing::warn!("{}: {}", generic_error, e);
                    cell.to_string()
                }
            },
            "o" | "u" | "x" => match cell.parse::<usize>() {
                Ok(cell) => sprintf!(&column_format, cell).unwrap_or(cell.to_string()),
                Err(e) => {
                    tracing::warn!("{}: {}", generic_error, e);
                    cell.to_string()
                }
            },
            "e" | "f" | "g" | "a" => match cell.parse::<f64>() {
                Ok(cell) => sprintf!(&column_format, cell).unwrap_or(cell.to_string()),
                Err(e) => {
                    tracing::warn!("{}: {}", generic_error, e);
                    cell.to_string()
                }
            },
            "s" => sprintf!(&column_format, cell).unwrap_or(cell.to_string()),
            _ => {
                tracing::warn!(
                    "Unsupported conversion specifier '{}' in column format '{}'",
                    conversion_spec,
                    column_format
                );
                cell.to_string()
            }
        }
    }

    /// Write the result set to the console
    pub fn to_console(&self) -> String {
        let tw = TabWriter::new(vec![]);
        let mut tw = tw.ansi(true);
        tw.write(format!("{}\n", self.range).as_bytes())
            .unwrap_or_default();
        let header = &self
            .columns
            .iter()
            .map(|c| c.column.clone())
            .collect::<Vec<String>>();
        tw.write(format!("{}\n", header.join("\t")).as_bytes())
            .unwrap_or_default();

        let format_regex = Regex::new(r#"^%.*([\w%])$"#).expect("Invalid regular expression");
        let mut contains_errors = false;
        for row in &self.rows {
            let cells = row
                .cells
                .iter()
                .map(|(column_name, cell)| {
                    let value_to_print = {
                        let column = self
                            .columns
                            .iter()
                            .filter(|col| &col.column == column_name)
                            .nth(0)
                            .unwrap();
                        let datatype = self
                            .datatypes
                            .iter()
                            .filter(|dt| dt.datatype == column.datatype)
                            .nth(0);
                        let column_format = match datatype {
                            Some(datatype) => match datatype.format.as_str() {
                                "" => "%s",
                                value => value,
                            },
                            None => "%s",
                        };
                        ResultSet::format_cell_text_value(&column_format, &format_regex, &cell.text)
                    };
                    if cell.message_level() >= 2 {
                        contains_errors = true;
                        format!("{}", value_to_print.red())
                    } else {
                        value_to_print
                    }
                })
                .collect::<Vec<_>>();
            tw.write(format!("{}\n", cells.join("\t")).as_bytes())
                .unwrap_or_default();
        }
        tw.flush().expect("TabWriter to flush");
        let written = String::from_utf8(tw.into_inner().unwrap()).unwrap();
        written
    }
}

impl std::fmt::Display for ResultSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut tw = TabWriter::new(vec![]);
        tw.write(format!("{}\n", self.range).as_bytes())
            .unwrap_or_default();
        let header = &self
            .columns
            .iter()
            .map(|c| c.column.clone())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Relatable;
    use anyhow::Result;
    use serde_json::{from_value, json};

    #[tokio::test]
    async fn test_result_set() -> Result<()> {
        let rltbl = Relatable::test("test_result_set", false).await?;
        crate::demo::build_demo(&rltbl, &true, 10).await.unwrap();

        // A basic URL
        let query_params = from_value(json!({})).unwrap();
        let select = Select::from_path_and_query("penguin", &query_params, &rltbl)
            .await
            .limit(&1)
            .offset(&9);

        let result_set = rltbl.fetch(&select).await?;
        let expected = r"Rows 10-10 of 10
study_name  sample_number  species             island     individual_id  bill_length  bill_depth  body_mass
FAKE123     10             Pygoscelis adeliae  Torgersen  N5A2           34.5         27.9        3237
";
        assert_eq!(result_set.to_console(), expected);

        rltbl.drop_test().await
    }
}
