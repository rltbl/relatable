use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use tabld::{
    model::{
        Graph, IndexedMemoryGraph, MemoryGraph, Object, Subject, ANNOTATION_PROPERTY, CLASS,
        DATATYPE, DATA_PROPERTY, OBJECT_PROPERTY, ONTOLOGY, OWL_TYPES, PLAIN, STRING, SUBCLASS_OF,
        TYPE,
    },
    prefix::Prefixes,
};

use crate::{core::Relatable, demo::vaccine::PREFIXES, select::Select};

pub struct Trees {
    pub prefixes: Prefixes,
    pub graphs: IndexMap<String, IndexedMemoryGraph>,
}

impl Trees {
    pub async fn load(rltbl: &Relatable) -> Result<Self> {
        // let prefixes = Prefixes::load(&prefix_path).expect("read prefix file");
        let prefixes = PREFIXES.clone();

        let mut graphs: IndexMap<String, IndexedMemoryGraph> = IndexMap::new();

        let name = "http://example.com/graph";
        let mut graph = MemoryGraph::from_id(name);
        let mut select = Select::from("statement");
        select.order_by("subject");
        select.limit = 0;
        let result = rltbl.fetch(&select).await?;
        for rows in result
            .rows
            .chunk_by(|a, b| a.cells.get("subject") == b.cells.get("subject"))
        {
            // println!("ROW {rows:?}");
            let name = rows
                .first()
                .expect("at least one row")
                .cells
                .get("subject")
                .expect("subject column")
                .text
                .clone();
            let mut subject = Subject::from_name(&name);
            for row in rows {
                let predicate = row
                    .cells
                    .get("predicate")
                    .expect("predicate column")
                    .text
                    .clone();
                let object = row.cells.get("object").expect("object column").text.clone();
                let datatype = row
                    .cells
                    .get("datatype")
                    .expect("datatype column")
                    .text
                    .clone();
                match Object::try_from(&object, &datatype) {
                    Ok(obj) => {
                        subject.insert(&predicate, obj);
                    }
                    Err(_) => {
                        tracing::error!(
                            "Could not parse object '{object}' with datatype '{datatype}'"
                        );
                    }
                };
            }
            graph.insert(subject);
        }

        // let owl_path = "src/demo/vaccine.owl";
        // let rdfxml_input =
        //     std::fs::read_to_string(owl_path).expect(&format!("Read from file {owl_path:?}"));
        // let graph = tabld::rdfxml::read(&rdfxml_input).expect("Read from string");

        let graph = IndexedMemoryGraph::from(graph);
        graphs.insert("ontology".to_string(), graph);

        Ok(Trees { prefixes, graphs })
    }

    // Collect labels, curies, and deprecated terms.
    pub fn fill_signature(
        &self,
        graph: &IndexedMemoryGraph,
        signature: &BTreeSet<&String>,
    ) -> (
        IndexMap<String, String>,
        IndexMap<String, String>,
        IndexSet<String>,
    ) {
        let subjects: IndexMap<String, Option<&Subject>> = signature
            .iter()
            .map(|id| (id.to_string(), graph.get(id).clone()))
            .collect();
        let mut labels = IndexMap::new();
        let mut deprecated_labels = IndexMap::new();
        let mut curies = IndexMap::new();
        let mut deprecated = IndexSet::new();
        for (iri, subject) in subjects {
            let curie = self.prefixes.compact(&iri);
            curies.insert(iri.to_string(), curie.to_string());
            match subject {
                Some(subject) => {
                    let mut name = subject.label();
                    if name == subject.name() {
                        name = curie.to_string();
                    }
                    match subject.deprecated() {
                        true => {
                            deprecated.insert(iri.to_string());
                            deprecated_labels.insert(iri.to_string(), name.to_string());
                        }
                        false => {
                            labels.insert(iri.to_string(), name.to_string());
                        }
                    }
                }
                None => {
                    labels.insert(iri.to_string(), curie.to_string());
                }
            }
        }
        labels.sort_by_cached_key(|_, v| v.to_lowercase());
        deprecated_labels.sort_by_cached_key(|_, v| v.to_lowercase());
        let builtin_labels: IndexMap<String, String> = [
            (ONTOLOGY, "Ontology"),
            (CLASS, "Class"),
            (OBJECT_PROPERTY, "Object Property"),
            (DATA_PROPERTY, "Data Property"),
            (ANNOTATION_PROPERTY, "Annotation Property"),
            (DATATYPE, "Datatype"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .filter(|(k, _)| signature.contains(k))
        .collect();
        let labels = labels
            .into_iter()
            .chain(builtin_labels)
            .chain(deprecated_labels)
            .collect();
        (labels, curies, deprecated)
    }

    pub fn get_context(&self, signature: &BTreeSet<&String>) -> Map<String, Value> {
        let prefixes: IndexSet<String> = signature
            .iter()
            .filter_map(|iri| self.prefixes.prefix(iri))
            .collect();
        let prefixes: Vec<String> = ["rdf", "rdfs", "xsd", "owl"]
            .iter()
            .map(|p| p.to_string())
            .filter(|p| prefixes.contains(p))
            .chain(prefixes.clone())
            .collect();
        let mut context = Map::new();
        for prefix in prefixes {
            context.insert(
                prefix.to_string(),
                json!({
                    "@id": self.prefixes.get(&prefix).unwrap(),
                    "@prefix": true,
                }),
            );
        }
        for predicate in [TYPE, SUBCLASS_OF].iter().map(|s| s.to_string()) {
            if signature.contains(&predicate) {
                context.insert(self.prefixes.compact(&predicate), json!({"@type": "@id"}));
            }
        }
        context
    }

    pub fn get_owl_type(
        &self,
        graph: &IndexedMemoryGraph,
        iri: &str,
    ) -> Result<Map<String, Value>, String> {
        if !OWL_TYPES.contains(&iri) {
            return Err(format!(r#"No such OWL type {iri}."#));
        }

        let owl_types: Vec<String> = OWL_TYPES
            .iter()
            .filter(|s| graph.roots(s).len() > 0)
            .map(|s| s.to_string())
            .collect();

        let mut roots = graph.roots(iri);

        let mut signature: BTreeSet<&String> = BTreeSet::new();
        signature.extend(owl_types.iter());
        signature.extend(roots.iter());

        let (labels, curies, deprecated) = self.fill_signature(graph, &signature);

        roots.sort_by(|a, b| {
            labels
                .get_index_of(*a)
                .unwrap_or_default()
                .cmp(&labels.get_index_of(*b).unwrap_or_default())
        });

        // let mut tree: Map<String, Value> = Map::new();
        // for owl_type in owl_types {
        //     if iri == owl_type {
        //         let branch: Map<String, Value> =
        //             roots.iter().map(|s| (s.to_string(), json!({}))).collect();
        //         tree.insert(owl_type.to_string(), branch.into());
        //     } else {
        //         tree.insert(owl_type.to_string(), json!({}));
        //     }
        // }
        let mut tree: Vec<Value> = Vec::new();
        for owl_type in owl_types {
            let children: Value = if iri == owl_type {
                let branch: Vec<Value> = roots
                    .iter()
                    .map(|s| {
                        json!({
                            "id": s,
                            "rel": "SUPERCLASS_OF",
                            "children": [],
                        })
                    })
                    .collect();
                branch.into()
            } else {
                json!({})
            };
            tree.push(json!({
                "id": owl_type,
                "rel": "SUPERCLASS_OF",
                "children": children,
            }));
        }

        Ok(json!({
            "@id": iri.to_string(),
            "@graph": [
              {
                "@id": iri.to_string(),
              }
            ],
            "labels": labels,
            "curies": curies,
            "deprecated": deprecated,
            "predicates": {},
            "tree": tree,
        })
        .as_object()
        .unwrap()
        .clone())
    }

    fn compact_map(
        &self,
        context: &Map<String, Value>,
        map: &BTreeMap<String, BTreeSet<Object>>,
    ) -> Value {
        json!(map
            .iter()
            .map(|(p, os)| (
                self.prefixes.compact(p),
                self.compact_objects(context, p, &Vec::from_iter(os.iter().cloned()))
            ))
            .collect::<Map<String, Value>>())
    }

    fn compact_objects(
        &self,
        context: &Map<String, Value>,
        predicate: &str,
        objects: &Vec<Object>,
    ) -> Value {
        let objects: Vec<Value> = objects
            .iter()
            .map(|o| self.compact_object(context, predicate, o))
            .collect();
        if objects.len() == 1 {
            json!(objects.first().unwrap())
        } else {
            json!(objects)
        }
    }

    fn compact_object(
        &self,
        context: &Map<String, Value>,
        predicate: &str,
        object: &Object,
    ) -> Value {
        let typed = json!({"@type": "@id"});
        let json = match object {
            Object::ID { id, .. } => {
                let p = self.prefixes.compact(&predicate);
                if context.get(&p) == Some(&typed) {
                    json!(self.prefixes.compact(id))
                } else {
                    json!({"@id": self.prefixes.compact(id)})
                }
            }
            Object::TypedLiteral {
                value, datatype, ..
            } => match datatype.as_str() {
                STRING | PLAIN => json!(value),
                _ => json!({
                    "@value": value,
                    "@type": self.prefixes.compact(datatype),
                }),
            },
            Object::LanguageLiteral {
                value, language, ..
            } => {
                json!({
                    "@value": value,
                    "@language": language,
                })
            }
            Object::List { list, .. } => self.compact_objects(context, predicate, list),
            Object::Map { content, .. } => self.compact_map(context, content),
        };
        // TODO: Maybe include annotations
        // let annotations: Vec<Value> = object
        //     .annotations()
        //     .iter()
        //     .map(|map| self.compact_map(context, map))
        //     .collect();
        // if annotations.len() > 0 {
        //     json.as_object_mut()
        //         .unwrap()
        //         .insert("@annotations".to_string(), json!(annotations));
        // }
        json
    }

    pub fn get_term_jsonld(&self, subject: &Subject) -> Result<Map<String, Value>, String> {
        let mut json: Map<String, Value> = Map::new();
        let context = self.get_context(&subject.signature());
        json.insert("@context".to_string(), context.clone().into());
        // json.insert("@id".to_string(), json!(curie));
        for (predicate, objects) in subject.predicates() {
            json.insert(
                self.prefixes.compact(&predicate),
                self.compact_objects(&context, &predicate, &Vec::from_iter(objects.into_iter())),
            );
        }
        Ok(json)
    }

    pub fn get_term(
        &self,
        graph: &IndexedMemoryGraph,
        subject: &Subject,
        iri: &str,
    ) -> Result<Map<String, Value>, String> {
        let part_of = "http://purl.obolibrary.org/obo/BFO_0000050";
        let has_part = "http://purl.obolibrary.org/obo/BFO_0000051";
        let located_in = "http://purl.obolibrary.org/obo/RO_0001025";
        let location_of = "http://purl.obolibrary.org/obo/RO_0001015";
        let develops_from = "http://purl.obolibrary.org/obo/RO_0002202";
        let develops_into = "http://purl.obolibrary.org/obo/RO_0002203";
        let has_member = "http://purl.obolibrary.org/obo/RO_0002351";
        let member_of = "http://purl.obolibrary.org/obo/RO_0002350";
        // let restrictions = [has_part, location_of, develops_into, has_member];
        // let restrictions = [has_part, has_member];
        let restrictions = [has_part];
        // let inverses = [part_of, located_in, develops_from, member_of];
        // let inverses = [part_of, member_of];
        let inverses = [part_of];

        let mut map: IndexMap<String, Value> = IndexMap::new();
        for (predicate, objects) in subject.predicates() {
            map.insert(predicate, json!(objects));
        }
        let mut ancestors = graph.ancestors2(&iri, &inverses);
        // let mut ancestors = graph.ancestors2(&iri, &[]);
        let i = iri.to_string();
        ancestors.insert(&i);
        let parents = graph.parents2(&iri, &inverses);
        println!("PARENTS {parents:?}");
        ancestors.extend(parents);

        let owl_type = subject.owl_type().unwrap_or(CLASS);

        let mut roots: Vec<&String> = graph
            .roots(owl_type)
            .into_iter()
            .filter(|s| ancestors.contains(s))
            .collect();

        println!("ROOTS: {roots:?}");

        // let mut children = IndexSet::from_iter(graph.children(&iri));
        let edges = graph.edges(iri);
        let mut children: Vec<(String, String)> = edges
            .iter()
            .filter(|(p, _)| **p == "SUPERCLASS_OF" || restrictions.contains(&p.as_str()))
            .flat_map(|(p, os)| os.iter().map(|o| (p.to_string(), o.to_string())))
            .collect();

        let owl_types: Vec<String> = OWL_TYPES
            .iter()
            .filter(|s| graph.roots(s).len() > 0)
            .map(|s| s.to_string())
            .collect();
        let mut signature = subject.signature();
        signature.extend(owl_types.iter());
        signature.extend(ancestors.iter());
        signature.extend(children.iter().flat_map(|(p, o)| [p, o]));

        let (labels, curies, deprecated) = self.fill_signature(graph, &signature);

        roots.sort_by(|a, b| labels.get_index_of(*a).cmp(&labels.get_index_of(*b)));
        children.sort_by(|(_, a), (_, b)| labels.get_index_of(a).cmp(&labels.get_index_of(b)));

        // Build an ancestor tree from roots by depth-first recursion into children.
        fn build_tree(
            graph: &IndexedMemoryGraph,
            ancestors: &BTreeSet<&String>,
            labels: &IndexMap<String, String>,
            restrictions: &[&str],
            edges: &Vec<(String, String)>,
        ) -> Vec<Value> {
            let mut tree = Vec::new();
            let mut edges = edges.clone();
            edges.sort_by(|(_, a), (_, b)| labels.get_index_of(a).cmp(&labels.get_index_of(b)));

            for (edge, node) in edges.iter() {
                let node_edges: Vec<(String, String)> = graph
                    .edges(node)
                    .iter()
                    .filter(|(p, _)| **p == "SUPERCLASS_OF" || restrictions.contains(&p.as_str()))
                    .flat_map(|(p, os)| os.iter().map(|o| (p.to_string(), o.to_string())))
                    .filter(|(_, o)| ancestors.contains(o))
                    .collect();
                let children = build_tree(graph, ancestors, labels, restrictions, &node_edges);
                tree.push(json!({
                    "id": node,
                    "rel": edge,
                    "children": children,
                }));
            }
            tree
        }

        // Insert children into only the first instance of IRI in the tree.
        // WARN: This relies on preserving order in the JSON Map.
        fn insert_children(tree: &mut Vec<Value>, iri: &str, children: &Vec<(String, String)>) {
            let first = match tree.first_mut() {
                Some(first) => first,
                None => return,
            };
            let first = match first.as_object_mut() {
                Some(first) => first,
                None => return,
            };
            if first.get("id").and_then(|v| v.as_str()) == Some(iri) {
                let branch: Vec<Value> = children
                    .iter()
                    // .map(|s| (s.to_string(), json!({})))
                    .map(|(p, o)| {
                        json!({
                            "id": o,
                            "rel": p,
                            "children": [],
                        })
                    })
                    .collect();
                first.insert("children".to_string(), branch.into());
                first.insert("focus".to_string(), Value::Bool(true));
            } else {
                match first.get_mut("children").and_then(|v| v.as_array_mut()) {
                    Some(branch) => insert_children(branch, iri, children),
                    None => return,
                }
            }
        }
        let top: Vec<(String, String)> = roots
            .iter()
            .map(|iri| ("SUPERCLASS_OF".to_string(), iri.to_string()))
            .collect();
        let mut inner_tree = build_tree(&graph, &ancestors, &labels, &restrictions, &top);
        insert_children(&mut inner_tree, &iri, &children);

        let mut tree: Vec<Value> = Vec::new();
        for owl_type in owl_types {
            let children: Value = if subject.has_type(&owl_type) {
                inner_tree.clone().into()
            } else {
                json!({})
            };
            tree.push(json!({
                "id": owl_type,
                "rel": "SUPERCLASS_OF",
                "children": children,
            }));
        }

        Ok(json!({
            "@id": iri.to_string(),
            "@graph": [
              {
                "@id": iri.to_string(),
              }
            ],
            "labels": labels,
            "curies": curies,
            "icons": {
                has_part: "\u{1F15F}",
                part_of: "p",
                location_of: "\u{1F15B}",
                located_in: "l",
                develops_into: "🄳",
                develops_from: "d",
                has_member: "\u{1F15C}",
                member_of: "m",
            },
            "deprecated": deprecated,
            "predicates": map,
            "tree": tree,
        })
        .as_object()
        .unwrap()
        .clone())
    }
}
