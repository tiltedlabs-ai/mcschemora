//! Runtime access to a minecraft-data checkout. Catalogs are cached per version.
use crate::{Result, model::Block};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(crate) const MIN_DATA_VERSION: i32 = 1519; // Java 1.13.

#[derive(Debug)]
pub struct MinecraftData {
    root: PathBuf,
    paths: BTreeMap<String, BTreeMap<String, String>>,
    versions: Vec<Value>,
    pub(crate) legacy: BTreeMap<String, String>,
    catalogs: Mutex<BTreeMap<String, Arc<Registry>>>,
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

impl MinecraftData {
    /// Accept either the repository root or its data directory.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let root = if path.join("dataPaths.json").is_file() {
            path.to_path_buf()
        } else {
            path.join("data")
        };
        let manifest = read_json(&root.join("dataPaths.json"))
            .map_err(|e| format!("Cannot load minecraft-data: {e}. Initialize the submodule or provide a checkout path."))?;
        let paths = serde_json::from_value(manifest["pc"].clone())
            .map_err(|e| format!("Invalid Java data paths: {e}"))?;
        let versions =
            serde_json::from_value(read_json(&root.join("pc/common/protocolVersions.json"))?)
                .map_err(|e| format!("Invalid version metadata: {e}"))?;
        let legacy = serde_json::from_value(
            read_json(&root.join("pc/common/legacy.json"))?["blocks"].clone(),
        )
        .map_err(|e| format!("Invalid legacy mappings: {e}"))?;
        Ok(Self {
            root,
            paths,
            versions,
            legacy,
            catalogs: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn versions(&self) -> Vec<String> {
        self.paths
            .keys()
            .filter(|v| {
                self.record(v)
                    .and_then(|v| v["dataVersion"].as_i64())
                    .is_some_and(|n| n >= i64::from(MIN_DATA_VERSION))
            })
            .cloned()
            .collect()
    }

    pub fn version_for_data_version(&self, id: i32) -> Option<String> {
        self.versions
            .iter()
            .find(|v| v["dataVersion"].as_i64() == Some(id as i64))
            .and_then(|v| v["minecraftVersion"].as_str())
            .map(str::to_owned)
    }

    fn record(&self, version: &str) -> Option<&Value> {
        self.versions
            .iter()
            .find(|v| v["minecraftVersion"].as_str() == Some(version))
    }

    fn latest(&self) -> Result<String> {
        self.versions
            .iter()
            .filter(|v| v["releaseType"] == "release")
            .filter_map(|v| v["minecraftVersion"].as_str())
            .find(|v| self.paths.contains_key(*v))
            .map(str::to_owned)
            .ok_or_else(|| "No release catalog in minecraft-data".into())
    }

    fn dataset(&self, paths: &BTreeMap<String, String>, kind: &str) -> Result<Value> {
        let dir = paths
            .get(kind)
            .ok_or_else(|| format!("This catalog has no {kind} data"))?;
        // Manifest paths are relative to the data directory.
        if Path::new(dir)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(format!("Invalid minecraft-data path {dir:?}"));
        }
        read_json(&self.root.join(dir).join(format!("{kind}.json")))
    }

    pub fn registry(&self, requested: &str) -> Result<Arc<Registry>> {
        let version = if requested == "latest" {
            self.latest()?
        } else {
            requested.into()
        };
        if self
            .record(&version)
            .and_then(|v| v["dataVersion"].as_i64())
            .is_none_or(|n| n < i64::from(MIN_DATA_VERSION))
        {
            return Err(format!(
                "Unsupported Java version {version}; Schemora requires 1.13 or later with version metadata in minecraft-data."
            ));
        }
        let mut catalogs = self.catalogs.lock().map_err(|_| "Catalog lock poisoned")?;
        if let Some(catalog) = catalogs.get(&version) {
            return Ok(catalog.clone());
        }
        let paths = self.paths.get(&version).ok_or_else(|| format!("No catalog for Java {version} in this minecraft-data checkout. Update the checkout or choose a version from MinecraftData.versions."))?;
        let data_version = self
            .record(&version)
            .and_then(|v| v["dataVersion"].as_i64())
            .and_then(|n| i32::try_from(n).ok())
            .ok_or("Invalid Minecraft data version")?;
        let blocks = index(self.dataset(paths, "blocks")?)?
            .into_iter()
            .map(|(name, value)| Ok((name, BlockSchema::parse(&value)?)))
            .collect::<Result<_>>()?;
        let items = index(self.dataset(paths, "items")?)?.into_keys().collect();
        let mobs = index(self.dataset(paths, "entities")?)?
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
        };
        for (key, value) in &self.legacy {
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
        let catalog = Arc::new(catalog);
        catalogs.insert(version, catalog.clone());
        Ok(catalog)
    }
}

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
    properties: BTreeMap<String, Property>,
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
            properties,
        })
    }
}

impl Registry {
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
