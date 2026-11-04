use crate::{self as rltbl};
use rltbl::{
    core::Relatable, data_table::DataTableBuilder, datatype::DatatypeBuilder, tsv_table::TsvTable,
};
use rltbl_db::values;

use anyhow::Result;
use rand::{rngs::StdRng, seq::IteratorRandom as _, Rng as _, SeedableRng as _};
use rust_decimal::Decimal;

/// Build a demonstration database. Based on <https://github.com/allisonhorst/palmerpenguins>.
pub async fn build_demo(rltbl: &Relatable, force: &bool, size: usize) -> Result<()> {
    create_demo_datatype_table(rltbl, force).await?;
    create_island_table(rltbl, *force).await?;
    create_penguin_table(rltbl, None, *force, size).await?;
    Ok(())
}

/// Create a demonstration table similar to the penguin table, but with the given name,
/// and add `size` rows of data to it. Drop the table first if `force` is set.
pub async fn create_penguin_table(
    rltbl: &Relatable,
    table: Option<&str>,
    force: bool,
    size: usize,
) -> Result<()> {
    tracing::trace!("create_penguin_table({rltbl:?}, {table:?}, {force}, {size})");
    let table = match table {
        Some(table) => table,
        None => "penguin",
    };
    let path = format!("{table}.tsv");

    let mut builder = DataTableBuilder::from_path(&path);
    builder
        .column("study_name")
        .label("study name")
        .description("the name of the study")
        .datatype("study_name");
    builder
        .column("sample_number")
        .label("sample number")
        .description("a sample number for this measurement")
        .datatype("integer");
    builder
        .column("species")
        .description("the species of this penguin")
        .nulltype("empty");
    builder
        .column("island")
        .description("the island where this penguin was studied")
        .datatype("text")
        .structure("from(island.island)");
    builder
        .column("individual_id")
        .label("individual id")
        .description("an identifier for this penguin")
        .nulltype("empty")
        .datatype("word");
    builder
        .column("bill_length")
        .label("bill length (mm)")
        .datatype("decimal");
    builder
        .column("bill_depth")
        .label("bill depth (mm)")
        .datatype("decimal");
    builder
        .column("body_mass")
        .label("body mass (g)")
        .nulltype("empty")
        .datatype("integer");

    // Populate the demo table with random data.
    let islands = vec!["Biscoe", "Dream", "Torgersen"];
    let mut rng = StdRng::seed_from_u64(0);
    let study_name = "FAKE123";
    let species = "Pygoscelis adeliae";
    for i in 0..size {
        let sample_number = i + 1;
        let island = islands.iter().choose(&mut rng).unwrap().to_string();
        let individual_id = format!("N{}A{}", (i / 2) + 1, (i % 2) + 1);
        let bill_length = rng.random_range(300..500) as f64 / 10.0;
        let bill_depth = rng.random_range(200..400) as f64 / 10.0;
        let bill_depth = Decimal::try_from(bill_depth).unwrap();
        let body_mass = rng.random_range(1000..5000);

        builder.row(values![
            study_name,
            sample_number,
            species,
            island,
            individual_id,
            bill_length as f32,
            bill_depth,
            body_mass as i64
        ])?;
    }

    builder.force(force);
    builder.create(&rltbl).await?;
    Ok(())
}

/// Create a demonstration table of islands.
/// Drop the table first if `force` is set.
pub async fn create_island_table(rltbl: &Relatable, force: bool) -> Result<()> {
    let mut builder = DataTableBuilder::from_path("island.tsv");
    builder.column("island_id").datatype("integer");
    builder.column("island");
    builder.row(values![1, "Torgersen"])?;
    builder.row(values![2, "Biscoe"])?;
    builder.row(values![3, "Dream"])?;
    builder.force(force);
    builder.create(&rltbl).await?;
    Ok(())
}

/// Create the datatype table for the demonstration database
pub async fn create_demo_datatype_table(rltbl: &Relatable, force: &bool) -> Result<()> {
    let datatype_table = rltbl.datatype_table().await?;
    if *force {
        datatype_table.drop().await?;
        datatype_table.init().await?;
    }
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

/// Create a tableset for the demonstration database
pub async fn create_demo_tableset(rltbl: &Relatable, force: &bool, size: usize) -> Result<()> {
    tracing::trace!("create_demo_tableset({rltbl:?}, {force}, {size})");
    if *force {
        if rltbl.pool.syntax().name() == "postgres" {
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

    create_penguin_table(rltbl, None, *force, size).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RelatableBuilder;
    use anyhow::Result;

    #[tokio::test]
    async fn test_island() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_demo_island")
            .init(true)
            .await?;

        create_island_table(&rltbl, false).await?;

        rltbl.drop_test().await
    }
}
