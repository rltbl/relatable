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

use std::{fmt::Display, sync::LazyLock};

use anyhow::Result;
use indexmap::{indexmap, IndexMap};
use rand::{
    rand_core::{Rng, SeedableRng},
    rngs::ChaCha20Rng,
    RngExt,
};
use regex::Regex;
use serde::{Deserialize, Serialize, Serializer};
use strum::{EnumIter, IntoEnumIterator};
use tabld::{
    model::{CLASS, LABEL, SUBCLASS_OF, TYPE},
    prefix::Prefixes,
};

use crate::{
    core::Relatable,
    data_table::DataTableBuilder,
    linkml::{Class, Classes, Enum, Enums},
};

pub type Id = u32;

static VACCINE: &str = "https://example.org/vaccine/";
static OBO: &str = "http://purl.obolibrary.org/obo/";
static NONWORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\W+").unwrap());
pub static PREFIXES: LazyLock<Prefixes> = LazyLock::new(|| {
    Prefixes::from_iter(
        [
            ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
            ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
            ("xsd", "http://www.w3.org/2001/XMLSchema#"),
            ("owl", "http://www.w3.org/2002/07/owl#"),
            ("oio", "http://www.geneontology.org/formats/oboInOwl#"),
            ("dce", "http://purl.org/dc/elements/1.1/"),
            ("dct", "http://purl.org/dc/terms/"),
            ("foaf", "http://xmlns.com/foaf/0.1/"),
            ("BFO", "http://purl.obolibrary.org/obo/BFO_"),
            ("COB", "http://purl.obolibrary.org/obo/COB_"),
            ("DOID", "http://purl.obolibrary.org/obo/DOID_"),
            ("IAO", "http://purl.obolibrary.org/obo/IAO_"),
            ("NCBITaxon", "http://purl.obolibrary.org/obo/NCBITaxon_"),
            ("ncbi", "http://purl.obolibrary.org/obo/ncbitaxon#"),
            ("OBI", "http://purl.obolibrary.org/obo/OBI_"),
            ("PR", "http://purl.obolibrary.org/obo/PR_"),
            ("VO", "http://purl.obolibrary.org/obo/VO_"),
            ("UBERON", "http://purl.obolibrary.org/obo/UBERON_"),
            ("obo", "http://purl.obolibrary.org/obo/"),
            ("ONTIE", "https://ontology.iedb.org/ontology/ONTIE_"),
            ("IEDB", "http://iedb.org/"),
            ("ex", "https://example.org/"),
            ("vaccine", "https://example.org/vaccine/"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string())),
    )
});

// Replace non-wor
fn to_curie(prefix: &str, local: &str) -> String {
    [
        NONWORD.replace(prefix, "_").to_string(),
        NONWORD.replace(local, "_").to_string(),
    ]
    .join(":")
}

fn to_iri(curie: &str) -> String {
    let iri = PREFIXES.expand(curie);
    if iri == curie {
        tracing::error!("Missing prefix for {curie}");
    }
    iri
}

fn term_table_builder(
    name: &str,
    terms: impl Iterator<Item = impl Into<Term>>,
) -> DataTableBuilder {
    let mut builder = DataTableBuilder::from_path(&format!("{name}.tsv"));
    builder
        .column("label")
        .sql_type("TEXT")
        .datatype("phrase")
        .structure("from(terminology.label)") // TODO: also primary
        .description(&format!("the label for this {name}"));
    builder
        .column("curie")
        .sql_type("TEXT")
        .datatype("nonword")
        .structure("from(terminology.curie)") // TODO: also primary
        .description(&format!("the CURIE for this {name}"));
    builder
        .column("description")
        .sql_type("TEXT")
        .datatype("paragraph")
        .description(&format!("a description of this {name}"));
    for term in terms {
        let term: Term = term.into();
        builder
            .row([term.label, term.curie, term.description])
            .expect("valid row");
    }
    builder
}

#[derive(Clone, Debug)]
pub struct Term {
    pub label: String,
    pub curie: String,
    pub iri: String,
    pub rdf_type: String,
    pub parent_iris: Vec<String>,
    pub description: String,
}

impl Default for Term {
    fn default() -> Self {
        Self {
            label: Default::default(),
            curie: Default::default(),
            iri: Default::default(),
            rdf_type: CLASS.to_string(),
            parent_iris: Default::default(),
            description: Default::default(),
        }
    }
}

impl From<(&str, &str)> for Term {
    fn from((curie, label): (&str, &str)) -> Self {
        Self {
            label: label.to_string(),
            curie: curie.to_string(),
            iri: to_iri(curie),
            ..Default::default()
        }
    }
}

impl Term {
    pub fn parent(mut self, curie: &str) -> Self {
        self.parent_iris.push(to_iri(curie));
        self
    }

    pub fn rdf_type(mut self, curie_or_iri: &str) -> Self {
        self.rdf_type = PREFIXES.expand(curie_or_iri);
        self
    }
}

#[derive(Clone, Debug, EnumIter, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VaccineType {
    BarVax(u32),
    FooMist(u32),
    FooVax(u32),
    VacciFoo(u32),
}

impl VaccineType {
    pub fn brand(&self) -> String {
        match self {
            VaccineType::BarVax(_) => "BarVax",
            VaccineType::FooMist(_) => "FooMist",
            VaccineType::FooVax(_) => "FooVax",
            VaccineType::VacciFoo(_) => "VacciFoo",
        }
        .to_string()
    }

    pub fn year(&self) -> String {
        match self {
            VaccineType::BarVax(year) => year,
            VaccineType::FooMist(year) => year,
            VaccineType::FooVax(year) => year,
            VaccineType::VacciFoo(year) => year,
        }
        .to_string()
    }

    pub fn generate(rng: &mut impl Rng) -> Self {
        let year: u32 = rng.random_range(2000..=2026);
        match rng.random_range(1..=4) {
            1 => VaccineType::BarVax(year),
            2 => VaccineType::FooMist(year),
            3 => VaccineType::FooVax(year),
            4 => VaccineType::VacciFoo(year),
            _ => unreachable!(),
        }
    }
}

impl Default for VaccineType {
    fn default() -> Self {
        Self::FooVax(2025)
    }
}

impl Display for VaccineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.brand(), self.year())
    }
}

impl Into<Term> for VaccineType {
    fn into(self) -> Term {
        let local = format!("{}_{}", self.brand(), self.year());
        Term {
            label: self.to_string(),
            curie: to_curie("vaccine", &local),
            iri: format!("{VACCINE}{local}"),
            parent_iris: vec![format!("{OBO}VO_0000001")],
            description: format!(
                "The {} brand vaccine for the {} season.",
                self.brand(),
                self.year()
            ),
            ..Default::default()
        }
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

impl Into<Term> for SpecimenType {
    fn into(self) -> Term {
        let curie = match self {
            SpecimenType::Blood => "UBERON:0000178",
        }
        .to_string();
        Term {
            label: self.to_string(),
            curie: curie.clone(),
            iri: to_iri(&curie),
            parent_iris: vec![format!("{OBO}BFO_0000040")],
            description: match self {
                SpecimenType::Blood => "A fluid that is composed of blood plasma and erythrocytes.",
            }
            .to_string(),
            ..Default::default()
        }
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

impl Into<Term> for AssayType {
    fn into(self) -> Term {
        let curie = match self {
            AssayType::ElispotAssay => "OBI:0600031",
        }
        .to_string();
        Term {
            label: self.to_string(),
            curie: curie.clone(),
            iri: to_iri(&curie),
            parent_iris: vec![format!("{OBO}OBI_0000070")],
            description: match self {
            AssayType::ElispotAssay => "A cytometry assay in which cells are cultured on a surface coated with a capture antibody binding a secretory material of interest which subsequently gets stained resulting in a spot for each cell producing the secretory material of interest.",
            }
            .to_string(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, EnumIter, Serialize, Deserialize)]
pub enum TargetType {
    Fake(u32),
    Phony(u32),
}

impl TargetType {
    pub fn generate(rng: &mut impl Rng) -> Self {
        let number: u32 = rng.random_range(1..10);
        match rng.random_range(1..=2) {
            1 => TargetType::Fake(number),
            2 => TargetType::Phony(number),
            _ => unreachable!(),
        }
    }

    pub fn terms() -> impl Iterator<Item = Self> {
        [Self::Fake(1), Self::Fake(2)].into_iter()
    }
}

impl Default for TargetType {
    fn default() -> Self {
        Self::Fake(1)
    }
}

impl Display for TargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetType::Fake(number) => write!(f, "FAKE{number}"),
            TargetType::Phony(number) => write!(f, "PHONY{number}"),
        }
    }
}

impl Into<Term> for TargetType {
    fn into(self) -> Term {
        Term {
            label: self.to_string(),
            curie: to_curie("vaccine", &self.to_string()),
            iri: format!("{VACCINE}{self}"),
            parent_iris: vec![format!("{OBO}PR_000000001")],
            description: format!("The {self} cytokine."),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Investigation {
    pub investigation_id: Id,
    pub label: String,
    pub description: String,
}

impl Investigation {
    pub fn generate(rng: &mut impl Rng, dataset: &mut Dataset) {
        let vaccine_type = VaccineType::generate(rng);
        if !dataset.vaccine_type.contains(&vaccine_type) {
            dataset.vaccine_type.push(vaccine_type.clone());
        }

        // let target_types = TargetType::generate(rng);

        let investigation_id = dataset.investigation.len() as Id + 1;

        let arm_id = dataset.arm.len() as Id + 1;
        let arm = Arm {
            arm_id,
            investigation_id,
            vaccine_type: Some(vaccine_type.clone()),
            label: format!("vaccination arm"),
            description: format!("10 healthy adults vaccinated with {vaccine_type}"),
        };
        arm.generate(rng, dataset);
        dataset.arm.push(arm);

        let arm_id = dataset.arm.len() as Id + 1;
        let arm = Arm {
            arm_id,
            investigation_id,
            vaccine_type: None,
            label: format!("control arm"),
            description: format!("10 healthy adults"),
        };
        arm.generate(rng, dataset);
        dataset.arm.push(arm);

        dataset.investigation.push(Investigation {
            investigation_id,
            label: format!("Immune response to {vaccine_type}"),
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
            .column("label")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the label for this investigation");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("paragraph")
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
    pub label: String,
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
            .datatype("phrase")
            .nulltype("empty")
            .structure("from(vaccine_type.label)")
            .description("the vaccine used in this arm, if any");
        builder
            .column("label")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the label for this arm");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("paragraph")
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
        for target_type in [TargetType::Fake(1), TargetType::Fake(2)] {
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
            .datatype("phrase")
            .structure("from(specimen_type.label)")
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
            .datatype("phrase")
            .structure("from(assay_type.label)")
            .description("the assay type for this data");
        builder
            .column("target_type")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("from(target_type.label)")
            .description("the target type for this data");
        builder
            .column("value")
            .sql_type("INT")
            .datatype("integer")
            .description("the measured value for this data");
        builder
            .column("unit")
            .sql_type("TEXT")
            .datatype("phrase")
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
        let mut builder = DataTableBuilder::from_path("rule.tsv");
        builder
            .column("rule")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the name of rule");
        builder
            .column("table")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("from(table.table)")
            .description("the table for this rule");
        builder
            .column("when_column")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("from(column.column)")
            .description("the column for the when condition");
        builder
            .column("when_condition")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the when condition for this rule");
        builder
            .column("then_column")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("from(column.column)")
            .description("the column for the then condition");
        builder
            .column("then_condition")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the then condition for this rule");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("paragraph")
            .description("the description of this rule");
        builder.row([
            "ELISPOT count",
            "data",
            "assay_type",
            "equals(ELISPOT)",
            "unit",
            "equals(count)",
            "when the assay type is 'ELISPOT' then the unit must be 'count'",
        ])?;
        builder.create(rltbl).await?;

        let mut builder = DataTableBuilder::from_path("tableset.tsv");
        builder
            .column("tableset")
            .sql_type("TEXT")
            .datatype("phrase")
            .description("the name of the table set");
        builder
            .column("group")
            .sql_type("TEXT")
            .datatype("phrase")
            .nulltype("empty")
            .description("the group for this table in the table set");
        builder
            .column("table")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("from(table.table)")
            .description("the table in this table set");
        builder.row(["schema", "", "table"])?;
        builder.row(["schema", "", "column"])?;
        builder.row(["schema", "", "datatype"])?;
        builder.row(["schema", "", "rule"])?;
        builder.row(["schema", "", "tableset"])?;
        builder.row(["schema", "", "message"])?;
        builder.row(["terminology", "", "terminology"])?;
        builder.row(["terminology", "", "vaccine_type"])?;
        builder.row(["terminology", "", "specimen_type"])?;
        builder.row(["terminology", "", "assay_type"])?;
        builder.row(["terminology", "", "target_type"])?;
        builder.row(["terminology", "", "statement"])?;
        builder.row(["terminology", "", "ontology"])?;
        builder.row(["vaccine", "", "investigation"])?;
        builder.row(["vaccine", "", "arm"])?;
        builder.row(["vaccine", "", "participant"])?;
        builder.row(["vaccine", "", "specimen"])?;
        builder.row(["vaccine", "", "data"])?;
        builder.create(rltbl).await?;

        for builder in self.builders().into_values() {
            builder.create(rltbl).await?;
        }

        Ok(())
    }

    fn terms(&self) -> Vec<Term> {
        let mut terms = Vec::new();
        for term in &self.vaccine_type {
            terms.push(term.clone().into());
        }
        for term in SpecimenType::iter() {
            terms.push(term.into());
        }
        for term in AssayType::iter() {
            terms.push(term.into());
        }
        for term in TargetType::terms() {
            terms.push(term.into());
        }
        terms
    }

    fn builders(self) -> IndexMap<String, DataTableBuilder> {
        let name = "statement";
        let mut builder = DataTableBuilder::from_path(&format!("{name}.tsv"));
        builder
            .column("subject")
            .sql_type("TEXT")
            .datatype("nonspace")
            .description(&format!("the subject for this statement"));
        builder
            .column("predicate")
            .sql_type("TEXT")
            .datatype("nonspace")
            .description(&format!("the predicate for this statement"));
        builder
            .column("object")
            .sql_type("TEXT")
            .datatype("text")
            .description(&format!("the object for this statement"));
        builder
            .column("datatype")
            .sql_type("TEXT")
            .datatype("nonspace")
            .description(&format!("the datatype for this statement"));
        builder
            .column("annotation")
            .sql_type("TEXT")
            .datatype("any")
            .description(&format!("the annotation for this statement"));

        // let owl_path = "src/demo/vaccine.owl";
        // let rdfxml_input =
        //     std::fs::read_to_string(owl_path).expect(&format!("Read from file {owl_path:?}"));
        // let graph = tabld::rdfxml::read(&rdfxml_input).expect("Read from string");
        // for triple in graph.triples() {
        //     builder
        //         .row([
        //             triple.subject,
        //             triple.predicate,
        //             triple.object.object(),
        //             triple.object.datatype(),
        //             triple.object.annotation(),
        //         ])
        //         .unwrap();
        // }

        let terms: Vec<Term> = [
            Term::from(("ex:vaccine", "Fake Vaccine Ontology")).rdf_type(TYPE),
            Term::from(("BFO:0000040", "material entity")).parent("owl:Thing"),
            Term::from(("VO:0000001", "vaccine")).parent("BFO:0000040"),
            Term::from(("PR:000000001", "protein")).parent("BFO:0000040"),
            Term::from(("BFO:0000015", "process")),
            Term::from(("OBI:0000070", "assay")).parent("BFO:0000015"),
        ]
        .into_iter()
        .chain(self.terms().into_iter())
        .collect();

        for term in terms {
            builder
                .row([&term.iri, TYPE, &term.rdf_type, "_ID", ""])
                .unwrap();
            for parent_iri in &term.parent_iris {
                builder
                    .row([&term.iri, SUBCLASS_OF, parent_iri, "_ID", ""])
                    .unwrap();
            }
            builder
                .row([&term.iri, LABEL, &term.label, "@en", ""])
                .unwrap();
            if !term.description.is_empty() {
                builder
                    .row([
                        &term.iri,
                        &format!("{OBO}IAO_0000115"),
                        &term.description,
                        "@en",
                        "",
                    ])
                    .unwrap();
            }
        }
        let statement_builder = builder;

        let mut builder = DataTableBuilder::from_path(&format!("terminology.tsv"));
        builder
            .column("label")
            .sql_type("TEXT")
            .datatype("phrase")
            .structure("primary")
            .description("the label for this term");
        builder
            .column("curie")
            .sql_type("TEXT")
            .datatype("nonspace")
            .description("the CURIE for this term");
        builder
            .column("iri")
            .sql_type("TEXT")
            .datatype("nonspace")
            .structure("from(statement.subject)")
            .description("the IRI for this term");
        builder
            .column("description")
            .sql_type("TEXT")
            .datatype("paragraph")
            .description("a description of this term");
        for term in self.terms() {
            builder
                .row([term.label, term.curie, term.iri, term.description])
                .unwrap();
        }
        let terminology_builder = builder;

        indexmap![
            "statement".to_string() => statement_builder,
            "terminology".to_string() => terminology_builder,
            "vaccine_type".to_string() => term_table_builder("vaccine_type", self.vaccine_type.into_iter()),
            "specimen_type".to_string() => term_table_builder("specimen_type", SpecimenType::iter()),
            "assay_type".to_string() => term_table_builder("assay_type", AssayType::iter()),
            "target_type".to_string() => term_table_builder("target_type", TargetType::terms()),
            "investigation".to_string() => self.investigation.into(),
            "arm".to_string() => self.arm.into(),
            "participant".to_string() => self.participant.into(),
            "speciment".to_string() => self.specimen.into(),
            "data".to_string() => self.data.into(),
        ]
    }

    pub fn linkml(self) -> String {
        let builders = self.builders();
        let mut enums: Vec<Enum> = Vec::new();
        let mut classes: Vec<Class> = Vec::new();
        for (name, builder) in builders {
            if ["terminology", "statement"].contains(&name.as_str()) {
                continue;
            }
            if name.ends_with("_type") {
                enums.push(builder.linkml_enum());
            } else {
                classes.push(builder.linkml_class());
            }
        }
        let classes = Classes::from(classes);
        let enums = Enums::from(enums);
        let slots = classes.slots();

        let slots = slots.render();
        let classes = classes.render();
        let enums = enums.render();
        format!(
            r#"id: https://example.org/linkml/examples/vaccine
name: vaccine
description: |-
  Synthetic data about vaccine studies.
license: https://creativecommons.org/publicdomain/zero/1.0/
default_curi_maps:
  - semweb_context
imports:
  - linkml:types
prefixes:
  vaccine: https://example.org/linkml/examples/vaccine/
  linkml: https://w3id.org/linkml/
  schema: http://schema.org/
  rdfs: http://www.w3.org/2000/01/rdf-schema#
  skos: http://www.w3.org/2004/02/skos/core#
  OBI: http://purl.obolibrary.org/obo/OBI_
  UBERON: http://purl.obolibrary.org/obo/UBERON_
default_prefix: vaccine
default_range: string

emit_prefixes:
  - rdf
  - rdfs
  - xsd
  - skos
  - vaccine
  - OBI
  - UBERON

{classes}

{slots}

{enums}
"#
        )
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
