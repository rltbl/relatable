use indexmap::IndexMap;
use itertools::Itertools;

#[derive(Clone, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct Slot {
    pub name: String,
    pub identifier: bool,
    pub description: String,
    pub range: String,
}

impl Slot {
    pub fn render(&self) -> String {
        let mut lines = vec![format!("  {}:", self.name)];
        if self.identifier {
            lines.push("    identifier: true".to_string());
        }
        if !self.description.is_empty() {
            lines.push(format!("    description: {}", self.description))
        }
        if !self.range.is_empty() {
            lines.push(format!("    range: {}", self.range))
        }
        lines.join("\n")
    }

    pub fn identifier(mut self, flag: bool) -> Self {
        self.identifier = flag;
        self
    }

    pub fn range(mut self, range: &str) -> Self {
        self.range = range.to_string();
        self
    }
}

impl From<&str> for Slot {
    fn from(value: &str) -> Self {
        Self {
            name: value.to_string(),
            ..Default::default()
        }
    }
}

pub struct Slots {
    pub slots: IndexMap<String, Slot>,
}

impl Slots {
    pub fn render(&self) -> String {
        let slots: Vec<String> = self.slots.values().map(|e| e.render()).collect();
        let slots = slots.join("\n");
        format!("slots:\n{slots}")
    }
}

impl From<Vec<Slot>> for Slots {
    fn from(slots: Vec<Slot>) -> Self {
        Self {
            slots: slots.into_iter().map(|s| (s.name.clone(), s)).collect(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Class {
    pub name: String,
    pub description: String,
    pub slots: Vec<Slot>,
}

impl Class {
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    pub fn slot(mut self, slot: Slot) -> Self {
        self.slots.push(slot);
        self
    }

    pub fn slots(mut self, slots: Vec<impl Into<Slot>>) -> Self {
        self.slots = slots.into_iter().map(|s| s.into()).collect();
        self
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!("  {}:", self.name)];
        if !self.description.is_empty() {
            lines.push(format!("    description: {}", self.description))
        }
        lines.push("    slots:".to_string());
        for slot in &self.slots {
            lines.push(format!("      - {}", slot.name))
        }
        lines.join("\n")
    }
}

impl From<&str> for Class {
    fn from(value: &str) -> Self {
        Self {
            name: value.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Classes {
    classes: IndexMap<String, Class>,
}

impl Classes {
    //
    pub fn slots(&self) -> Slots {
        let slots: Vec<Slot> = self
            .classes
            .values()
            .flat_map(|c| c.slots.clone())
            .unique()
            .collect();
        slots.into()
    }

    pub fn render(&self) -> String {
        let classes: Vec<String> = self.classes.values().map(|e| e.render()).collect();
        let classes = classes.join("\n");
        format!("classes:\n{classes}")
    }
}

impl From<Vec<Class>> for Classes {
    fn from(classes: Vec<Class>) -> Self {
        Self {
            classes: classes.into_iter().map(|s| (s.name.clone(), s)).collect(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Value {
    pub name: String,
    pub meaning: String,
    pub description: String,
}

impl Value {
    pub fn render(&self) -> String {
        let mut lines = vec![format!("      {}:", self.name)];
        if !self.meaning.is_empty() {
            lines.push(format!("meaning: {}", self.meaning))
        }
        if !self.description.is_empty() {
            lines.push(format!("description: {}", self.description))
        }
        lines.join("\n        ")
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self {
            name: value.to_string(),
            ..Default::default()
        }
    }
}

impl From<String> for Value {
    fn from(name: String) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Enum {
    pub name: String,
    pub description: String,
    pub permissible_values: Vec<Value>,
}

impl Enum {
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    pub fn value(mut self, value: Value) -> Self {
        self.permissible_values.push(value);
        self
    }

    pub fn values(mut self, values: Vec<impl Into<Value>>) -> Self {
        self.permissible_values = values.into_iter().map(|v| v.into()).collect();
        self
    }

    pub fn render(&self) -> String {
        let mut lines = vec![format!("  {}:", self.name)];
        if !self.description.is_empty() {
            lines.push(format!("    description: {}", self.description))
        }
        lines.push("    permissible_values:".to_string());
        for value in &self.permissible_values {
            lines.push(value.render())
        }
        lines.join("\n")
    }
}

impl From<&str> for Enum {
    fn from(value: &str) -> Self {
        Self {
            name: value.to_string(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Enums {
    enums: IndexMap<String, Enum>,
}

impl Enums {
    pub fn render(&self) -> String {
        let enums: Vec<String> = self.enums.values().map(|e| e.render()).collect();
        let enums = enums.join("\n");
        format!("enums:\n{enums}")
    }
}

impl From<Vec<Enum>> for Enums {
    fn from(enums: Vec<Enum>) -> Self {
        Self {
            enums: enums.into_iter().map(|e| (e.name.clone(), e)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use indexmap::indexmap;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_serialize() {
        let e = Enum {
            name: "Foo".to_string(),
            description: "bar".to_string(),
            permissible_values: vec![Value::from("baz")],
        };
        let enums = Enums {
            enums: indexmap! { "Foo".to_string() => e},
        };
        let expected = r#"enums:
  Foo:
    description: bar
    permissible_values:
      baz:"#;
        let actual = enums.render();
        assert_eq!(expected, actual);
    }
}
