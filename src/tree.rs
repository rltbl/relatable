use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use rltbl_db::StringRow;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::{BTreeMap, BTreeSet};
use tabld::{
    model::{
        Graph, IndexedMemoryGraph, MemoryGraph, Object, Subject, ANNOTATION_PROPERTY, CLASS,
        DATATYPE, DATA_PROPERTY, OBJECT_PROPERTY, ONTOLOGY, OWL_TYPES, PLAIN, STRING, SUBCLASS_OF,
        TYPE,
    },
    prefix::Prefixes,
};

use crate::{core::Relatable, demo::vaccine::PREFIXES};

// Collect labels, curies, and deprecated terms.
pub fn fill_signature(
    prefixes: &Prefixes,
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
        let curie = prefixes.compact(&iri);
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

pub fn get_context(
    prefix_map: &Prefixes,
    signature: &BTreeSet<&String>,
) -> JsonMap<String, JsonValue> {
    let prefixes: IndexSet<String> = signature
        .iter()
        .filter_map(|iri| prefix_map.prefix(iri))
        .collect();
    let prefixes: Vec<String> = ["rdf", "rdfs", "xsd", "owl"]
        .iter()
        .map(|p| p.to_string())
        .filter(|p| prefixes.contains(p))
        .chain(prefixes.clone())
        .collect();
    let mut context = JsonMap::new();
    for prefix in prefixes {
        context.insert(
            prefix.to_string(),
            json!({
                "@id": prefix_map.get(&prefix).unwrap(),
                "@prefix": true,
            }),
        );
    }
    for predicate in [TYPE, SUBCLASS_OF].iter().map(|s| s.to_string()) {
        if signature.contains(&predicate) {
            context.insert(prefix_map.compact(&predicate), json!({"@type": "@id"}));
        }
    }
    context
}

pub fn get_owl_type(
    prefixes: &Prefixes,
    graph: &IndexedMemoryGraph,
    iri: &str,
) -> Result<JsonMap<String, JsonValue>, String> {
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

    let (labels, curies, deprecated) = fill_signature(prefixes, graph, &signature);

    roots.sort_by(|a, b| {
        labels
            .get_index_of(*a)
            .unwrap_or_default()
            .cmp(&labels.get_index_of(*b).unwrap_or_default())
    });

    // let mut tree: JsonMap<String, JsonValue> = JsonMap::new();
    // for owl_type in owl_types {
    //     if iri == owl_type {
    //         let branch: JsonMap<String, JsonValue> =
    //             roots.iter().map(|s| (s.to_string(), json!({}))).collect();
    //         tree.insert(owl_type.to_string(), branch.into());
    //     } else {
    //         tree.insert(owl_type.to_string(), json!({}));
    //     }
    // }
    let mut tree: Vec<JsonValue> = Vec::new();
    for owl_type in owl_types {
        let children: JsonValue = if iri == owl_type {
            let branch: Vec<JsonValue> = roots
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
    prefixes: &Prefixes,
    context: &JsonMap<String, JsonValue>,
    map: &BTreeMap<String, BTreeSet<Object>>,
) -> JsonValue {
    json!(map
        .iter()
        .map(|(p, os)| (
            prefixes.compact(p),
            compact_objects(prefixes, context, p, &Vec::from_iter(os.iter().cloned()))
        ))
        .collect::<JsonMap<String, JsonValue>>())
}

fn compact_objects(
    prefixes: &Prefixes,
    context: &JsonMap<String, JsonValue>,
    predicate: &str,
    objects: &Vec<Object>,
) -> JsonValue {
    let objects: Vec<JsonValue> = objects
        .iter()
        .map(|o| compact_object(prefixes, context, predicate, o))
        .collect();
    if objects.len() == 1 {
        json!(objects.first().unwrap())
    } else {
        json!(objects)
    }
}

fn compact_object(
    prefixes: &Prefixes,
    context: &JsonMap<String, JsonValue>,
    predicate: &str,
    object: &Object,
) -> JsonValue {
    let typed = json!({"@type": "@id"});
    let json = match object {
        Object::ID { id, .. } => {
            let p = prefixes.compact(&predicate);
            if context.get(&p) == Some(&typed) {
                json!(prefixes.compact(id))
            } else {
                json!({"@id": prefixes.compact(id)})
            }
        }
        Object::TypedLiteral {
            value, datatype, ..
        } => match datatype.as_str() {
            STRING | PLAIN => json!(value),
            _ => json!({
                "@value": value,
                "@type": prefixes.compact(datatype),
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
        Object::List { list, .. } => compact_objects(prefixes, context, predicate, list),
        Object::Map { content, .. } => compact_map(prefixes, context, content),
    };
    // TODO: Maybe include annotations
    // let annotations: Vec<JsonValue> = object
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

pub fn get_term_jsonld(
    prefixes: &Prefixes,
    subject: &Subject,
) -> Result<JsonMap<String, JsonValue>, String> {
    let mut json: JsonMap<String, JsonValue> = JsonMap::new();
    let context = get_context(prefixes, &subject.signature());
    json.insert("@context".to_string(), context.clone().into());
    // json.insert("@id".to_string(), json!(curie));
    for (predicate, objects) in subject.predicates() {
        json.insert(
            prefixes.compact(&predicate),
            compact_objects(
                prefixes,
                &context,
                &predicate,
                &Vec::from_iter(objects.into_iter()),
            ),
        );
    }
    Ok(json)
}

pub fn get_term_inner(
    prefixes: &Prefixes,
    graph: &IndexedMemoryGraph,
    subject: &Subject,
    iri: &str,
) -> Result<JsonMap<String, JsonValue>> {
    let part_of = "http://purl.obolibrary.org/obo/BFO_0000050";
    let has_part = "http://purl.obolibrary.org/obo/BFO_0000051";
    // let located_in = "http://purl.obolibrary.org/obo/RO_0001025";
    // let location_of = "http://purl.obolibrary.org/obo/RO_0001015";
    // let develops_from = "http://purl.obolibrary.org/obo/RO_0002202";
    // let develops_into = "http://purl.obolibrary.org/obo/RO_0002203";
    // let has_member = "http://purl.obolibrary.org/obo/RO_0002351";
    // let member_of = "http://purl.obolibrary.org/obo/RO_0002350";
    // let restrictions = [has_part, location_of, develops_into, has_member];
    // let restrictions = [has_part, has_member];
    let restrictions = [has_part];
    // let inverses = [part_of, located_in, develops_from, member_of];
    // let inverses = [part_of, member_of];
    let inverses = [part_of];

    let mut map: IndexMap<String, JsonValue> = IndexMap::new();
    for (predicate, objects) in subject.predicates() {
        map.insert(predicate, json!(objects));
    }
    let mut ancestors = graph.ancestors2(&iri, &inverses);
    // let mut ancestors = graph.ancestors2(&iri, &[]);
    let i = iri.to_string();
    ancestors.insert(&i);
    let parents = graph.parents2(&iri, &inverses);
    // println!("PARENTS {parents:?}");
    ancestors.extend(parents);

    let owl_type = subject.owl_type().unwrap_or(CLASS);

    let mut roots: Vec<&String> = graph
        .roots(owl_type)
        .into_iter()
        .filter(|s| ancestors.contains(s))
        .collect();

    // println!("ROOTS: {roots:?}");

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

    let (labels, curies, deprecated) = fill_signature(prefixes, graph, &signature);

    roots.sort_by(|a, b| labels.get_index_of(*a).cmp(&labels.get_index_of(*b)));
    children.sort_by(|(_, a), (_, b)| labels.get_index_of(a).cmp(&labels.get_index_of(b)));

    // Build an ancestor tree from roots by depth-first recursion into children.
    fn build_tree(
        graph: &IndexedMemoryGraph,
        ancestors: &BTreeSet<&String>,
        labels: &IndexMap<String, String>,
        restrictions: &[&str],
        edges: &Vec<(String, String)>,
    ) -> Vec<JsonValue> {
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
    // WARN: This relies on preserving order in the JSON JsonMap.
    fn insert_children(tree: &mut Vec<JsonValue>, iri: &str, children: &Vec<(String, String)>) {
        let first = match tree.first_mut() {
            Some(first) => first,
            None => return,
        };
        let first = match first.as_object_mut() {
            Some(first) => first,
            None => return,
        };
        if first.get("id").and_then(|v| v.as_str()) == Some(iri) {
            let branch: Vec<JsonValue> = children
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
            first.insert("focus".to_string(), JsonValue::Bool(true));
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

    let mut tree: Vec<JsonValue> = Vec::new();
    for owl_type in owl_types {
        let children: JsonValue = if subject.has_type(&owl_type) {
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
        },
        "deprecated": deprecated,
        "predicates": map,
        "tree": tree,
    })
    .as_object()
    .unwrap()
    .clone())
}

pub async fn get_term(
    rltbl: &Relatable,
    table_name: &str,
    iri: &str,
) -> Result<JsonMap<String, JsonValue>> {
    let sql = format!(
        r#"WITH RECURSIVE ancestors(child, parent) AS (
    SELECT '{iri}' AS child, object AS parent
    FROM "{table_name}"
    WHERE subject = '{iri}'
      AND predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
      AND datatype = '_ID'
    UNION
    SELECT subject, object
    FROM "{table_name}" AS statement
    JOIN ancestors ON statement.subject = ancestors.parent
    WHERE predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
      AND datatype = '_ID'
), children(child, parent) AS (
    SELECT subject AS child, object AS parent
    FROM "{table_name}"
    WHERE predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
      AND object = '{iri}'
      AND datatype = '_ID'
), grandchildren(child, parent) AS (
    SELECT subject AS child, object AS parent
    FROM "{table_name}"
    WHERE predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
      AND object IN (SELECT child FROM children)
      AND datatype = '_ID'
), instances(id) AS (
    SELECT subject
    FROM "{table_name}"
    WHERE predicate = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'
      AND object = '{iri}'
      AND datatype = '_ID'
), triples AS (
    SELECT *
    FROM "{table_name}"
    WHERE subject = '{iri}'
), ids AS (
    SELECT subject AS id FROM triples
    UNION
    SELECT predicate AS id FROM triples
    UNION
    SELECT object AS id FROM triples
    WHERE datatype == '_ID'
    UNION
    SELECT value AS id
    FROM triples, json_tree(triples.object)
    WHERE datatype = '_JSONLD'
      AND type = 'text'
    UNION
    SELECT datatype AS id FROM triples
    WHERE datatype NOT LIKE '_%'
      AND datatype NOT LIKE '@%'
    UNION
    SELECT child AS id FROM ancestors
    UNION
    SELECT parent AS id FROM ancestors
    UNION
    SELECT child AS id FROM children
    UNION
    SELECT child AS id FROM grandchildren
    UNION
    SELECT id FROM instances
)
SELECT * FROM triples
UNION
SELECT *
FROM "{table_name}"
WHERE subject IN (
    SELECT parent FROM ancestors
    UNION
    SELECT child FROM children
    UNION
    SELECT child FROM grandchildren
) AND predicate = 'http://www.w3.org/2000/01/rdf-schema#subClassOf'
  AND datatype = '_ID'
UNION
SELECT *
FROM "{table_name}"
WHERE subject IN (SELECT id FROM ids)
  AND subject != '{iri}'
  AND predicate IN (
    'http://www.w3.org/1999/02/22-rdf-syntax-ns#type',
    'http://www.w3.org/2000/01/rdf-schema#label'
);"#
    );

    let mut graph = MemoryGraph::new();
    // let rows = rltbl.pool.query(&sql, ()).await?;
    let rows = tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        rltbl.pool.query(&sql, ()),
    )
    .await??;
    for rows in rows
        .rows
        .chunk_by(|a, b| a.get("subject") == b.get("subject"))
    {
        let mut subj = Subject::new();
        for row in rows {
            let string_row: StringRow = row.into();
            // println!("{string_row:?}");
            let subject = string_row.get("subject").unwrap();
            let predicate = string_row.get("predicate").unwrap();
            let object = string_row.get("object").unwrap();
            let datatype = string_row.get("datatype").unwrap();

            if subj.name().is_empty() {
                subj.set_name(subject);
            }
            let obj = match Object::try_from(object, datatype) {
                Ok(obj) => obj,
                Err(err) => {
                    println!("WARN: could not parse '{object}' as '{datatype}' because {err}");
                    continue;
                }
            };
            subj.insert(predicate, obj);
            // if let Ok(obj) = Object::try_from(object, datatype) {
            //     subj.insert(predicate, obj);
            // } else {
            //     println!("WARN: could not parse '{object}' as '{datatype}'");
            // }
        }
        graph.insert(subj);
    }

    let graph = IndexedMemoryGraph::from(graph);
    println!("IRI {iri}");
    let subject = graph.get(iri).unwrap();
    get_term_inner(&PREFIXES, &graph, subject, iri)
}

#[cfg(test)]
mod tests {
    use crate::core::RelatableBuilder;

    use super::*;

    #[ignore]
    #[tokio::test]
    async fn test_tree_get_term() -> Result<()> {
        let rltbl = RelatableBuilder::test("test_tree_get_term")
            .init(true)
            .await?;

        let dataset = crate::demo::vaccine::Dataset::generate(1, 1);
        dataset.build(&rltbl).await?;

        let iri = "http://purl.obolibrary.org/obo/BFO_0000040".to_string();

        let _actual = get_term(&rltbl, "statement", &iri).await?;
        // println!("{actual:#?}");

        // assert!(false, "SUCCESS!!");
        Ok(())
    }
}
