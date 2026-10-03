mod blocks;
mod entities;
mod items;

use crate::{Result, catalog::Registry, model::*, nbt::Tag as V};
use serde::Deserialize;
use std::{
    borrow::Cow,
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
    target: Arc<Registry>,
}

impl Context {
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
        Ok(id)
    }
}

pub(crate) async fn document<'a>(
    doc: &'a Schematic,
    version: Option<&str>,
) -> Result<Cow<'a, Schematic>> {
    let Some(version) = version else {
        return Ok(Cow::Borrowed(doc));
    };
    if doc.edition != "java" {
        return Err("A target Minecraft version is supported only for Java documents".into());
    }
    if doc.data_version < crate::versions::MIN_JAVA_DATA_VERSION {
        return Err("Version conversion requires a known Java 1.13+ source data version".into());
    }
    if version.is_empty() || version == "latest" {
        return Err("Export requires an explicit target Minecraft version".into());
    }
    let target = doc.data.load(version).await?;
    if target.data_version == doc.data_version {
        return Ok(Cow::Borrowed(doc));
    }
    let supported = [3578, 3698, 3700, 3837, 3839];
    if !supported.contains(&doc.data_version) || !supported.contains(&target.data_version) {
        return Err(format!(
            "Minecraft conversion {} → {} is not implemented; supported versions are 1.20.2–1.20.6, with payload conversion from 1.20.3 onward",
            doc.version, target.version
        ));
    }
    if doc.data_version >= crate::versions::ITEM_COMPONENTS
        && target.data_version < crate::versions::ITEM_COMPONENTS
    {
        return Err(
            "Downgrading item-component schemas to Minecraft 1.20.4 or earlier is not implemented"
                .into(),
        );
    }
    let context = Context {
        source: doc.data_version,
        target,
    };
    let mut converted = doc.clone();
    for (name, region) in &mut converted.regions {
        if context.source.min(context.target.data_version) < 3698
            && (!region.entities.is_empty() || !region.block_entities.is_empty())
        {
            return Err(format!(
                "{name}: entity and block-entity conversion involving Minecraft 1.20.2 is not implemented"
            ));
        }
        let mut palette = std::collections::BTreeMap::new();
        for state in region.blocks.states() {
            let next = blocks::state(state, &context)
                .map_err(|e| format!("{name}: {}: {e}", state.text()))?;
            palette.insert(state.clone(), next);
        }
        let cells: Vec<_> = region
            .blocks
            .iter()
            .map(|(p, b)| (*p, palette[b].clone()))
            .collect();
        region.blocks.clear();
        for (p, b) in cells {
            if b != Block::air() {
                region.blocks.set(p, &b);
            }
        }
        for (p, data) in &mut region.block_entities {
            entities::block_entity(data, &context, 0)
                .map_err(|e| format!("{name} {p:?}: block_entity.{e}"))?;
        }
        for e in &mut region.entities {
            entities::entity(&mut e.data, &context, 0)
                .map_err(|error| format!("{name} entity {}: {error}", e.reference))?;
        }
        if region.retained.bedrock.is_some() {
            return Err(format!("{name}: cannot convert retained Bedrock data"));
        }
        for (key, value) in &mut region.retained.spatial {
            match key.as_str() {
                "PendingBlockTicks" | "PendingFluidTicks" => {
                    let field = if key == "PendingBlockTicks" {
                        "Block"
                    } else {
                        "Fluid"
                    };
                    for (index, tick) in list_mut(value)?.iter_mut().enumerate() {
                        let tick = map_mut(tick)?;
                        let id = text(tick, field)?;
                        let renamed = context
                            .rename(if field == "Block" { "block" } else { "fluid" }, &id)?;
                        if field == "Block" {
                            context
                                .target
                                .resolve(&Block::parse(&renamed)?)
                                .map_err(|e| format!("{name}.{key}[{index}]: {e}"))?;
                        }
                        tick.insert(field.into(), V::String(renamed));
                    }
                }
                _ => {
                    return Err(format!(
                        "{name}: version conversion of retained {key} data is not implemented"
                    ));
                }
            }
        }
    }
    converted.version = context.target.version.clone();
    converted.data_version = context.target.data_version;
    converted.catalog = Some(context.target);
    Ok(Cow::Owned(converted))
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

fn move_field(data: &mut Compound, from: &str, to: &str) -> Result<()> {
    if let Some(value) = data.remove(from) {
        if data.contains_key(to) {
            return Err(format!("{from}: destination field {to} already exists"));
        }
        data.insert(to.into(), value);
    }
    Ok(())
}

fn depth(value: usize) -> Result<()> {
    if value > 64 {
        Err("nested Minecraft data exceeds 64 levels".into())
    } else {
        Ok(())
    }
}
