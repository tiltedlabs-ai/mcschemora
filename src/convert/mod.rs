mod attributes;
mod block_entities;
mod blocks;
mod commands;
mod component_changes;
mod components;
mod defaults;
mod effects;
mod entities;
mod entity_changes;
mod equipment;
mod game_events;
mod item_variants;
mod items;
mod legacy_items;
mod maps;
mod modern;
mod particles;
mod potions;
mod pottery;
mod profiles;
mod signs;
mod spatial;
mod spawners;
mod text;
mod tooltips;
mod uuids;
mod villagers;

use crate::{Result, catalog::Registry, model::*, nbt::Tag as V};
use serde::Deserialize;
use std::{
    borrow::Cow,
    cell::RefCell,
    sync::{Arc, OnceLock},
};

#[derive(Deserialize)]
struct Rename {
    data_version: i32,
    kinds: Vec<String>,
    from: String,
    to: String,
}

pub(super) struct Context {
    source: i32,
    source_registry: Arc<Registry>,
    path: RefCell<String>,
    losses: RefCell<Vec<String>>,
    target: Arc<Registry>,
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn palette<'a>(
    source: Option<Arc<Registry>>,
    target: Arc<Registry>,
    blocks: impl Iterator<Item = &'a Block>,
) -> Vec<Result<Block>> {
    let Some(source_registry) = source else {
        return blocks
            .map(|_| Err("No source catalog available for rendering conversion".into()))
            .collect();
    };
    let context = Context {
        source: source_registry.data_version,
        source_registry,
        path: RefCell::new(String::new()),
        losses: RefCell::new(Vec::new()),
        target,
    };
    blocks
        .map(|block| {
            if context.source >= 2503 && block.id.ends_with("_wall") {
                let mut block = block.clone();
                for key in ["north", "south", "east", "west"] {
                    if let Some(value) = block.properties.get_mut(key) {
                        match value.as_str() {
                            "false" => *value = "none".into(),
                            "true" => *value = "low".into(),
                            _ => {}
                        }
                    }
                }
                blocks::state(&block, &context)
            } else {
                blocks::state(block, &context)
            }
            .and_then(|block| context.target.resolve(&block))
        })
        .collect()
}

impl Context {
    fn loss(&self, field: &str, reason: &str) {
        self.losses.borrow_mut().push(format!(
            "{}.{} [{} → {}]: {}",
            self.path.borrow(),
            field,
            self.source,
            self.target.data_version,
            reason
        ));
    }

    fn scoped<T>(&self, field: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let previous = self.path.borrow().clone();
        self.path.replace(format!("{previous}.{field}"));
        let result = operation();
        self.path.replace(previous);
        result
    }

    fn forward(&self) -> bool {
        self.source < self.target.data_version
    }

    fn crosses(&self, boundary: i32) -> bool {
        self.source.min(self.target.data_version) < boundary
            && boundary <= self.source.max(self.target.data_version)
    }

    fn legacy(&self) -> bool {
        self.source >= crate::versions::ITEM_COMPONENTS
            && self.target.data_version < crate::versions::ITEM_COMPONENTS
    }

    fn components(&self) -> bool {
        self.source < crate::versions::ITEM_COMPONENTS
            && self.target.data_version >= crate::versions::ITEM_COMPONENTS
    }

    fn rename(&self, kind: &str, id: &str) -> Result<String> {
        static RENAMES: OnceLock<std::result::Result<Vec<Rename>, String>> = OnceLock::new();
        let rules = RENAMES.get_or_init(|| {
            serde_json::from_str(include_str!("data/renames.json")).map_err(|e| e.to_string())
        });
        let rules = rules.as_ref().map_err(Clone::clone)?;
        let mut id = crate::catalog::namespace(id);
        feature(kind, &id, self.source)?;
        let forward = self.source < self.target.data_version;
        let apply = |rule: &Rename, id: &mut String| {
            if !rule.kinds.iter().any(|k| k == kind) {
                return;
            }
            if forward
                && self.source < rule.data_version
                && rule.data_version <= self.target.data_version
                && *id == rule.from
            {
                *id = rule.to.clone();
            } else if !forward
                && self.target.data_version < rule.data_version
                && rule.data_version <= self.source
                && *id == rule.to
            {
                *id = rule.from.clone();
            }
        };
        if forward {
            for rule in rules {
                apply(rule, &mut id);
            }
        } else {
            for rule in rules.iter().rev() {
                apply(rule, &mut id);
            }
        }
        feature(kind, &id, self.target.data_version)?;
        Ok(id)
    }
}

pub(crate) struct Prepared<'a> {
    pub schematic: Cow<'a, Schematic>,
    pub errors: Vec<String>,
    pub losses: Vec<String>,
}

#[derive(Deserialize)]
struct Release {
    version: String,
    data_version: i32,
}

impl Release {
    async fn load(&self, data: &crate::catalog::MinecraftData) -> Result<Arc<Registry>> {
        let registry = data.load(&self.version).await?;
        if registry.data_version != self.data_version {
            return Err(format!(
                "Catalog data version for {} disagrees with conversion manifest",
                self.version
            ));
        }
        Ok(registry)
    }
}

fn releases() -> Result<&'static Vec<Release>> {
    static RELEASES: OnceLock<std::result::Result<Vec<Release>, String>> = OnceLock::new();
    RELEASES
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/releases.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(crate) async fn schematic<'a>(
    schematic: &'a Schematic,
    version: Option<&str>,
) -> Result<Prepared<'a>> {
    let unchanged = || Prepared {
        schematic: Cow::Borrowed(schematic),
        errors: Vec::new(),
        losses: Vec::new(),
    };
    let Some(version) = version else {
        return Ok(unchanged());
    };
    if schematic.edition != "java" {
        return Err("A target Minecraft version is supported only for Java schematics".into());
    }
    if schematic.data_version < crate::versions::MIN_JAVA_DATA_VERSION {
        return Err("Version conversion requires a known Java 1.13+ source data version".into());
    }
    if version.is_empty() || version == "latest" {
        return Err("Export requires an explicit target Minecraft version".into());
    }
    let manifest = releases()?;
    let target_release = manifest
        .iter()
        .find(|r| r.version == version)
        .ok_or_else(|| {
            format!("Minecraft {version} is outside the pinned stable conversion manifest")
        })?;
    if target_release.data_version == schematic.data_version {
        return Ok(unchanged());
    }
    let source_release = manifest
        .iter()
        .find(|r| r.data_version == schematic.data_version)
        .ok_or_else(|| {
            format!(
                "Source data version {} is outside the pinned stable conversion manifest",
                schematic.data_version
            )
        })?;
    let target = target_release.load(&schematic.data).await?;
    let mut converted = schematic.clone();
    let mut errors = Vec::new();
    let mut losses = Vec::new();
    let mut current = schematic.data_version;
    let mut source_registry = source_release.load(&schematic.data).await?;
    let forward = current < target.data_version;
    let mut destinations = Vec::new();
    for boundary in [
        3105, 3218, 3337, 3463, 3578, 3698, 3837, 3953, 4059, 4173, 4290, 4420, 4531, 4648, 4763,
        4903, 4996,
    ] {
        if current.min(target.data_version) < boundary
            && boundary <= current.max(target.data_version)
        {
            let split = manifest.partition_point(|release| release.data_version < boundary);
            let next = &manifest[if forward { split } else { split - 1 }];
            destinations.push(next.version.as_str());
        }
    }
    if !forward {
        destinations.reverse();
    }
    destinations.push(version);
    for destination in destinations {
        let release = manifest
            .iter()
            .find(|r| r.version == destination)
            .ok_or("conversion step has no release metadata")?;
        let next = if destination == version {
            target.clone()
        } else {
            release.load(&schematic.data).await?
        };
        if next.data_version == current {
            continue;
        }
        let context = Context {
            source: current,
            source_registry: source_registry.clone(),
            path: RefCell::new(String::new()),
            losses: RefCell::new(Vec::new()),
            target: next,
        };
        spatial::prepare(&converted, &context).await?;
        let transition = format!("{current} → {}", context.target.data_version);
        for (name, region) in &mut converted.regions {
            let mut palette = std::collections::BTreeMap::new();
            for state in region.blocks.states() {
                context
                    .path
                    .replace(format!("{name}.palette.{}", state.text()));
                match block_feature(&state.id, context.source)
                    .and_then(|()| blocks::state(state, &context))
                    .and_then(|next| {
                        block_feature(&next.id, context.target.data_version)?;
                        Ok(next)
                    }) {
                    Ok(next) => {
                        palette.insert(state.clone(), next);
                    }
                    Err(e) => errors.push(format!(
                        "{name}.palette.{} [{transition}]: {e}",
                        state.text()
                    )),
                }
            }
            let cells: Vec<_> = region
                .blocks
                .iter()
                .map(|(p, b)| (*p, palette.get(b).unwrap_or(b).clone()))
                .collect();
            region.blocks.clear();
            for (p, b) in cells {
                if b != Block::air() {
                    region.blocks.set(p, &b);
                }
            }
            region.block_entities.retain(|p, data| {
                context
                    .path
                    .replace(format!("{name}.block_entities[{p:?}]"));
                match block_entities::removed(data, &context) {
                    Ok(true) => false,
                    Ok(false) => {
                        if let Err(error) = block_entities::convert(data, &context, 0) {
                            errors.push(format!(
                                "{name}.block_entities[{p:?}] [{transition}].{error}"
                            ));
                        }
                        true
                    }
                    Err(error) => {
                        errors.push(format!(
                            "{name}.block_entities[{p:?}] [{transition}].{error}"
                        ));
                        true
                    }
                }
            });
            for e in &mut region.entities {
                context
                    .path
                    .replace(format!("{name}.entities[{}]", e.reference));
                if let Err(error) = entities::entity(&mut e.data, &context, 0) {
                    errors.push(format!(
                        "{name}.entities[{}] [{transition}].{error}",
                        e.reference
                    ));
                }
            }
            context.path.replace(format!("{name}.retained"));
            if let Err(error) = spatial::convert(region, &context) {
                errors.push(format!("{name}.retained [{transition}].{error}"));
            }
        }
        losses.extend(context.losses.take());
        if !errors.is_empty() {
            break;
        }
        current = context.target.data_version;
        source_registry = context.target;
    }
    errors.sort();
    errors.dedup();
    if errors.is_empty() {
        converted.version = target.version.clone();
        converted.data_version = target.data_version;
        converted.catalog = Some(target);
    }
    Ok(Prepared {
        schematic: Cow::Owned(converted),
        errors,
        losses,
    })
}

fn map_mut(value: &mut V) -> Result<&mut Compound> {
    if let V::Compound(value) = value {
        Ok(value)
    } else {
        Err("expected NBT compound".into())
    }
}

fn list_mut(value: &mut V) -> Result<&mut Vec<V>> {
    if let V::List(value) = value {
        Ok(value)
    } else {
        Err("expected NBT list".into())
    }
}

fn text(data: &Compound, key: &str) -> Result<String> {
    match data.get(key) {
        Some(V::String(value)) => Ok(value.clone()),
        _ => Err(format!("{key}: expected string")),
    }
}

fn take_map(data: &mut Compound, key: &str) -> Result<Compound> {
    match data.remove(key) {
        Some(V::Compound(value)) => Ok(value),
        None => Ok(Compound::new()),
        _ => Err(format!("{key}: expected compound")),
    }
}

fn references(value: &mut V, kind: &str, context: &Context) -> Result<()> {
    for value in list_mut(value)? {
        *value = V::String(context.rename(kind, crate::nbt::string(value)?)?);
    }
    Ok(())
}

fn insert(data: &mut Compound, key: &str, value: V) -> Result<()> {
    if data.contains_key(key) {
        return Err(format!("{key}: destination field already exists"));
    }
    data.insert(key.into(), value);
    Ok(())
}

fn move_field(data: &mut Compound, from: &str, to: &str) -> Result<()> {
    if let Some(value) = data.remove(from) {
        insert(data, to, value)?;
    }
    Ok(())
}

fn rename(data: &mut Compound, old: &str, new: &str, forward: bool) -> Result<()> {
    let (from, to) = if forward { (old, new) } else { (new, old) };
    move_field(data, from, to)
}

fn depth(value: usize) -> Result<()> {
    if value > 64 {
        Err("nested Minecraft data exceeds 64 levels".into())
    } else {
        Ok(())
    }
}

fn block_feature(id: &str, version: i32) -> Result<()> {
    feature("block", id, version)
}

fn feature(kind: &str, id: &str, version: i32) -> Result<()> {
    #[derive(Deserialize)]
    struct Feature {
        stable: i32,
        kind: String,
        ids: Vec<String>,
    }
    static FEATURES: OnceLock<std::result::Result<Vec<Feature>, String>> = OnceLock::new();
    let features = FEATURES
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/features.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    if let Some(feature) = features.iter().find(|feature| {
        feature.kind == kind
            && version < feature.stable
            && feature.ids.iter().any(|candidate| candidate == id)
    }) {
        return Err(format!(
            "{id}: experimental {kind} requires feature-pack context before data version{}",
            feature.stable
        ));
    }
    Ok(())
}
