use crate::{Result, model::Block};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn index(value: Value) -> Result<BTreeMap<String, Value>> {
    let array = value.as_array().ok_or("Catalog must be an array")?;
    array
        .iter()
        .map(|v| {
            let name = v["name"].as_str().ok_or("Catalog entry has no name")?;
            Ok((namespace(name), v.clone()))
        })
        .collect()
}

/// Block schemas, item IDs, and living-mob IDs for one Java version.
#[derive(Debug)]
pub struct Registry {
    /// Resolved Java version string.
    pub version: String,
    /// Numeric Minecraft data version for this registry.
    pub data_version: i32,
    blocks: BTreeMap<String, BlockSchema>,
    items: BTreeSet<String>,
    mobs: BTreeSet<String>,
    entities: BTreeSet<String>,
    pub(crate) validation_shapes: std::sync::OnceLock<crate::validate::Shapes>,
}

/// Adds minecraft: when an identifier has no namespace.
pub fn namespace(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn values(state: &Value) -> Result<Vec<String>> {
    if let Some(values) = state["values"].as_array() {
        return serde_json::from_value(Value::Array(values.clone()))
            .map_err(|e| format!("Invalid block state values: {e}"));
    }
    if state["type"] == "bool" {
        return Ok(vec!["true".into(), "false".into()]);
    }
    let count = state["num_values"]
        .as_u64()
        .ok_or("Missing block state values")?;
    if count > 65536 {
        return Err("Block state has too many values".into());
    }
    Ok((0..count).map(|n| n.to_string()).collect())
}

#[derive(Debug, Serialize)]
struct Property {
    values: Vec<String>,
    default: String,
}

#[derive(Debug)]
struct BlockSchema {
    numeric_id: Value,
    display_name: String,
    properties: BTreeMap<String, Property>,
    state_order: Vec<String>,
}
impl BlockSchema {
    fn parse(block: &Value) -> Result<Self> {
        let mut offset = block["defaultState"]
            .as_u64()
            .ok_or("Missing default state")?
            .checked_sub(
                block["minStateId"]
                    .as_u64()
                    .ok_or("Missing minimum state")?,
            )
            .ok_or("Invalid default state")?;
        let mut properties = BTreeMap::new();
        for state in block["states"]
            .as_array()
            .ok_or("Missing block states")?
            .iter()
            .rev()
        {
            let values = values(state)?;
            if values.is_empty() {
                return Err("Block state has no values".into());
            }
            let default = values[offset as usize % values.len()].clone();
            offset /= values.len() as u64;
            properties.insert(
                state["name"].as_str().ok_or("Missing state name")?.into(),
                Property { values, default },
            );
        }
        Ok(Self {
            numeric_id: block["id"].clone(),
            display_name: block["displayName"]
                .as_str()
                .ok_or("Missing block display name")?
                .into(),
            properties,
            state_order: block["states"]
                .as_array()
                .unwrap()
                .iter()
                .map(|state| state["name"].as_str().unwrap().to_owned())
                .collect(),
        })
    }
}

impl Registry {
    pub(crate) fn block_names(&self) -> impl Iterator<Item = &str> {
        self.blocks.keys().map(String::as_str)
    }

    pub(crate) fn block_states(&self, id: &str) -> Result<Vec<Block>> {
        let schema = self.schema(id)?;
        let mut states = vec![Block::parse(id)?];
        for (key, property) in &schema.properties {
            let mut expanded = Vec::new();
            for state in states {
                for value in &property.values {
                    let mut state = state.clone();
                    state.properties.insert(key.clone(), value.clone());
                    expanded.push(state);
                }
            }
            states = expanded;
        }
        Ok(states)
    }

    pub(crate) fn block_display_name(&self, id: &str) -> Result<&str> {
        Ok(&self.schema(id)?.display_name)
    }
    /// Offset and count in the catalog's original state order, not property key order.
    pub(crate) fn state_offset(&self, block: &Block) -> Option<(usize, usize)> {
        let schema = self.schema(&block.name).ok()?;
        let (mut offset, mut count) = (0usize, 1usize);
        for key in &schema.state_order {
            let property = schema.properties.get(key)?;
            let value = block.properties.get(key).unwrap_or(&property.default);
            offset = offset
                .checked_mul(property.values.len())?
                .checked_add(property.values.iter().position(|v| v == value)?)?;
            count = count.checked_mul(property.values.len())?;
        }
        Some((offset, count))
    }
    fn schema(&self, name: &str) -> Result<&BlockSchema> {
        self.blocks
            .get(name)
            .ok_or_else(|| format!("Unknown block {name} in Java {}", self.version))
    }
    /// Returns a block schema with id, properties, version, and numeric_id.
    pub fn describe(&self, id: &str) -> Result<Value> {
        let name = namespace(id);
        let schema = self.schema(&name)?;
        Ok(json!({"id": name, "properties": schema.properties,
            "version": self.version, "numeric_id": schema.numeric_id}))
    }
    /// Validates supplied block properties and fills omitted properties with defaults.
    pub fn resolve(&self, block: &Block) -> Result<Block> {
        let props = &self.schema(&block.name)?.properties;
        for (key, value) in &block.properties {
            let property = props.get(key).ok_or_else(|| {
                format!(
                    "{}: unknown property {key} in Java {}; allowed: {:?}",
                    block.name,
                    self.version,
                    props.keys().collect::<Vec<_>>()
                )
            })?;
            if !property.values.contains(value) {
                return Err(format!(
                    "{}: invalid {key}={value}; allowed: {:?}",
                    block.name, property.values
                ));
            }
        }
        let mut result = block.clone();
        for (key, prop) in props {
            result
                .properties
                .entry(key.clone())
                .or_insert_with(|| prop.default.clone());
        }
        Ok(result)
    }

    /// Checks that an item identifier exists in this version.
    pub fn item(&self, id: &str) -> Result<()> {
        if self.items.contains(&namespace(id)) {
            Ok(())
        } else {
            Err(format!("Unknown item {id} in Java {}", self.version))
        }
    }
    pub(crate) fn entity_id(&self, id: &str) -> Result<()> {
        if self.entities.contains(&namespace(id)) {
            Ok(())
        } else {
            Err(format!("Unknown entity {id} in Java {}", self.version))
        }
    }

    /// Validates a living-mob identifier and returns its namespaced form.
    pub fn mob_id(&self, id: &str) -> Result<String> {
        let name = namespace(id);
        if self.mobs.contains(&name) {
            Ok(name)
        } else {
            Err(format!("Unknown living mob {id} in Java {}", self.version))
        }
    }
}

fn is_mob(entry: &Value) -> bool {
    let category = entry["category"].as_str().unwrap_or("");
    let kind = entry["type"].as_str().unwrap_or("");
    matches!(
        kind,
        "animal" | "living" | "hostile" | "passive" | "water_creature" | "ambient"
    ) || (kind == "mob"
        && (category.contains("mob")
            || matches!(
                category,
                "Hostile mobs" | "Passive mobs" | "NPCs" | "Animals" | "Monsters" | "Ambient"
            )))
}

impl Registry {
    pub(super) fn parse(
        version: String,
        data_version: i32,
        blocks: Value,
        items: Value,
        entities: Value,
    ) -> Result<Self> {
        let blocks = index(blocks)?
            .into_iter()
            .map(|(name, value)| Ok((name, BlockSchema::parse(&value)?)))
            .collect::<Result<_>>()?;
        let items = index(items)?.into_keys().collect();
        let entity_data = index(entities)?;
        let entities = entity_data.keys().cloned().collect();
        let mobs = entity_data
            .into_iter()
            .filter(|(_, entry)| is_mob(entry))
            .map(|(name, _)| name)
            .collect();
        Ok(Registry {
            version,
            data_version,
            blocks,
            items,
            mobs,
            entities,
            validation_shapes: std::sync::OnceLock::new(),
        })
    }
}
