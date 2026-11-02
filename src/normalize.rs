use crate::{
    Result,
    catalog::{Registry, block_entity_id},
    helpers,
    model::{Block, Schematic},
    nbt,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub edition: String,
    #[serde(default)]
    pub keep: Vec<String>,
    #[serde(default)]
    pub replacements: Vec<Replacement>,
    pub fallback: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    pub from: Vec<String>,
    pub to: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub changed: u64,
    pub replacements: Vec<ReplacementCount>,
}

#[derive(Debug, Serialize)]
pub struct ReplacementCount {
    pub from: String,
    pub to: String,
    pub count: u64,
}

fn resolve(registry: &Registry, id: &str) -> Option<Block> {
    let block = registry
        .resolve(&Block::new(id, BTreeMap::new()).ok()?)
        .ok()?;
    (!block.is_air()).then_some(block)
}

fn replacement(registry: &Registry, block: &Block, target: &Block) -> Option<Block> {
    if block_entity_id(&block.id) != block_entity_id(&target.id) {
        return None;
    }
    let mut source = registry.resolve(block).ok()?;
    source.id.clone_from(&target.id);
    let target = registry.resolve(&source).ok()?;
    (target.properties == source.properties).then_some(target)
}

impl Schematic {
    pub fn normalize(&mut self, profile: &Profile) -> Result<Report> {
        if profile.edition != self.edition {
            return Err(format!(
                "Normalization profile requires {}; schematic is {}",
                profile.edition, self.edition
            ));
        }
        let registry = self.registry()?;
        let keep: BTreeSet<_> = profile
            .keep
            .iter()
            .filter_map(|id| Block::new(id, BTreeMap::new()).ok())
            .map(|block| block.id)
            .collect();
        let fallback = profile
            .fallback
            .as_deref()
            .and_then(|id| resolve(registry, id));
        let mut replacements = BTreeMap::new();
        for rule in &profile.replacements {
            let Some(target) = resolve(registry, &rule.to) else {
                continue;
            };
            for id in &rule.from {
                if let Some(source) = resolve(registry, id) {
                    if keep.contains(&source.id)
                        || fallback.as_ref().is_some_and(|block| block.id == source.id)
                    {
                        continue;
                    }
                    replacements
                        .entry(source.id)
                        .or_insert_with(|| target.clone());
                }
            }
        }
        let sources: BTreeSet<_> = replacements
            .iter()
            .filter(|(source, target)| **source != target.id)
            .map(|(source, _)| source.clone())
            .collect();
        replacements.retain(|_, target| !sources.contains(&target.id));
        if let Some(fallback) = fallback {
            let mut targets: BTreeSet<_> = replacements
                .values()
                .map(|block| block.id.clone())
                .collect();
            targets.insert(fallback.id.clone());
            for region in self.regions.values() {
                for block in region.blocks.states() {
                    if keep.contains(&block.id)
                        || targets.contains(&block.id)
                        || replacements.contains_key(&block.id)
                    {
                        continue;
                    }
                    if let Some(source) = resolve(registry, &block.id) {
                        replacements.insert(source.id, fallback.clone());
                    }
                }
            }
        }
        if replacements.is_empty() {
            return Ok(Report::default());
        }
        let mut states = BTreeMap::new();
        for region in self.regions.values() {
            for block in region.blocks.states() {
                let Some(target) = replacements.get(&block.id) else {
                    continue;
                };
                if block.id != target.id {
                    states
                        .entry(block.clone())
                        .or_insert_with(|| replacement(registry, block, target));
                }
            }
        }
        let mut counts = BTreeMap::new();
        for region in self.regions.values_mut() {
            let mut edits = Vec::new();
            for (position, block) in region.blocks.iter() {
                if !region.bounds.contains(*position)
                    || region
                        .present
                        .as_ref()
                        .is_some_and(|cells| !cells.contains(position))
                {
                    continue;
                }
                let Some(Some(target)) = states.get(block) else {
                    continue;
                };
                if region.block_entities.get(position).is_some_and(|data| {
                    data.get("id")
                        .and_then(|value| nbt::string(value).ok())
                        .is_none_or(|id| !helpers::compatible(id, target))
                }) {
                    continue;
                }
                *counts.entry(block.id.clone()).or_insert(0u64) += 1;
                edits.push((*position, target));
            }
            for (position, target) in edits {
                region.blocks.set(position, target);
            }
        }
        Ok(Report {
            changed: counts.values().sum(),
            replacements: counts
                .into_iter()
                .map(|(from, count)| ReplacementCount {
                    to: replacements[&from].id.clone(),
                    from,
                    count,
                })
                .collect(),
        })
    }
}
