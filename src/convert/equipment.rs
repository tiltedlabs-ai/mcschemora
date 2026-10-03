use super::*;

const HANDS: [&str; 2] = ["mainhand", "offhand"];
const ARMOR: [&str; 4] = ["feet", "legs", "chest", "head"];

fn lists(
    data: &mut Compound,
    output: &mut Compound,
    field: &str,
    names: &[&str],
    forward: bool,
    items: bool,
) -> Result<()> {
    if forward {
        if let Some(value) = data.remove(field) {
            let values = crate::nbt::list(&value)?;
            if values.len() > names.len() {
                return Err(format!(
                    "{field}: too many {}",
                    if items {
                        "equipment slots"
                    } else {
                        "drop chances"
                    }
                ));
            }
            for (name, value) in names.iter().zip(values) {
                if items {
                    if crate::nbt::compound(value)?.is_empty() {
                        continue;
                    }
                } else if !matches!(value, V::Float(n) if n.is_finite()) {
                    return Err(format!("{field}.{name}: expected finite float"));
                }
                super::insert(output, name, value.clone())?;
            }
        }
    } else if names.iter().any(|name| output.contains_key(*name)) {
        let values = names
            .iter()
            .map(|name| {
                output.remove(*name).unwrap_or_else(|| {
                    if items {
                        V::Compound(Compound::new())
                    } else {
                        V::Float(0.085)
                    }
                })
            })
            .collect();
        super::insert(data, field, V::List(values))?;
    }
    Ok(())
}

fn body_owner(id: &str) -> bool {
    matches!(
        id,
        "minecraft:horse" | "minecraft:llama" | "minecraft:trader_llama" | "minecraft:wolf"
    )
}

pub(super) fn convert(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if id == "minecraft:player" {
        return player(data, context);
    }
    if !super::entities::living(id, context) {
        return Ok(());
    }
    if !context.crosses(4301) {
        return Ok(());
    }
    let forward = context.forward();
    let mut equipment = take_map(data, "equipment")?;
    let mut drop = take_map(data, "drop_chances")?;
    lists(data, &mut equipment, "HandItems", &HANDS, forward, true)?;
    lists(data, &mut equipment, "ArmorItems", &ARMOR, forward, true)?;
    lists(data, &mut drop, "HandDropChances", &HANDS, forward, false)?;
    lists(data, &mut drop, "ArmorDropChances", &ARMOR, forward, false)?;
    if forward {
        if body_owner(id)
            && let Some(value) = data.remove("body_armor_item")
        {
            super::insert(&mut equipment, "body", value)?;
        }
        if body_owner(id)
            && let Some(value) = data.remove("body_armor_drop_chance")
        {
            super::insert(&mut drop, "body", value)?;
        }
        if saddle_item_owner(id)
            && let Some(value) = data.remove("SaddleItem")
        {
            super::insert(&mut equipment, "saddle", value)?;
            super::insert(&mut drop, "saddle", V::Float(2.0))?;
        }
        if matches!(id, "minecraft:pig" | "minecraft:strider")
            && let Some(value) = data.remove("Saddle")
            && crate::nbt::number(&value)? != 0
        {
            super::insert(
                &mut equipment,
                "saddle",
                V::Compound(Compound::from([
                    ("id".into(), V::String("minecraft:saddle".into())),
                    ("count".into(), V::Int(1)),
                ])),
            )?;
            super::insert(&mut drop, "saddle", V::Float(2.0))?;
        }
    } else {
        if (equipment.contains_key("body") || drop.contains_key("body")) && !body_owner(id) {
            return Err("equipment.body: this entity has no legacy body armor slot".into());
        }
        if let Some(value) = equipment.remove("body") {
            super::insert(data, "body_armor_item", value)?;
        }
        if let Some(value) = drop.remove("body") {
            super::insert(data, "body_armor_drop_chance", value)?;
        }
        if let Some(value) = equipment.remove("saddle") {
            if matches!(id, "minecraft:pig" | "minecraft:strider") {
                let saddle = crate::nbt::compound(&value)?;
                if text(saddle, "id")? != "minecraft:saddle"
                    || saddle
                        .get("count")
                        .map(crate::nbt::number)
                        .transpose()?
                        .unwrap_or(1)
                        != 1
                    || saddle.keys().any(|k| !matches!(k.as_str(), "id" | "count"))
                {
                    return Err("equipment.saddle: customized saddle cannot be represented as a legacy saddle flag".into());
                }
                super::insert(data, "Saddle", V::Byte(1))?;
            } else if saddle_item_owner(id) {
                super::insert(data, "SaddleItem", value)?;
            } else {
                return Err("equipment.saddle: entity has no legacy saddle slot".into());
            }
        }
        if let Some(value) = drop.remove("saddle")
            && value != V::Float(2.0)
        {
            return Err(
                "drop_chances.saddle: nondefault saddle drop chance cannot be represented".into(),
            );
        }
        if !equipment.is_empty() {
            return Err(format!(
                "equipment: unsupported slots {:?}",
                equipment.keys().collect::<Vec<_>>()
            ));
        }
        if !drop.is_empty() {
            return Err(format!(
                "drop_chances: unsupported slots {:?}",
                drop.keys().collect::<Vec<_>>()
            ));
        }
    }
    if !equipment.is_empty() {
        data.insert("equipment".into(), V::Compound(equipment));
    }
    if !drop.is_empty() {
        data.insert("drop_chances".into(), V::Compound(drop));
    }
    Ok(())
}

fn player(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(4312) {
        return Ok(());
    }
    let mut equipment = take_map(data, "equipment")?;
    let slots = [
        (100, "feet"),
        (101, "legs"),
        (102, "chest"),
        (103, "head"),
        (-106, "offhand"),
    ];
    if context.forward() {
        if let Some(value) = data.get_mut("Inventory") {
            let inventory = list_mut(value)?;
            let mut retained = Vec::new();
            for value in std::mem::take(inventory) {
                let mut item = crate::nbt::compound(&value)?.clone();
                let slot = item
                    .get("Slot")
                    .map(crate::nbt::number)
                    .transpose()?
                    .unwrap_or(-1);
                if let Some((_, name)) = slots.iter().find(|(candidate, _)| *candidate == slot) {
                    item.remove("Slot");
                    super::insert(&mut equipment, name, V::Compound(item))?;
                } else {
                    retained.push(value);
                }
            }
            *inventory = retained;
        }
    } else {
        let mut additions = Vec::new();
        for (slot, name) in slots {
            if let Some(value) = equipment.remove(name) {
                let mut item = crate::nbt::compound(&value)?.clone();
                super::insert(&mut item, "Slot", V::Byte(slot as i8))?;
                additions.push(V::Compound(item));
            }
        }
        if !equipment.is_empty() {
            return Err(
                "equipment: player slot cannot be represented by legacy inventory equipment".into(),
            );
        }
        if !additions.is_empty() {
            let inventory = data
                .entry("Inventory".into())
                .or_insert_with(|| V::List(Vec::new()));
            let inventory = list_mut(inventory)?;
            for added in &additions {
                let slot = crate::nbt::get(crate::nbt::compound(added)?, "Slot")?;
                if inventory.iter().any(|item| {
                    crate::nbt::compound(item)
                        .ok()
                        .and_then(|item| item.get("Slot"))
                        == Some(slot)
                }) {
                    return Err("Inventory: duplicate equipment slot".into());
                }
            }
            inventory.extend(additions);
        }
    }
    if !equipment.is_empty() {
        data.insert("equipment".into(), V::Compound(equipment));
    }
    Ok(())
}

fn saddle_item_owner(id: &str) -> bool {
    matches!(
        id,
        "minecraft:horse"
            | "minecraft:skeleton_horse"
            | "minecraft:zombie_horse"
            | "minecraft:donkey"
            | "minecraft:mule"
            | "minecraft:camel"
    )
}
