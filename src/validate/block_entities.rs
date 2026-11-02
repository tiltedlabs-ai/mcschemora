use super::{Point, Report, scene::Scene};
use crate::{
    model::{Compound, Position, Schematic},
    versions::ITEM_COMPONENTS,
};
use fastnbt::Value;
use std::collections::HashSet;

// https://minecraft.wiki/w/Block_entity
// Inventory slot IDs are local to each block, including each double-chest half.
pub(super) fn check(
    scene: &Scene<'_>,
    p: Point,
    data: &Compound,
    region: &str,
    local: Position,
    schematic: &Schematic,
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
            if crate::catalog::block_entity_id(&block.id)
                == Some(id.strip_prefix("minecraft:").unwrap_or(id)) => {}
        _ => error("missing ID or ID does not match the block"),
    }
    let capacity =
        crate::catalog::block_entity_id(&block.id).and_then(crate::catalog::inventory_size);
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
                Some(Value::String(id))
                    if schematic.registry().is_ok_and(|r| r.item(id).is_ok()) => {}
                _ => error("unknown or missing inventory item ID"),
            }
            let valid_count = if schematic.data_version >= ITEM_COMPONENTS {
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
