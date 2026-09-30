use crate::Result;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, Deserialize)]
pub struct EntityDefinition {
    pub source_identifier: String,
    pub models: BTreeMap<String, String>,
    pub textures: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct EntityCatalog {
    pub format: u32,
    pub source: Value,
    pub entities: BTreeMap<String, EntityDefinition>,
    pub models: BTreeMap<String, Value>,
}

pub fn entity_catalog() -> Result<&'static EntityCatalog> {
    static CATALOG: OnceLock<Result<EntityCatalog>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            serde_json::from_slice(include_bytes!("entities.json"))
                .map_err(|e| format!("Invalid bundled entity models: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}
