use crate::{Result, versions::MIN_JAVA_DATA_VERSION};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

pub const REVISION: &str = "8ffb321c74cffe779acf5c447d08c473c4c291d7";
pub(super) const JSON_LIMIT: u64 = 32 * 1024 * 1024;
pub(super) const KINDS: [&str; 4] = ["blocks", "items", "entities", "blockCollisionShapes"];

pub(crate) fn safe_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains(['\\', ':', '?', '#', '%'])
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("Invalid data path {value:?}"));
    }
    Ok(path)
}

pub(super) fn url(relative: &str) -> Result<String> {
    safe_path(relative)?;
    Ok(format!(
        "https://raw.githubusercontent.com/PrismarineJS/minecraft-data/{REVISION}/data/{relative}"
    ))
}

pub(super) fn json(relative: &str, bytes: &[u8]) -> Result<Value> {
    if bytes.len() as u64 > JSON_LIMIT {
        return Err(format!("Catalog dataset exceeds 32 MiB: {relative}"));
    }
    serde_json::from_slice(bytes).map_err(|e| format!("{relative}: {e}"))
}

#[derive(Debug)]
pub(super) struct Metadata {
    paths: BTreeMap<String, BTreeMap<String, String>>,
    versions: Vec<Value>,
}

impl Metadata {
    pub fn parse(paths: Value, versions: Value) -> Result<Self> {
        Ok(Self {
            paths: serde_json::from_value(paths["pc"].clone())
                .map_err(|e| format!("Invalid Java data paths: {e}"))?,
            versions: serde_json::from_value(versions)
                .map_err(|e| format!("Invalid version metadata: {e}"))?,
        })
    }

    pub fn version_for_data_version(&self, id: i32) -> Option<String> {
        self.versions
            .iter()
            .find(|v| v["dataVersion"].as_i64() == Some(i64::from(id)))
            .and_then(|v| v["minecraftVersion"].as_str())
            .map(str::to_owned)
    }

    pub fn resolve(&self, requested: &str) -> Result<String> {
        if requested != "latest" {
            return Ok(requested.into());
        }
        self.versions
            .iter()
            .filter(|v| v["releaseType"] == "release")
            .filter_map(|v| v["minecraftVersion"].as_str())
            .find(|v| self.paths.contains_key(*v))
            .map(str::to_owned)
            .ok_or_else(|| "No release catalog in minecraft-data".into())
    }

    pub fn data_version(&self, version: &str) -> Result<i32> {
        self.versions
            .iter()
            .find(|v| v["minecraftVersion"].as_str() == Some(version))
            .and_then(|v| v["dataVersion"].as_i64())
            .and_then(|v| i32::try_from(v).ok())
            .filter(|v| *v >= MIN_JAVA_DATA_VERSION)
            .filter(|_| self.paths.contains_key(version))
            .ok_or_else(|| {
                format!(
                    "Unsupported Java version {version}; expected a catalog for Java 1.13 or later"
                )
            })
    }

    pub fn versions(&self) -> Vec<String> {
        self.paths
            .keys()
            .filter(|v| self.data_version(v).is_ok())
            .cloned()
            .collect()
    }

    pub fn dataset(&self, version: &str, kind: &str) -> Result<String> {
        if !KINDS.contains(&kind) {
            return Err(format!("Unsupported catalog dataset {kind:?}"));
        }
        let dir = self
            .paths
            .get(version)
            .and_then(|p| p.get(kind))
            .ok_or_else(|| format!("No {kind} dataset for Java {version}"))?;
        let path = format!("{dir}/{kind}.json");
        safe_path(&path)?;
        Ok(path)
    }
}
