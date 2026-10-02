use super::{Point, Report, name, scene::Scene};
use crate::{
    model::{Block, Compound, Position, Schematic},
    versions::ITEM_COMPONENTS,
};
use fastnbt::Value;
use std::collections::HashSet;

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/entity/BlockEntityType.java
pub(crate) fn block_entity_id(b: &Block) -> Option<&str> {
    let n = name(b);
    Some(match n {
        "moving_piston" => "piston",
        "soul_campfire" => "campfire",
        "chain_command_block" | "repeating_command_block" => "command_block",
        n if n.ends_with("_hanging_sign") => "hanging_sign",
        n if n.ends_with("_sign") || n == "sign" => "sign",
        n if n.ends_with("_bed") => "bed",
        n if n.ends_with("_banner") => "banner",
        n if n.ends_with("shulker_box") => "shulker_box",
        "skeleton_skull"
        | "skeleton_wall_skull"
        | "wither_skeleton_skull"
        | "wither_skeleton_wall_skull"
        | "player_head"
        | "player_wall_head"
        | "zombie_head"
        | "zombie_wall_head"
        | "creeper_head"
        | "creeper_wall_head"
        | "dragon_head"
        | "dragon_wall_head"
        | "piglin_head"
        | "piglin_wall_head" => "skull",
        "chest"
        | "trapped_chest"
        | "furnace"
        | "blast_furnace"
        | "smoker"
        | "hopper"
        | "dispenser"
        | "dropper"
        | "barrel"
        | "beacon"
        | "spawner"
        | "lectern"
        | "brewing_stand"
        | "crafter"
        | "campfire"
        | "command_block"
        | "comparator"
        | "daylight_detector"
        | "enchanting_table"
        | "end_portal"
        | "end_gateway"
        | "ender_chest"
        | "jigsaw"
        | "jukebox"
        | "structure_block"
        | "conduit"
        | "bell"
        | "beehive"
        | "sculk_sensor"
        | "calibrated_sculk_sensor"
        | "sculk_catalyst"
        | "sculk_shrieker"
        | "chiseled_bookshelf"
        | "decorated_pot"
        | "trial_spawner"
        | "vault" => n,
        "bee_nest" => "beehive",
        "suspicious_sand" | "suspicious_gravel" => "brushable_block",
        _ => return None,
    })
}

// https://minecraft.wiki/w/Block_entity
// Inventory slot IDs are local to each block, including each double-chest half.
pub(super) fn check(
    scene: &Scene<'_>,
    p: Point,
    data: &Compound,
    region: &str,
    local: Position,
    doc: &Schematic,
    report: &mut Report,
) {
    let Some(block) = scene.get(p) else {
        report.unknown.push(format!(
            "{region} {local:?}: block_entity: owning block is unknown"
        ));
        return;
    };
    let mut error = |message: &str| {
        report
            .errors
            .push(format!("{region} {local:?}: block_entity: {message}"))
    };
    match data.get("id") {
        Some(Value::String(id))
            if block_entity_id(block) == Some(id.strip_prefix("minecraft:").unwrap_or(id)) => {}
        _ => error("missing ID or ID does not match the block"),
    }
    let capacity = match name(block) {
        "chest" | "trapped_chest" | "barrel" => Some(27),
        "hopper" | "brewing_stand" => Some(5),
        "furnace" | "blast_furnace" | "smoker" => Some(3),
        "dispenser" | "dropper" | "crafter" => Some(9),
        "campfire" | "soul_campfire" => Some(4),
        "chiseled_bookshelf" => Some(6),
        n if n.ends_with("shulker_box") => Some(27),
        _ => None,
    };
    if let Some(items) = data.get("Items") {
        let Value::List(items) = items else {
            error("Items must be a list");
            return;
        };
        let mut slots = HashSet::new();
        for item in items {
            let Value::Compound(item) = item else {
                error("inventory entries must be compounds");
                continue;
            };
            match item.get("Slot") {
                Some(Value::Byte(slot))
                    if *slot >= 0 && capacity.is_none_or(|n| i32::from(*slot) < n) =>
                {
                    if !slots.insert(*slot) {
                        error("duplicate inventory slot");
                    }
                }
                _ => error("invalid inventory slot"),
            }
            match item.get("id") {
                Some(Value::String(id)) if doc.registry().is_ok_and(|r| r.item(id).is_ok()) => (),
                _ => error("unknown or missing inventory item ID"),
            }
            let valid_count = if doc.data_version >= ITEM_COMPONENTS {
                item.get("count").is_none()
                    || matches!(item.get("count"),Some(Value::Int(n)) if *n > 0)
            } else {
                matches!(item.get("Count"),Some(Value::Byte(n)) if *n > 0)
            };
            if !valid_count {
                error("item count has the wrong type, name, or value for this version");
            }
        }
    }
}
