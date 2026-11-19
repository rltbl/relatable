// # Model
// VaccineType
// SpecimenType
// AssayType
// TargetType
// Investigation
// Arm
// Participant
// Specimen
// Data
//
// Dataset
// - generate from seed, size
// - load into datbase
// - insert errors, generate messages
//
// Generators
// LinkML
// Relatable schema

use std::fmt::Display;

use anyhow::Result;
use rand::{
    rand_core::{Rng, SeedableRng},
    rngs::ChaCha20Rng,
    RngExt,
};
use serde::{Deserialize, Serialize, Serializer};
use strum::{EnumIter, IntoEnumIterator};

use crate::{core::Relatable, data_table::DataTableBuilder};

pub type Id = u32;

fn term_table_builder(name: &str, terms: impl Iterator<Item = impl Term>) -> DataTableBuilder {
    let mut builder = DataTableBuilder::from_path(&format!("{name}.tsv"));
    builder
        .column("curie")
        .sql_type("TEXT")
        .datatype("trimmed_line")
        .description(&format!("the CURIE for this {name}"));
    builder
        .column("name")
        .sql_type("TEXT")
        .datatype("trimmed_line")
        .structure("primary")
        .description(&format!("the name of this {name}"));
    builder
        .column("description")
        .sql_type("TEXT")
        .datatype("trimmed_line")
        .description(&format!("a description of this {name}"));
    for term in terms {
        builder
            .row([term.curie(), term.to_string(), term.description()])
            .expect("valid row");
    }
    builder
}

pub trait Term: Display {
    fn curie(&self) -> String;
    fn description(&self) -> String;
}

#[derive(Clone, Debug, EnumIter, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VaccineType {
    FooVax(u32),
    FooMist(u32),
}

impl VaccineType {
    pub fn brand(&self) -> String {
        match self {
            VaccineType::FooVax(_) => "FooVax",
            VaccineType::FooMist(_) => "FooMist",
        }
        .to_string()
    }

    pub fn year(&self) -> String {
        match self {
            VaccineType::FooVax(year) => year,
            VaccineType::FooMist(year) => year,
        }
        .to_string()
    }
}

impl Default for VaccineType {
    fn default() -> Self {
        Self::FooVax(2025)
    }
}

impl Display for VaccineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaccineType::FooVax(year) => write!(f, "FooVax {year}"),
            VaccineType::FooMist(year) => write!(f, "FooMist {year}"),
        }
    }
}

impl Term for VaccineType {
    fn curie(&self) -> String {
        format!("ex:{}_{}", self.brand(), self.year())
    }

    fn description(&self) -> String {
        format!(
            "The {} brand vaccine for the {} season.",
            self.brand(),
            self.year()
        )
    }
}

#[derive(Clone, Debug, Default, EnumIter, Serialize, Deserialize)]
pub enum SpecimenType {
    #[default]
    Blood,
}

impl Display for SpecimenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecimenType::Blood => write!(f, "blood"),
        }
    }
}

impl Term for SpecimenType {
    fn curie(&self) -> String {
        match self {
            SpecimenType::Blood => "UBERON:0000178",
        }
        .to_string()
    }

    fn description(&self) -> String {
        match self {
            SpecimenType::Blood => "A fluid that is composed of blood plasma and erythrocytes.",
        }
        .to_string()
    }
}

#[derive(Clone, Debug, Default, EnumIter, Serialize, Deserialize)]
pub enum AssayType {
    #[default]
    ElispotAssay,
}

impl Display for AssayType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssayType::ElispotAssay => write!(f, "ELISPOT"),
        }
    }
}

impl Term for AssayType {
    fn curie(&self) -> String {
        match self {
            AssayType::ElispotAssay => "OBI:0600031",
        }
        .to_string()
    }

    fn description(&self) -> String {
        match self {
            AssayType::ElispotAssay => "A cytometry assay in which cells are cultured on a surface coated with a capture antibody binding a secretory material of interest which subsequently gets stained resulting in a spot for each cell producing the secretory material of interest.",
        }
        .to_string()
    }
}

#[derive(Clone, Debug, Default, EnumIter, Serialize, Deserialize)]
pub enum TargetType {
    #[default]
    Fake1,
    Fake2,
}

impl Display for TargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetType::Fake1 => write!(f, "FAKE1"),
            TargetType::Fake2 => write!(f, "FAKE2"),
        }
    }
}

impl Term for TargetType {
    fn curie(&self) -> String {
        match self {
            TargetType::Fake1 => "ex:FAKE1",
            TargetType::Fake2 => "ex:FAKE2",
        }
        .to_string()
    }

    fn description(&self) -> String {
        format!("The {self} cytokine.")
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Investigation {
    pub investigation_id: Id,
    pub name: String,
    pub description: String,
}

impl Investigation {
    pub fn generate(rng: &mut impl Rng, dataset: &mut Dataset) {
        let vaccine_type = VaccineType::default();
        if !dataset.vaccine_type.contains(&vaccine_type) {
            dataset.vaccine_type.push(vaccine_type.clone());
        }

        let investigation_id = dataset.investigation.len() as Id + 1;

        let arm_id = dataset.arm.len() as Id + 1;
        let arm = Arm {
            arm_id,
            investigation_id,
            vaccine_type: Some(vaccine_type.clone()),
            name: format!("vaccination arm"),
            description: format!("10 healthy adults vaccinated with {vaccine_type}"),
        };
        arm.generate(rng, dataset);
        dataset.arm.push(arm);

        let arm_id = dataset.arm.len() as Id + 1;
        let arm = Arm {
            arm_id,
            investigation_id,
            vaccine_type: None,
            name: format!("control arm"),
            description: format!("10 healthy adults"),
        };
        arm.generate(rng, dataset);
        dataset.arm.push(arm);

        dataset.investigation.push(Investigation {
            investigation_id,
            name: format!("Immune response to {vaccine_type}"),
            description: format!("Immune response to {vaccine_type} in 10 healthy adults, compared to 10 health adult controls, assessed in ELISPOT activity of FAKE1 and FAKE2 cytokines in blood."),
        });
    }
}

impl Into<DataTableBuilder> for Vec<Investigation> {
    fn into(self) -> DataTableBuilder {
        let mut builder = DataTableBuilder::from_path("investigation.tsv");
        builder
            .column("investigation_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("primary")
            .description("the ID for this investigation");
        builder
            .column("name")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("the name of this investigation");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("a description of this investigation");
        for item in &self {
            builder.to_row(item).expect("valid row");
        }
        builder
    }
}

fn serialize_to_string<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Display,
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

fn serialize_option_to_string<T, S>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    T: Display,
    S: Serializer,
{
    match value {
        Some(value) => serialize_to_string(value, serializer),
        None => serialize_to_string(&String::new(), serializer),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Arm {
    pub arm_id: Id,
    pub investigation_id: Id,
    #[serde(serialize_with = "serialize_option_to_string")]
    pub vaccine_type: Option<VaccineType>,
    pub name: String,
    pub description: String,
}

impl Arm {
    pub fn generate(&self, rng: &mut impl Rng, dataset: &mut Dataset) {
        for _ in 1..=10 {
            let arm_id = self.arm_id;
            let participant_id = dataset.participant.len() as Id + 1;
            let participant = Participant {
                participant_id,
                arm_id,
                age_at_enrollment: rng.random_range(18..=65),
            };
            participant.generate(rng, dataset);
            dataset.participant.push(participant);
        }
    }
}

impl Into<DataTableBuilder> for Vec<Arm> {
    fn into(self) -> DataTableBuilder {
        let mut builder = DataTableBuilder::from_path("arm.tsv");
        builder
            .column("arm_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("primary")
            .description("the ID for this arm");
        builder
            .column("investigation_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("from(investigation.investigation_id)")
            .description("the investigation for this arm");
        builder
            .column("vaccine_type")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .nulltype("empty")
            .structure("from(vaccine_type.name)")
            .description("the vaccine used in this arm, if any");
        builder
            .column("name")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("the name of this arm");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("a description of this arm");
        for item in &self {
            builder.to_row(item).expect("valid row");
        }
        builder
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Participant {
    pub participant_id: Id,
    pub arm_id: Id,
    pub age_at_enrollment: u32,
}

impl Participant {
    pub fn generate(&self, rng: &mut impl Rng, dataset: &mut Dataset) {
        let participant_id = self.participant_id;
        for study_day in [0, 1, 3, 7, 14, 28] {
            let specimen_id = dataset.specimen.len() as Id + 1;
            let specimen = Specimen {
                specimen_id,
                participant_id,
                specimen_type: SpecimenType::Blood,
                study_day,
            };
            specimen.generate(rng, dataset);
            dataset.specimen.push(specimen);
        }
    }
}

impl Into<DataTableBuilder> for Vec<Participant> {
    fn into(self) -> DataTableBuilder {
        let mut builder = DataTableBuilder::from_path("participant.tsv");
        builder
            .column("participant_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("primary")
            .description("the investigation for this participant");
        builder
            .column("arm_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("from(arm.arm_id)")
            .description("the arm for this participant");
        builder
            .column("age_at_enrollment")
            .sql_type("INT")
            .datatype("integer")
            .description("the age of this participant at study enrollment");
        for item in &self {
            builder.to_row(item).expect("valid row");
        }
        builder
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Specimen {
    pub specimen_id: Id,
    pub participant_id: Id,
    #[serde(serialize_with = "serialize_to_string")]
    pub specimen_type: SpecimenType,
    pub study_day: u32,
}

impl Specimen {
    pub fn generate(&self, rng: &mut impl Rng, dataset: &mut Dataset) {
        let participant_id = self.participant_id;
        let specimen_id = self.specimen_id;
        for target_type in [TargetType::Fake1, TargetType::Fake2] {
            let data_id = dataset.data.len() as Id + 1;
            let data = Data {
                data_id,
                participant_id,
                specimen_id,
                assay_type: AssayType::ElispotAssay,
                target_type,
                value: rng.random_range(0..100),
                unit: "count".to_string(),
            };
            dataset.data.push(data);
        }
    }
}

impl Into<DataTableBuilder> for Vec<Specimen> {
    fn into(self) -> DataTableBuilder {
        let mut builder = DataTableBuilder::from_path("specimen.tsv");
        builder
            .column("specimen_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("primary")
            .description("the ID for this specimen");
        builder
            .column("participant_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("from(participant.participant_id)")
            .description("the participant for this specimen");
        builder
            .column("specimen_type")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .structure("from(specimen_type.name)")
            .description("the type of this specimen");
        builder
            .column("study_day")
            .sql_type("INT")
            .datatype("integer")
            .description("the study day when this specimen was collected");
        for item in &self {
            builder.to_row(item).expect("valid row");
        }
        builder
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Data {
    pub data_id: Id,
    pub participant_id: Id,
    pub specimen_id: Id,
    #[serde(serialize_with = "serialize_to_string")]
    pub assay_type: AssayType,
    #[serde(serialize_with = "serialize_to_string")]
    pub target_type: TargetType,
    pub value: u32,
    pub unit: String,
}

#[derive(Clone, Debug, Default)]
pub struct Dataset {
    pub vaccine_type: Vec<VaccineType>,
    pub investigation: Vec<Investigation>,
    pub arm: Vec<Arm>,
    pub participant: Vec<Participant>,
    pub specimen: Vec<Specimen>,
    pub data: Vec<Data>,
}

impl Into<DataTableBuilder> for Vec<Data> {
    fn into(self) -> DataTableBuilder {
        let mut builder = DataTableBuilder::from_path("data.tsv");
        builder
            .column("data_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("primary")
            .description("the ID for this data");
        builder
            .column("participant_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("from(participant.participant_id)")
            .description("the participant for this data");
        builder
            .column("specimen_id")
            .sql_type("INT")
            .datatype("integer")
            .structure("from(specimen.specimen_id)")
            .description("the specimen for this data");
        builder
            .column("assay_type")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .structure("from(assay_type.name)")
            .description("the assay type for this data");
        builder
            .column("target_type")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .structure("from(target_type.name)")
            .description("the target type for this data");
        builder
            .column("value")
            .sql_type("INT")
            .datatype("integer")
            .description("the measured value for this data");
        builder
            .column("unit")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("the measurement unit for this data");
        for item in &self {
            builder.to_row(item).expect("valid row");
        }
        builder
    }
}

impl Dataset {
    pub fn generate(seed: u64, size: u32) -> Self {
        let mut dataset = Self::default();
        let mut rng = ChaCha20Rng::seed_from_u64(seed);

        for _ in 1..=size {
            Investigation::generate(&mut rng, &mut dataset);
        }

        dataset
    }

    pub async fn build(self, rltbl: &Relatable) -> Result<()> {
        let mut builder = DataTableBuilder::from_path("tableset.tsv");
        builder
            .column("tableset")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("the name of the table set");
        builder
            .column("group")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .nulltype("empty")
            .description("the group for this table in the table set");
        builder
            .column("table")
            .sql_type("TEXT")
            .datatype("trimmed_line")
            .description("the table in this table set");
        builder.row(["vaccine", "", "investigation"])?;
        builder.row(["vaccine", "", "arm"])?;
        builder.row(["vaccine", "", "participant"])?;
        builder.row(["vaccine", "", "specimen"])?;
        builder.row(["vaccine", "", "data"])?;
        builder.create(rltbl).await?;

        let builder = term_table_builder("vaccine_type", self.vaccine_type.into_iter());
        builder.create(rltbl).await?;

        let builder = term_table_builder("specimen_type", SpecimenType::iter());
        builder.create(rltbl).await?;

        let builder = term_table_builder("assay_type", AssayType::iter());
        builder.create(rltbl).await?;

        let builder = term_table_builder("target_type", TargetType::iter());
        builder.create(rltbl).await?;

        let builder: DataTableBuilder = self.investigation.into();
        builder.create(rltbl).await?;

        let builder: DataTableBuilder = self.arm.into();
        builder.create(rltbl).await?;

        let builder: DataTableBuilder = self.participant.into();
        builder.create(rltbl).await?;

        let builder: DataTableBuilder = self.specimen.into();
        builder.create(rltbl).await?;

        let builder: DataTableBuilder = self.data.into();
        builder.create(rltbl).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RelatableBuilder;
    use anyhow::Result;

    #[test]
    fn test_vaccine_generate() {
        let dataset = Dataset::generate(1, 1);

        assert_eq!(1, dataset.investigation.len());
        assert_eq!(2, dataset.arm.len());
        assert_eq!(2 * 10, dataset.participant.len());
        assert_eq!(2 * 10 * 6, dataset.specimen.len());
        assert_eq!(2 * 10 * 6 * 2, dataset.data.len());
    }

    #[tokio::test]
    async fn test_vaccine_init() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_vaccine_init")
            .init(true)
            .await?;

        let dataset = Dataset::generate(1, 1);
        dataset.build(&rltbl).await?;

        // assert!(false, "SUCCESS!");
        Ok(())
    }
}
