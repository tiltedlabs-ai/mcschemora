use crate::{Result, mc_data::Cache, model::Block};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

pub const VISUAL_VERSION: &str = "1.21.1";

pub(crate) const MIN_DATA_VERSION: i32 = 1519;

#[derive(Debug)]
pub struct MinecraftData {
    cache: Cache,
    metadata: OnceLock<Metadata>,
    legacy: OnceLock<BTreeMap<String, String>>,
    catalogs: Mutex<BTreeMap<String, Arc<Registry>>>,
}

#[derive(Debug)]
struct Metadata {
    paths: BTreeMap<String, BTreeMap<String, String>>,
    versions: Vec<Value>,
}

impl Metadata {
    fn record(&self, version: &str) -> Option<&Value> {
        self.versions
            .iter()
            .find(|v| v["minecraftVersion"].as_str() == Some(version))
    }
}

impl MinecraftData {
    pub fn new(cache_dir: Option<PathBuf>, offline: bool) -> Result<Self> {
        Ok(Self {
            cache: Cache::new(cache_dir, offline)?,
            metadata: OnceLock::new(),
            legacy: OnceLock::new(),
            catalogs: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache.root
    }

    fn metadata(&self) -> Result<&Metadata> {
        if self.metadata.get().is_none() {
            let manifest = self.cache.json(&self.cache.catalog("dataPaths.json")?)?;
            let paths = serde_json::from_value(manifest["pc"].clone())
                .map_err(|e| format!("Invalid Java data paths: {e}"))?;
            let versions = serde_json::from_value(
                self.cache
                    .json(&self.cache.catalog("pc/common/protocolVersions.json")?)?,
            )
            .map_err(|e| format!("Invalid version metadata: {e}"))?;
            let _ = self.metadata.set(Metadata { paths, versions });
        }
        Ok(self.metadata.get().unwrap())
    }

    pub(crate) fn legacy(&self) -> Result<&BTreeMap<String, String>> {
        if self.legacy.get().is_none() {
            let legacy = serde_json::from_value(
                self.cache
                    .json(&self.cache.catalog("pc/common/legacy.json")?)?["blocks"]
                    .clone(),
            )
            .map_err(|e| format!("Invalid legacy mappings: {e}"))?;
            let _ = self.legacy.set(legacy);
        }
        Ok(self.legacy.get().unwrap())
    }

    pub fn versions(&self) -> Result<Vec<String>> {
        let metadata = self.metadata()?;
        Ok(metadata
            .paths
            .keys()
            .filter(|v| {
                metadata
                    .record(v)
                    .and_then(|v| v["dataVersion"].as_i64())
                    .is_some_and(|n| n >= i64::from(MIN_DATA_VERSION))
            })
            .cloned()
            .collect())
    }

    pub fn version_for_data_version(&self, id: i32) -> Result<Option<String>> {
        Ok(self
            .metadata()?
            .versions
            .iter()
            .find(|v| v["dataVersion"].as_i64() == Some(i64::from(id)))
            .and_then(|v| v["minecraftVersion"].as_str())
            .map(str::to_owned))
    }

    fn latest(&self) -> Result<String> {
        let metadata = self.metadata()?;
        metadata
            .versions
            .iter()
            .filter(|v| v["releaseType"] == "release")
            .filter_map(|v| v["minecraftVersion"].as_str())
            .find(|v| metadata.paths.contains_key(*v))
            .map(str::to_owned)
            .ok_or_else(|| "No release catalog in minecraft-data".into())
    }

    pub fn dataset_path(&self, requested: &str, kind: &str) -> Result<PathBuf> {
        if !matches!(
            kind,
            "blocks" | "items" | "entities" | "blockCollisionShapes"
        ) {
            return Err(format!("Unsupported catalog dataset {kind:?}"));
        }
        let version = if requested == "latest" {
            self.latest()?
        } else {
            requested.to_owned()
        };
        let metadata = self.metadata()?;
        let paths = metadata
            .paths
            .get(&version)
            .ok_or_else(|| format!("No catalog for Java {version}"))?;
        let dir = paths
            .get(kind)
            .ok_or_else(|| format!("Java {version} has no {kind} data"))?;
        crate::mc_data::safe_path(dir)?;
        self.cache.catalog(&format!("{dir}/{kind}.json"))
    }

    fn dataset(&self, version: &str, kind: &str) -> Result<Value> {
        self.cache.json(&self.dataset_path(version, kind)?)
    }

    pub(crate) fn collision_shapes(&self, version: &str) -> Result<Value> {
        self.dataset(version, "blockCollisionShapes")
    }

    pub fn fetch(&self, requested: &str, visuals: bool) -> Result<String> {
        for path in [
            "dataPaths.json",
            "pc/common/protocolVersions.json",
            "pc/common/legacy.json",
        ] {
            self.cache.catalog(path)?;
        }
        let catalog = self.registry(requested)?;
        for kind in ["blocks", "items", "entities", "blockCollisionShapes"] {
            self.dataset_path(&catalog.version, kind)?;
        }
        if visuals {
            self.visuals(&catalog.version)?;
        }
        Ok(catalog.version.clone())
    }

    pub fn visuals(&self, requested: &str) -> Result<PathBuf> {
        let version = if requested == "latest" {
            self.latest()?
        } else {
            requested.to_owned()
        };
        if version != VISUAL_VERSION {
            eprintln!(
                "warning: visuals for Java {version} are not supported; falling back to Java {VISUAL_VERSION} textures and models"
            );
        }
        self.cache.visuals(VISUAL_VERSION, Some("1.21"))
    }

    pub fn registry(&self, requested: &str) -> Result<Arc<Registry>> {
        let version = if requested == "latest" {
            self.latest()?
        } else {
            requested.into()
        };
        let metadata = self.metadata()?;
        if metadata
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
        metadata.paths.get(&version).ok_or_else(|| format!("No catalog for Java {version} in the pinned minecraft-data snapshot. Choose a version from MinecraftData.versions."))?;
        let data_version = metadata
            .record(&version)
            .and_then(|v| v["dataVersion"].as_i64())
            .and_then(|n| i32::try_from(n).ok())
            .ok_or("Invalid Minecraft data version")?;
        let blocks = index(self.dataset(&version, "blocks")?)?
            .into_iter()
            .map(|(name, value)| Ok((name, BlockSchema::parse(&value)?)))
            .collect::<Result<_>>()?;
        let items = index(self.dataset(&version, "items")?)?
            .into_keys()
            .collect();
        let mobs = index(self.dataset(&version, "entities")?)?
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
        for (key, value) in self.legacy()? {
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
