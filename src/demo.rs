use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use serde_json::json;

use crate::{
    core::{Relatable, NEW_ORDER_MULTIPLIER},
    sql::{self, CachingStrategy, DbKind, JsonRow, SqlParam},
};

use anyhow::Result;
use rltbl_db::core::DbQuery;
use serde_json::Value as JsonValue;

/// Build a demonstration database. Based on <https://github.com/allisonhorst/palmerpenguins>.
pub async fn build_demo(rltbl: &Relatable, force: &bool, size: usize) -> Result<()> {
    create_demo_column_table(rltbl, force).await?;
    create_demo_datatype_table(rltbl, force).await?;
    create_penguin_table(rltbl, None, force, size).await?;
    create_island_table(rltbl, None, force).await?;
    Ok(())
}

/// Create a demonstration table similar to the penguin table, but with the given name,
/// and add `size` rows of data to it. Drop the table first if `force` is set.
pub async fn create_penguin_table(
    rltbl: &Relatable,
    table: Option<&str>,
    force: &bool,
    size: usize,
) -> Result<()> {
    tracing::trace!("create_penguin_table({rltbl:?}, {table:?}, {force}, {size})");
    let table = match table {
        Some(table) => table,
        None => "penguin",
    };
    if *force {
        let sql = match rltbl.connection.kind() {
            DbKind::Postgres => format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#),
            DbKind::Sqlite => format!(r#"DROP TABLE IF EXISTS "{table}""#),
        };
        rltbl.pool.execute(&sql, &[]).await?;
    }

    let sql = format!(r#"INSERT INTO "table" ("table", "path") VALUES ('{table}', '{table}.tsv')"#);
    rltbl.pool.execute(&sql, &[]).await?;

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    // Create the demo table:
    let sql = format!(
        r#"CREATE TABLE "{table}" (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             study_name TEXT,
             sample_number INTEGER,
             species TEXT,
             island TEXT,
             individual_id TEXT,
             bill_length REAL,
             bill_depth NUMERIC,
             body_mass BIGINT
           )"#,
    );
    rltbl.pool.execute(&sql, &[]).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, table, &rltbl.connection.kind());
    if let CachingStrategy::Trigger = rltbl.caching_strategy {
        sql::add_caching_trigger_ddl(&mut ddl, table, &rltbl.connection.kind());
    }
    for sql in ddl {
        rltbl.pool.execute(&sql, &[]).await?;
    }
    // Populate the demo table with random data.
    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);
    let sql_first_part = format!(r#"INSERT INTO "{table}" VALUES "#);
    let mut sql_value_parts = vec![];
    let mut sql_param = SqlParam::new(&rltbl.connection.kind());
    let mut param_values = vec![];
    let max_params = match rltbl.connection.kind() {
        DbKind::Sqlite => sql::MAX_PARAMS_SQLITE,
        DbKind::Postgres => sql::MAX_PARAMS_POSTGRES,
    };
    for i in 0..size {
        if (param_values.len() + 8) >= max_params {
            let sql = format!(
                "{sql_first_part} {sql_value_part}",
                sql_value_part = sql_value_parts.join(", ")
            );
            rltbl.pool.execute(&sql, &param_values).await?;
            tracing::info!(
                "{num_rows} rows loaded to table '{table}'",
                num_rows = i - 1
            );
            param_values.clear();
            sql_value_parts.clear();
            sql_param.reset();
        }

        let id = i + 1;
        let order = id * NEW_ORDER_MULTIPLIER;
        let island = islands.iter().choose(&mut rng);
        let bill_length = rng.gen_range(300..500) as f64 / 10.0;
        let bill_depth = rng.gen_range(200..400) as f64 / 10.0;
        let body_mass = rng.gen_range(1000..5000);
        sql_value_parts.push(format!(
            "({sql_param_list_1}, 'FAKE123', {lone_sql_param}, 'Pygoscelis adeliae', \
                 {sql_param_list_2})",
            sql_param_list_1 = sql_param.get_as_list(2),
            lone_sql_param = sql_param.next(),
            sql_param_list_2 = sql_param.get_as_list(5),
        ));
        param_values.push(json!(id));
        param_values.push(json!(order));
        param_values.push(json!(id));
        param_values.push(json!(island));
        param_values.push(json!(format!("N{}A{}", (i / 2) + 1, (i % 2) + 1)));
        param_values.push(json!(bill_length));
        param_values.push(json!(bill_depth));
        param_values.push(json!(body_mass));
    }
    if param_values.len() > 0 {
        let sql = format!(
            "{sql_first_part} {sql_value_part}",
            sql_value_part = sql_value_parts.join(", ")
        );
        rltbl.pool.execute(&sql, &param_values).await?;
    }

    Ok(())
}

/// Create a demonstration table similar to the island table, but with the given name,
/// and add `size` rows of data to it. Drop the table first if `force` is set.
pub async fn create_island_table(
    rltbl: &Relatable,
    table: Option<&str>,
    force: &bool,
) -> Result<()> {
    tracing::trace!("create_island_table({rltbl:?}, {table:?}, {force})");
    let table = match table {
        Some(table) => table,
        None => "island",
    };
    if *force {
        if let DbKind::Postgres = rltbl.connection.kind() {
            rltbl
                .pool
                .execute(&format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#), &[])
                .await?;
        }
    }

    let sql = format!(r#"INSERT INTO "table" ("table", "path") VALUES ('{table}', '{table}.tsv')"#);
    rltbl.pool.execute(&sql, &[]).await?;

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    // Create the demo table:
    let sql = format!(
        r#"CREATE TABLE "{table}" (
                 _id {pkey_clause},
                 _order INTEGER UNIQUE,
                 island_id INTEGER,
                 island TEXT
               )"#,
    );
    rltbl.pool.query(&sql, &[]).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, table, &rltbl.connection.kind());
    if let CachingStrategy::Trigger = rltbl.caching_strategy {
        sql::add_caching_trigger_ddl(&mut ddl, table, &rltbl.connection.kind());
    }
    for sql in ddl {
        rltbl.pool.execute(&sql, &[]).await?;
    }

    let sql = format!(
        r#"INSERT INTO "{table}" ("island_id", "island")
               VALUES (1, 'Torgersen'), (2, 'Biscoe'), (3, 'Dream')"#
    );

    rltbl.pool.execute(&sql, &[]).await?;
    Ok(())
}

/// Create the datatype table for the demonstration database
pub async fn create_demo_datatype_table(rltbl: &Relatable, force: &bool) -> Result<()> {
    tracing::trace!("create_demo_datatype_table({rltbl:?}, {force})");
    if *force {
        if let DbKind::Postgres = rltbl.connection.kind() {
            rltbl
                .pool
                .execute(r#"DROP TABLE IF EXISTS "datatype" CASCADE"#, &[])
                .await?;
        }
    }

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    let sql = format!(
        r#"CREATE TABLE "datatype" (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             "datatype" TEXT,
             "description" TEXT,
             "parent" TEXT,
             "condition" TEXT,
             "sql_type" TEXT,
             "format" TEXT
           )"#,
    );
    rltbl.pool.execute(&sql, &[]).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, "datatype", &rltbl.connection.kind());
    for sql in ddl {
        rltbl.pool.execute(&sql, &[]).await?;
    }

    let datatype_contents = [
        json!({
            "datatype": "decimal",
            "description": "A decimal number",
            "parent": "",
            "condition": r"match(-?\d+(\.\d+)?)",
            "sql_type": "NUMERIC",
            "format": "%.1f"
        }),
        json!({
            "datatype": "study_name",
            "description": "",
            "parent": "text",
            "condition": "in(FAKE123, FAKE456)",
            "sql_type": "",
            "format": ""
        }),
    ]
    .iter()
    .map(|content| JsonRow {
        content: content.as_object().expect("Not a map").clone(),
    })
    .collect::<Vec<_>>();

    let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
    let mut param_values = vec![];
    let mut get_param = |row: &JsonRow, cname: &str| -> Result<String> {
        match row.get_value(cname)? {
            JsonValue::Null => Ok("NULL".to_string()),
            JsonValue::String(value) => {
                param_values.push(value.to_string());
                Ok(sql_param_gen.next().to_string())
            }
            _ => panic!("Invalid value type for datatype table"),
        }
    };
    let mut value_clauses = vec![];
    for row in &datatype_contents {
        let s1 = get_param(row, "datatype")?;
        let s2 = get_param(row, "description")?;
        let s3 = get_param(row, "parent")?;
        let s4 = get_param(row, "condition")?;
        let s5 = get_param(row, "sql_type")?;
        let s6 = get_param(row, "format")?;
        value_clauses.push(format!("({s1}, {s2}, {s3}, {s4}, {s5}, {s6})"));
    }

    let sql = format!(
        r#"INSERT INTO "datatype"
               ("datatype", "description", "parent", "condition", "sql_type", "format")
               VALUES {values}"#,
        values = value_clauses.join(", ")
    );
    let param_values = json!(param_values);
    rltbl.connection.query(&sql, Some(&param_values)).await?;
    Ok(())
}

/// Create the column table for the demonstration database
pub async fn create_demo_column_table(rltbl: &Relatable, force: &bool) -> Result<()> {
    tracing::trace!("create_demo_column_table({rltbl:?}, {force})");
    if *force {
        if let DbKind::Postgres = rltbl.connection.kind() {
            rltbl
                .pool
                .execute(r#"DROP TABLE IF EXISTS "column" CASCADE"#, &[])
                .await?;
        }
    }

    let pkey_clause = match rltbl.connection.kind() {
        DbKind::Sqlite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::Postgres => "SERIAL PRIMARY KEY",
    };

    let sql = format!(
        r#"CREATE TABLE "column" (
             _id {pkey_clause},
             _order INTEGER UNIQUE,
             "table" TEXT,
             "column" TEXT,
             "label" TEXT,
             "description" TEXT,
             "datatype" TEXT,
             "nulltype" TEXT,
             "structure" TEXT
           )"#,
    );
    rltbl.pool.execute(&sql, &[]).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, "column", &rltbl.connection.kind());
    for sql in ddl {
        rltbl.pool.execute(&sql, &[]).await?;
    }

    let column_contents = [
        json!({
            "table": "penguin",
            "column": "study_name",
            "label": "study name",
            "datatype": "study_name",
        }),
        json!({
            "table": "penguin",
            "column": "sample_number",
            "label": "sample number",
            "description": "a sample number",
            "datatype": "integer",
        }),
        json!({
            "table": "penguin",
            "column": "species",
            "label": "species",
            "nulltype": "empty",
        }),
        json!({
            "table": "penguin",
            "column": "island",
            "label": "island",
            "datatype": "text",
            "structure": "from(island.island)",
        }),
        json!({
            "table": "penguin",
            "column": "individual_id",
            "label": "individual id",
            "nulltype": "empty",
            "datatype": "word",
        }),
        json!({
            "table": "penguin",
            "column": "bill_length",
            "label": "bill length (mm)",
            "datatype": "decimal",
        }),
        json!({
            "table": "penguin",
            "column": "bill_depth",
            "label": "bill depth (mm)",
            "datatype": "decimal",
        }),
        json!({
            "table": "penguin",
            "column": "body_mass",
            "label": "body mass (g)",
            "nulltype": "empty",
            "datatype": "integer",
        }),
    ]
    .iter()
    .map(|content| JsonRow {
        content: content.as_object().expect("Not a map").clone(),
    })
    .collect::<Vec<_>>();

    let mut sql_param_gen = SqlParam::new(&rltbl.connection.kind());
    let mut param_values = vec![];
    let mut get_param = |row: &JsonRow, cname: &str| -> Result<String> {
        match row.get_value(cname).unwrap_or_default() {
            JsonValue::Null => Ok("NULL".to_string()),
            JsonValue::String(value) => {
                param_values.push(value.to_string());
                Ok(sql_param_gen.next().to_string())
            }
            _ => panic!("Invalid value type for column table"),
        }
    };
    let mut value_clauses = vec![];
    for row in &column_contents {
        let s1 = get_param(row, "table")?;
        let s2 = get_param(row, "column")?;
        let s3 = get_param(row, "label")?;
        let s4 = get_param(row, "description")?;
        let s5 = get_param(row, "nulltype")?;
        let s6 = get_param(row, "datatype")?;
        let s7 = get_param(row, "structure")?;
        value_clauses.push(format!("({s1}, {s2}, {s3}, {s4}, {s5}, {s6}, {s7})"));
    }

    let sql = format!(
        r#"INSERT INTO "column"
               ("table", "column", "label", "description", "nulltype", "datatype", "structure")
               VALUES {values}"#,
        values = value_clauses.join(", ")
    );
    let param_values = json!(param_values);
    rltbl.connection.query(&sql, Some(&param_values)).await?;
    Ok(())
}

/// Create a tableset for the demonstration database
pub async fn create_demo_tableset(rltbl: &Relatable, force: &bool, size: usize) -> Result<()> {
    tracing::trace!("create_demo_tableset({rltbl:?}, {force}, {size})");
    if *force {
        if let DbKind::Postgres = rltbl.connection.kind() {
            rltbl
                .connection
                .query(&format!(r#"DROP TABLE IF EXISTS "study" CASCADE"#), None)
                .await?;
            rltbl
                .connection
                .query(&format!(r#"DROP TABLE IF EXISTS "penguin" CASCADE"#), None)
                .await?;
            rltbl
                .connection
                .query(&format!(r#"DROP TABLE IF EXISTS "egg" CASCADE"#), None)
                .await?;
        }
    }

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('tableset', 'tableset.tsv')"#;
    rltbl.pool.execute(sql, &[]).await?;

    // Create the tableset table.
    let sql = r#"CREATE TABLE tableset (
              _id INTEGER PRIMARY KEY AUTOINCREMENT,
              _order INTEGER UNIQUE,
              tableset TEXT,
              left_table TEXT,
              left_column TEXT,
              right_table TEXT,
              right_column TEXT
            )"#;
    rltbl.pool.execute(sql, &[]).await?;

    let sql = r#"INSERT INTO "tableset" VALUES
              (1, 1000, 'combined', NULL, NULL, 'study', 'study_name'),
              (2, 2000, 'combined', 'study', 'study_name', 'penguin', 'individual_id'),
              (3, 3000, 'combined', 'penguin', 'individual_id', 'egg', 'egg_id')
            "#;
    rltbl.pool.execute(sql, &[]).await?;

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('study', 'study.tsv')"#;
    rltbl.pool.execute(sql, &[]).await?;

    // Create the study table.
    let sql = r#"CREATE TABLE study (
              _id INTEGER PRIMARY KEY AUTOINCREMENT,
              _order INTEGER UNIQUE,
              study_name TEXT UNIQUE,
              description TEXT
            )"#;
    rltbl.pool.execute(sql, &[]).await?;

    let sql = r#"INSERT INTO study VALUES
            (0, 0, 'FAKE123', 'Fake Study 123')"#;
    rltbl.pool.execute(sql, &[]).await?;

    create_penguin_table(rltbl, None, force, size).await?;

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('egg', 'egg.tsv')"#;
    rltbl.pool.execute(sql, &[]).await?;

    // Create the egg table.
    let sql = r#"CREATE TABLE egg (
      _id INTEGER PRIMARY KEY AUTOINCREMENT,
      _order INTEGER UNIQUE,
      egg_id TEXT UNIQUE,
      individual_id TEXT
    )"#;
    rltbl.pool.execute(sql, &[]).await?;

    let sql = r#"INSERT INTO egg VALUES
        (0, 0, 'E1', 'N1')"#;
    rltbl.pool.execute(sql, &[]).await?;

    Ok(())
}
