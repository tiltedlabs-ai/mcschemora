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

impl Profile {
    pub fn parse(value: &str) -> Result<Self> {
        let json = match value {
            "redstone" => include_str!("../data/profiles/java/redstone.json"),
            _ => value,
        };
        serde_json::from_str(json)
            .map_err(|error| format!("Invalid normalization profile: {error}"))
    }
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
    source
        .properties
        .retain(|name, _| target.properties.contains_key(name));
    source.id.clone_from(&target.id);
    registry.resolve(&source).ok()
}

impl Schematic {
    pub fn normalize(&mut self, profile: &Profile, keep_override: &[String]) -> Result<Report> {
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
            .chain(keep_override)
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
            let changes = region.blocks.remap(
                |block| states.get(block).and_then(Option::as_ref).cloned(),
                |position, target| {
                    region.bounds.contains(position)
                        && region
                            .present
                            .as_ref()
                            .is_none_or(|cells| cells.contains(&position))
                        && region.block_entities.get(&position).is_none_or(|data| {
                            match data.get("id") {
                                Some(value) => nbt::string(value)
                                    .is_ok_and(|id| helpers::compatible(id, target)),
                                None => block_entity_id(&target.id).is_some(),
                            }
                        })
                },
            );
            for (block, count) in changes {
                *counts.entry(block.id).or_insert(0u64) += count as u64;
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
