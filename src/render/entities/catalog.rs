use crate::Result;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Debug, Deserialize)]
pub(super) struct EntityDefinition {
    pub model: String,
    pub texture: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct EntityCatalog {
    pub entities: BTreeMap<String, EntityDefinition>,
    pub models: BTreeMap<String, Value>,
}

impl EntityCatalog {
    pub(super) fn bundled() -> Result<&'static Self> {
        static CATALOG: LazyLock<Result<EntityCatalog>> = LazyLock::new(|| {
            let catalog: EntityCatalog =
                serde_json::from_str(include_str!("../../../data/entity-models/entities.json"))
                    .map_err(|e| format!("Invalid bundled entity catalog: {e}"))?;
            if catalog
                .entities
                .values()
                .any(|e| !catalog.models.contains_key(&e.model))
            {
                return Err("Missing bundled entity model".into());
            }
            Ok(catalog)
        });
        CATALOG.as_ref().map_err(Clone::clone)
    }
}
