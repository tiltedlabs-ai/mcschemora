use crate::{
    Result,
    catalog::{Registry, block_entity_id},
    model::{Block, Schematic},
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
    (!block.is_air() && block.properties.is_empty() && block_entity_id(&block.id).is_none())
        .then_some(block)
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
        let mut counts = BTreeMap::new();
        for region in self.regions.values_mut() {
            let mut edits = Vec::new();
            for (position, block) in region.blocks.iter() {
                if !region.bounds.contains(*position)
                    || !block.properties.is_empty()
                    || region.block_entities.contains_key(position)
                    || region
                        .present
                        .as_ref()
                        .is_some_and(|cells| !cells.contains(position))
                {
                    continue;
                }
                let Some(target) = replacements.get(&block.id) else {
                    continue;
                };
                if block.id == target.id {
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
