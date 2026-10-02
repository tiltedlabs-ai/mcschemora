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

#[derive(Debug)]
pub struct Registry {
    pub version: String,
    pub data_version: i32,
    blocks: BTreeMap<String, BlockSchema>,
    items: BTreeSet<String>,
    mobs: BTreeSet<String>,
    legacy_reverse: BTreeMap<Block, (u16, u8)>,
    pub(crate) validation_shapes: std::sync::OnceLock<crate::validate::Shapes>,
}

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
    pub fn describe(&self, id: &str) -> Result<Value> {
        let name = namespace(id);
        let schema = self.schema(&name)?;
        Ok(json!({"id": name, "properties": schema.properties,
            "version": self.version, "numeric_id": schema.numeric_id}))
    }
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

    pub fn item(&self, id: &str) -> Result<()> {
        if self.items.contains(&namespace(id)) {
            Ok(())
        } else {
            Err(format!("Unknown item {id} in Java {}", self.version))
        }
    }
    pub fn mob_id(&self, id: &str) -> Result<String> {
        let name = namespace(id);
        if self.mobs.contains(&name) {
            Ok(name)
        } else {
            Err(format!("Unknown living mob {id} in Java {}", self.version))
        }
    }

    pub fn legacy_pair(&self, block: &Block) -> Result<(u16, u8)> {
        self.legacy_reverse
            .get(block)
            .copied()
            .ok_or_else(|| format!("No legacy mapping for {}", block.text()))
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
        legacy: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let blocks = index(blocks)?
            .into_iter()
            .map(|(name, value)| Ok((name, BlockSchema::parse(&value)?)))
            .collect::<Result<_>>()?;
        let items = index(items)?.into_keys().collect();
        let mobs = index(entities)?
            .into_iter()
            .filter(|(_, entry)| is_mob(entry))
            .map(|(name, _)| name)
            .collect();
        let mut catalog = Registry {
            version: version.clone(),
            data_version,
            blocks,
            items,
            mobs,
            legacy_reverse: BTreeMap::new(),
            validation_shapes: std::sync::OnceLock::new(),
        };
        for (key, value) in legacy {
            let (id, meta) = key
                .split_once(':')
                .ok_or_else(|| format!("Invalid legacy mapping key {key}"))?;
            let pair = (
                id.parse::<u16>().map_err(|_| "Invalid legacy block ID")?,
                meta.parse::<u8>()
                    .map_err(|_| "Invalid legacy block metadata")?,
            );
            let raw = Block::parse(value)?;
            let block = catalog.resolve(&raw).unwrap_or(raw);
            catalog
                .legacy_reverse
                .entry(block)
                .and_modify(|old| *old = (*old).min(pair))
                .or_insert(pair);
        }
        Ok(catalog)
    }
}
