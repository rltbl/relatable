use crate as rltbl;
use rltbl::{
    column::ColumnBuilder,
    core::{Relatable, NEW_ORDER_MULTIPLIER},
    datatype::DatatypeBuilder,
    sql::{self, CachingStrategy, SqlParam},
};
use rltbl_db::{
    core::{DbKind, DbQuery},
    params,
};

use anyhow::Result;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use rust_decimal::Decimal;

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
        let sql = match rltbl.pool.kind() {
            DbKind::PostgreSQL => format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#),
            DbKind::SQLite => format!(r#"DROP TABLE IF EXISTS "{table}""#),
        };
        rltbl.pool.execute(&sql, ()).await?;
    }

    let sql = format!(r#"INSERT INTO "table" ("table", "path") VALUES ('{table}', '{table}.tsv')"#);
    rltbl.pool.execute(&sql, ()).await?;

    let pkey_clause = match rltbl.pool.kind() {
        DbKind::SQLite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::PostgreSQL => "SERIAL PRIMARY KEY",
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
    rltbl.pool.execute(&sql, ()).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, table, &rltbl.pool.kind());
    if let CachingStrategy::Trigger = rltbl.caching_strategy {
        sql::add_caching_trigger_ddl(&mut ddl, table, &rltbl.pool.kind());
    }
    for sql in ddl {
        rltbl.pool.execute(&sql, ()).await?;
    }
    // Populate the demo table with random data.
    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);
    let sql_first_part = format!(r#"INSERT INTO "{table}" VALUES "#);
    let mut sql_value_parts = vec![];
    let mut sql_param = SqlParam::new(&rltbl.pool.kind());
    let mut param_values = vec![];
    let max_params = match rltbl.pool.kind() {
        DbKind::SQLite => sql::MAX_PARAMS_SQLITE,
        DbKind::PostgreSQL => sql::MAX_PARAMS_POSTGRES,
    };
    for i in 0..size {
        if (param_values.len() + 8) >= max_params {
            let sql = format!(
                "{sql_first_part} {sql_value_part}",
                sql_value_part = sql_value_parts.join(", ")
            );
            rltbl.pool.execute(&sql, param_values).await?;
            tracing::info!(
                "{num_rows} rows loaded to table '{table}'",
                num_rows = i - 1
            );
            param_values = vec![];
            sql_value_parts.clear();
            sql_param.reset();
        }

        let id = i as i32 + 1;
        let order = id * NEW_ORDER_MULTIPLIER as i32;
        let island = islands.iter().choose(&mut rng).unwrap().to_string();
        let bill_length = rng.gen_range(300..500) as f64 / 10.0;
        let bill_depth = rng.gen_range(200..400) as f64 / 10.0;
        let body_mass = rng.gen_range(1000..5000);
        let bill_depth = Decimal::try_from(bill_depth).unwrap();
        let bill_length = bill_length as f32;
        let body_mass = body_mass as i64;

        sql_value_parts.push(format!(
            "({sql_param_list_1}, 'FAKE123', {lone_sql_param}, 'Pygoscelis adeliae', \
                 {sql_param_list_2})",
            sql_param_list_1 = sql_param.get_as_list(2),
            lone_sql_param = sql_param.next(),
            sql_param_list_2 = sql_param.get_as_list(5),
        ));
        param_values.extend(params![
            id,
            order,
            id,
            island,
            format!("N{}A{}", (i / 2) + 1, (i % 2) + 1),
            bill_length,
            bill_depth,
            body_mass
        ]);
    }
    if param_values.len() > 0 {
        let sql = format!(
            "{sql_first_part} {sql_value_part}",
            sql_value_part = sql_value_parts.join(", ")
        );
        rltbl.pool.execute(&sql, param_values).await?;
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
        if let DbKind::PostgreSQL = rltbl.pool.kind() {
            rltbl
                .pool
                .execute(&format!(r#"DROP TABLE IF EXISTS "{table}" CASCADE"#), ())
                .await?;
        }
    }

    let sql = format!(r#"INSERT INTO "table" ("table", "path") VALUES ('{table}', '{table}.tsv')"#);
    rltbl.pool.execute(&sql, ()).await?;

    let pkey_clause = match rltbl.pool.kind() {
        DbKind::SQLite => "INTEGER PRIMARY KEY AUTOINCREMENT",
        DbKind::PostgreSQL => "SERIAL PRIMARY KEY",
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
    rltbl.pool.query(&sql, ()).await?;

    let mut ddl = vec![];
    sql::add_metacolumn_trigger_ddl(&mut ddl, table, &rltbl.pool.kind());
    if let CachingStrategy::Trigger = rltbl.caching_strategy {
        sql::add_caching_trigger_ddl(&mut ddl, table, &rltbl.pool.kind());
    }
    for sql in ddl {
        rltbl.pool.execute(&sql, ()).await?;
    }

    let sql = format!(
        r#"INSERT INTO "{table}" ("island_id", "island")
               VALUES (1, 'Torgersen'), (2, 'Biscoe'), (3, 'Dream')"#
    );

    rltbl.pool.execute(&sql, ()).await?;
    Ok(())
}

/// Create the datatype table for the demonstration database
pub async fn create_demo_datatype_table(rltbl: &Relatable, force: &bool) -> Result<()> {
    let datatype_table = rltbl.datatype_table();
    if *force {
        datatype_table.drop().await?;
    }
    datatype_table.create().await?;
    datatype_table
        .add(&[
            &DatatypeBuilder::new("decimal")
                .parent("nonspace")
                .description("a decimal number")
                .condition(r"match(-?\d+(\.\d+)?)")
                .sql_type("NUMERIC")
                .format("%.1f")
                .build()?,
            &DatatypeBuilder::new("study_name")
                .parent("word")
                .description("the name of this study")
                .condition(r"in(FAKE123, FAKE456)")
                .build()?,
        ])
        .await?;
    Ok(())
}

/// Create the column table for the demonstration database
pub async fn create_demo_column_table(rltbl: &Relatable, force: &bool) -> Result<()> {
    let column_table = rltbl.column_table();
    if *force {
        column_table.drop().await?;
    }
    column_table.create().await?;
    column_table
        .add(&[
            &ColumnBuilder::new("penguin", "study_name")
                .label("study name")
                .description("the name of the study")
                .datatype("study_name")
                .build()?,
            &ColumnBuilder::new("penguin", "sample_number")
                .label("sample number")
                .description("a sample number for this measurement")
                .datatype("integer")
                .build()?,
            &ColumnBuilder::new("penguin", "species")
                .description("the species of this penguin")
                .nulltype("empty")
                .build()?,
            &ColumnBuilder::new("penguin", "island")
                .description("the island where this penguin was studied")
                .datatype("text")
                .structure("from(island.island)")
                .build()?,
            &ColumnBuilder::new("penguin", "individual_id")
                .label("individual id")
                .description("an identifier for this penguin")
                .nulltype("empty")
                .datatype("word")
                .build()?,
            &ColumnBuilder::new("penguin", "bill_length")
                .label("bill length (mm)")
                .datatype("decimal")
                .build()?,
            &ColumnBuilder::new("penguin", "bill_depth")
                .label("bill depth (mm)")
                .datatype("decimal")
                .build()?,
            &ColumnBuilder::new("penguin", "body_mass")
                .label("body mass (g)")
                .nulltype("empty")
                .datatype("integer")
                .build()?,
        ])
        .await?;
    Ok(())
}

/// Create a tableset for the demonstration database
pub async fn create_demo_tableset(rltbl: &Relatable, force: &bool, size: usize) -> Result<()> {
    tracing::trace!("create_demo_tableset({rltbl:?}, {force}, {size})");
    if *force {
        if let DbKind::PostgreSQL = rltbl.pool.kind() {
            rltbl
                .pool
                .execute(&format!(r#"DROP TABLE IF EXISTS "study" CASCADE"#), ())
                .await?;
            rltbl
                .pool
                .execute(&format!(r#"DROP TABLE IF EXISTS "penguin" CASCADE"#), ())
                .await?;
            rltbl
                .pool
                .execute(&format!(r#"DROP TABLE IF EXISTS "egg" CASCADE"#), ())
                .await?;
        }
    }

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('tableset', 'tableset.tsv')"#;
    rltbl.pool.execute(sql, ()).await?;

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
    rltbl.pool.execute(sql, ()).await?;

    let sql = r#"INSERT INTO "tableset" VALUES
              (1, 1000, 'combined', NULL, NULL, 'study', 'study_name'),
              (2, 2000, 'combined', 'study', 'study_name', 'penguin', 'individual_id'),
              (3, 3000, 'combined', 'penguin', 'individual_id', 'egg', 'egg_id')
            "#;
    rltbl.pool.execute(sql, ()).await?;

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('study', 'study.tsv')"#;
    rltbl.pool.execute(sql, ()).await?;

    // Create the study table.
    let sql = r#"CREATE TABLE study (
              _id INTEGER PRIMARY KEY AUTOINCREMENT,
              _order INTEGER UNIQUE,
              study_name TEXT UNIQUE,
              description TEXT
            )"#;
    rltbl.pool.execute(sql, ()).await?;

    let sql = r#"INSERT INTO study VALUES
            (0, 0, 'FAKE123', 'Fake Study 123')"#;
    rltbl.pool.execute(sql, ()).await?;

    create_penguin_table(rltbl, None, force, size).await?;

    let sql = r#"INSERT INTO "table" ('table', 'path') VALUES ('egg', 'egg.tsv')"#;
    rltbl.pool.execute(sql, ()).await?;

    // Create the egg table.
    let sql = r#"CREATE TABLE egg (
      _id INTEGER PRIMARY KEY AUTOINCREMENT,
      _order INTEGER UNIQUE,
      egg_id TEXT UNIQUE,
      individual_id TEXT
    )"#;
    rltbl.pool.execute(sql, ()).await?;

    let sql = r#"INSERT INTO egg VALUES
        (0, 0, 'E1', 'N1')"#;
    rltbl.pool.execute(sql, ()).await?;

    Ok(())
}
