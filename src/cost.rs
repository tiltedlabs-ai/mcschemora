use crate::{
    Result,
    catalog::{Registry, namespace},
    model::{Block, Position, Region, Schematic},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub items: BTreeMap<String, u64>,
    pub raw_materials: BTreeMap<String, f64>,
}

#[derive(Debug)]
pub(crate) struct Catalog {
    names: Vec<String>,
    lookup: BTreeMap<String, usize>,
    costs: Vec<Vec<(usize, f64)>>,
}

struct Recipe {
    ingredients: Vec<(usize, f64)>,
    output: f64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Stack {
    Id(u64),
    Item { id: u64, count: Option<u64> },
}

impl Stack {
    fn parse(value: Value, ids: &BTreeMap<u64, usize>) -> Result<(usize, f64)> {
        let (id, count) = match serde_json::from_value(value)
            .map_err(|e| format!("Invalid recipe stack: {e}"))?
        {
            Self::Id(id) => (id, 1),
            Self::Item { id, count } => (id, count.unwrap_or(1)),
        };
        if count == 0 {
            return Err("Recipe stack count must be positive".into());
        }
        Ok((
            *ids.get(&id)
                .ok_or_else(|| format!("Unknown recipe item ID {id}"))?,
            count as f64,
        ))
    }
}

impl Catalog {
    pub(crate) fn build(items: Value, recipes: Value) -> Result<Self> {
        #[derive(Deserialize)]
        struct Item {
            id: u64,
            name: String,
        }
        let items: Vec<Item> =
            serde_json::from_value(items).map_err(|e| format!("Invalid item catalog: {e}"))?;
        let mut names = Vec::new();
        let mut lookup = BTreeMap::new();
        let mut ids = BTreeMap::new();
        for item in items {
            let name = Block::new(&item.name, BTreeMap::new())?.id;
            let index = names.len();
            if lookup.insert(name.clone(), index).is_some() || ids.insert(item.id, index).is_some()
            {
                return Err("Duplicate item name or ID in cost catalog".into());
            }
            names.push(name);
        }
        let mut rules: Vec<Vec<Recipe>> = (0..names.len()).map(|_| Vec::new()).collect();
        for (output, variants) in recipes
            .as_object()
            .ok_or("Recipe catalog must be an object")?
        {
            let output: u64 = output.parse().map_err(|_| "Invalid recipe output ID")?;
            let target = *ids.get(&output).ok_or("Unknown recipe output ID")?;
            let mut variants: Vec<_> = variants
                .as_array()
                .ok_or("Recipe variants must be an array")?
                .iter()
                .collect();
            variants.sort_by_key(|recipe| recipe.get("inShape").is_none());
            for recipe in variants {
                let (result, count) = Stack::parse(
                    recipe.get("result").ok_or("Recipe has no result")?.clone(),
                    &ids,
                )?;
                if result != target {
                    return Err("Recipe result does not match its output ID".into());
                }
                let cells: Vec<&Value> = match (recipe.get("inShape"), recipe.get("ingredients")) {
                    (Some(shape), None) => shape
                        .as_array()
                        .ok_or("Recipe shape must be an array")?
                        .iter()
                        .map(|row| row.as_array().ok_or("Recipe row must be an array"))
                        .collect::<std::result::Result<Vec<_>, _>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                    (None, Some(ingredients)) => ingredients
                        .as_array()
                        .ok_or("Recipe ingredients must be an array")?
                        .iter()
                        .collect(),
                    _ => return Err("Recipe must have a shape or ingredients".into()),
                };
                let mut ingredients = BTreeMap::new();
                for cell in cells.into_iter().filter(|v| !v.is_null()) {
                    let (item, amount) = Stack::parse(cell.clone(), &ids)?;
                    *ingredients.entry(item).or_insert(0.) += amount;
                }
                if ingredients.is_empty() {
                    return Err("Recipe has no ingredients".into());
                }
                rules[target].push(Recipe {
                    ingredients: ingredients.into_iter().collect(),
                    output: count,
                });
            }
        }
        let primitives: Vec<String> =
            serde_json::from_str(include_str!("../data/cost/java/primitives.json"))
                .map_err(|e| e.to_string())?;
        let mut costs = vec![None; names.len()];
        for name in primitives {
            if let Some(&id) = lookup.get(&name) {
                costs[id] = Some(BTreeMap::from([(id, 1.)]));
            }
        }
        for (id, name) in names.iter().enumerate() {
            if rules[id].is_empty()
                && let Some(source) = acquisition(name).and_then(|name| lookup.get(&name))
            {
                rules[id].push(Recipe {
                    ingredients: vec![(*source, 1.)],
                    output: 1.,
                });
            }
        }
        for (id, rules) in rules.iter().enumerate() {
            if rules.is_empty() {
                costs[id] = Some(BTreeMap::from([(id, 1.)]));
            }
        }
        for id in 0..names.len() {
            expand(id, &rules, &mut costs, &mut BTreeSet::new());
        }
        Ok(Self {
            names,
            lookup,
            costs: costs
                .into_iter()
                .enumerate()
                .map(|(id, cost)| {
                    cost.map(|cost| cost.into_iter().collect())
                        .unwrap_or_else(|| vec![(id, 1.)])
                })
                .collect(),
        })
    }
}

fn expand(
    id: usize,
    recipes: &[Vec<Recipe>],
    costs: &mut [Option<BTreeMap<usize, f64>>],
    visiting: &mut BTreeSet<usize>,
) -> Option<BTreeMap<usize, f64>> {
    if let Some(cost) = &costs[id] {
        return Some(cost.clone());
    }
    if !visiting.insert(id) {
        return None;
    }
    for recipe in &recipes[id] {
        let mut total = BTreeMap::new();
        let mut complete = true;
        for &(ingredient, amount) in &recipe.ingredients {
            let Some(cost) = expand(ingredient, recipes, costs, visiting) else {
                complete = false;
                break;
            };
            for (primitive, value) in cost {
                *total.entry(primitive).or_insert(0.) += value * amount / recipe.output;
            }
        }
        if complete && total.values().all(|value: &f64| value.is_finite()) {
            visiting.remove(&id);
            costs[id] = Some(total.clone());
            return Some(total);
        }
    }
    visiting.remove(&id);
    None
}

fn acquisition(name: &str) -> Option<String> {
    let name = name.strip_prefix("minecraft:")?;
    let source = if let Some(name) = name.strip_prefix("stripped_") {
        name.to_string()
    } else if let Some(name) = name.strip_suffix("_concrete") {
        format!("{name}_concrete_powder")
    } else if let Some(name) = name
        .strip_prefix("dead_")
        .filter(|name| name.contains("coral"))
    {
        name.to_string()
    } else if let Some(name) = ["exposed_", "weathered_", "oxidized_"]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
    {
        if name == "copper" {
            "copper_block".into()
        } else {
            name.into()
        }
    } else {
        match name {
            "carved_pumpkin" => "pumpkin".into(),
            "chipped_anvil" | "damaged_anvil" => "anvil".into(),
            _ => return None,
        }
    };
    Some(namespace(&source))
}

struct Placement {
    block: Block,
    items: Vec<(usize, u64)>,
    partner: Option<(Position, &'static str, &'static str)>,
    primitive: bool,
}

fn placement(block: &Block, registry: &Registry, catalog: &Catalog) -> Placement {
    let mut plan = Placement {
        block: block.clone(),
        items: Vec::new(),
        partner: None,
        primitive: false,
    };
    let Ok(block) = registry.resolve(block) else {
        plan.primitive = true;
        return plan;
    };
    plan.block = block;
    if plan.block.is_air() {
        return plan;
    }
    let name = plan
        .block
        .id
        .strip_prefix("minecraft:")
        .unwrap_or(&plan.block.id);
    if matches!(name, "water" | "lava")
        && plan
            .block
            .properties
            .get("level")
            .is_some_and(|level| level != "0")
    {
        return plan;
    }
    let mut names = Vec::new();
    if let Some(plant) = name.strip_prefix("potted_") {
        let plant = match plant {
            "azalea_bush" => "azalea",
            "flowering_azalea_bush" => "flowering_azalea",
            _ => plant,
        };
        names.extend(["flower_pot".to_string(), plant.to_string()]);
    } else if name.ends_with("candle_cake") {
        names.extend([
            "cake".to_string(),
            name.strip_suffix("_cake").unwrap().to_string(),
        ]);
    } else {
        names.push(match name {
            "redstone_wire" => "redstone".into(),
            "tripwire" => "string".into(),
            "wall_torch" => "torch".into(),
            "farmland" | "dirt_path" | "grass_path" => "dirt".into(),
            "wheat" => "wheat_seeds".into(),
            "carrots" => "carrot".into(),
            "potatoes" => "potato".into(),
            "beetroots" => "beetroot_seeds".into(),
            "cocoa" => "cocoa_beans".into(),
            "melon_stem" | "attached_melon_stem" => "melon_seeds".into(),
            "pumpkin_stem" | "attached_pumpkin_stem" => "pumpkin_seeds".into(),
            "torchflower_crop" => "torchflower_seeds".into(),
            "pitcher_crop" => "pitcher_pod".into(),
            "cave_vines" | "cave_vines_plant" => "glow_berries".into(),
            "sweet_berry_bush" => "sweet_berries".into(),
            "kelp_plant" => "kelp".into(),
            "bamboo_sapling" => "bamboo".into(),
            "twisting_vines_plant" => "twisting_vines".into(),
            "weeping_vines_plant" => "weeping_vines".into(),
            "tall_seagrass" => "seagrass".into(),
            _ => name
                .replace("_wall_hanging_sign", "_hanging_sign")
                .replace("_wall_", "_"),
        });
    }
    let quantity = if plan
        .block
        .properties
        .get("type")
        .is_some_and(|kind| kind == "double")
        && name.ends_with("_slab")
    {
        2
    } else {
        [
            "candles",
            "pickles",
            "eggs",
            "layers",
            "flower_amount",
            "segment_amount",
        ]
        .iter()
        .find_map(|property| plan.block.properties.get(*property))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(1)
    };
    for name in names {
        if let Some(&id) = catalog.lookup.get(&namespace(&name)) {
            plan.items.push((id, quantity));
        } else {
            plan.items.clear();
            plan.primitive = true;
            break;
        }
    }
    if plan
        .block
        .properties
        .get("half")
        .is_some_and(|half| half == "upper")
    {
        plan.partner = Some(([0, -1, 0], "half", "lower"));
    } else if plan
        .block
        .properties
        .get("part")
        .is_some_and(|part| part == "head")
        && name.ends_with("_bed")
    {
        let offset = match plan.block.properties.get("facing").map(String::as_str) {
            Some("north") => [0, 0, 1],
            Some("south") => [0, 0, -1],
            Some("west") => [1, 0, 0],
            _ => [-1, 0, 0],
        };
        plan.partner = Some((offset, "part", "foot"));
    }
    plan
}

fn present(region: &Region, position: Position) -> Option<&Block> {
    (region.bounds.contains(position)
        && region
            .present
            .as_ref()
            .is_none_or(|cells| cells.contains(&position)))
    .then(|| region.blocks.get(&position))
    .flatten()
}

fn has_partner(
    schematic: &Schematic,
    region: &Region,
    position: Position,
    plan: &Placement,
    registry: &Registry,
) -> bool {
    let Some((offset, property, expected)) = plan.partner else {
        return false;
    };
    let Some(local) = (0..3)
        .map(|i| position[i].checked_add(offset[i]))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    let local = [local[0], local[1], local[2]];
    let matches = |block: &Block| {
        registry.resolve(block).is_ok_and(|block| {
            block.id == plan.block.id
                && block
                    .properties
                    .get(property)
                    .is_some_and(|value| value == expected)
                && block.properties.get("facing") == plan.block.properties.get("facing")
                && block.properties.get("hinge") == plan.block.properties.get("hinge")
        })
    };
    if present(region, local).is_some_and(matches) {
        return true;
    }
    let Ok(global) = region.to_global(local) else {
        return false;
    };
    schematic
        .regions
        .values()
        .filter(|other| !std::ptr::eq(*other, region))
        .any(|other| {
            other
                .to_local(global)
                .ok()
                .and_then(|local| present(other, local))
                .is_some_and(matches)
        })
}

impl Schematic {
    pub async fn material_cost(&self) -> Result<Report> {
        if self.edition != "java" {
            return Err("Material costs require Java Edition".into());
        }
        let registry = self.registry()?;
        let catalog = self.data.material_costs(&self.version).await?;
        let mut quantities = vec![0u64; catalog.names.len()];
        let mut report = Report::default();
        for region in self.regions.values() {
            let plans: Vec<_> = (0..region.blocks.palette_len())
                .map(|id| placement(region.blocks.palette_entry(id as u32), registry, &catalog))
                .collect();
            let mut counts = vec![0u64; plans.len()];
            for (position, id) in region.blocks.indexed_iter() {
                if !region.bounds.contains(*position)
                    || region
                        .present
                        .as_ref()
                        .is_some_and(|cells| !cells.contains(position))
                {
                    continue;
                }
                if plans[id as usize].partner.is_some()
                    && has_partner(self, region, *position, &plans[id as usize], registry)
                {
                    continue;
                }
                counts[id as usize] += 1;
            }
            for (plan, count) in plans.iter().zip(counts).filter(|(_, count)| *count > 0) {
                for &(item, quantity) in &plan.items {
                    quantities[item] = count
                        .checked_mul(quantity)
                        .and_then(|count| quantities[item].checked_add(count))
                        .ok_or("Material item count overflow")?;
                }
                if plan.primitive {
                    let total = report.items.entry(plan.block.id.clone()).or_default();
                    *total = total
                        .checked_add(count)
                        .ok_or("Material item count overflow")?;
                    *report
                        .raw_materials
                        .entry(plan.block.id.clone())
                        .or_default() += count as f64;
                }
            }
        }
        for (id, count) in quantities
            .into_iter()
            .enumerate()
            .filter(|(_, count)| *count > 0)
        {
            let total = report.items.entry(catalog.names[id].clone()).or_default();
            *total = total
                .checked_add(count)
                .ok_or("Material item count overflow")?;
            for &(primitive, amount) in &catalog.costs[id] {
                let total = report
                    .raw_materials
                    .entry(catalog.names[primitive].clone())
                    .or_default();
                *total += amount * count as f64;
                if !total.is_finite() {
                    return Err("Material cost overflow".into());
                }
            }
        }
        Ok(report)
    }
}
