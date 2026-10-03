use super::*;
use fastnbt::IntArray;

pub(super) fn entity(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let id = crate::catalog::namespace(&text(data, "id")?);
    context.target.entity_id(&id)?;
    if !context.components() {
        return Ok(());
    }
    data.insert("id".into(), V::String(id.clone()));
    if id == "minecraft:arrow" || id == "minecraft:spectral_arrow" {
        return Err(format!("{id}: projectile conversion is not implemented"));
    }
    if id == "minecraft:area_effect_cloud" {
        if let Some(V::String(particle)) = data.get("Particle") {
            if particle.split_whitespace().count() != 1 {
                return Err(
                    "Particle: parameterized particle conversion is not implemented".into(),
                );
            }
            let particle = V::Compound(Compound::from([(
                "type".into(),
                V::String(particle.clone()),
            )]));
            data.insert("Particle".into(), particle);
        }
        let mut potion = Compound::new();
        for (old, new) in [
            ("Potion", "potion"),
            ("Color", "custom_color"),
            ("effects", "custom_effects"),
        ] {
            if let Some(mut value) = data.remove(old) {
                if old == "effects" {
                    effects(&mut value)?;
                }
                potion.insert(new.into(), value);
            }
        }
        if !potion.is_empty() {
            data.insert("potion_contents".into(), V::Compound(potion));
        }
    }
    let mut double_health = false;
    if let Some(value) = data.get_mut("Attributes") {
        for value in list_mut(value)? {
            let attribute = map_mut(value)?;
            let name = context.rename("attribute", &text(attribute, "Name")?)?;
            if id == "minecraft:wolf"
                && name == "minecraft:generic.max_health"
                && attribute.get("Base") == Some(&V::Double(20.0))
            {
                attribute.insert("Base".into(), V::Double(40.0));
                double_health = true;
            }
            attribute.insert("Name".into(), V::String(name));
        }
    }
    if double_health && let Some(V::Float(health)) = data.get_mut("Health") {
        *health *= 2.0;
    }
    reject_commands(data)?;
    for key in ["HandItems", "ArmorItems", "Inventory", "Items"] {
        item_list(
            data,
            key,
            context,
            level + 1,
            matches!(key, "HandItems" | "ArmorItems"),
        )?;
    }
    for key in [
        "Item",
        "item",
        "SaddleItem",
        "ArmorItem",
        "DecorItem",
        "FireworksItem",
        "SelectedItem",
    ] {
        item_field(data, key, context, level + 1)?;
    }
    if matches!(
        id.as_str(),
        "minecraft:horse" | "minecraft:llama" | "minecraft:trader_llama"
    ) {
        let key = if id == "minecraft:horse" {
            "ArmorItem"
        } else {
            "DecorItem"
        };
        if data.contains_key(key) {
            move_field(data, key, "body_armor_item")?;
            data.insert("body_armor_drop_chance".into(), V::Float(2.0));
            if id == "minecraft:horse" {
                if let Some(value) = data.get_mut("ArmorItems")
                    && let Some(slot) = list_mut(value)?.get_mut(2)
                {
                    *slot = V::Compound(Compound::new());
                }
                if let Some(value) = data.get_mut("ArmorDropChances")
                    && let Some(slot) = list_mut(value)?.get_mut(2)
                {
                    *slot = V::Float(0.085);
                }
            }
        }
    }
    if matches!(
        id.as_str(),
        "minecraft:llama" | "minecraft:trader_llama" | "minecraft:donkey" | "minecraft:mule"
    ) && let Some(value) = data.get_mut("Items")
    {
        for value in list_mut(value)? {
            let item = map_mut(value)?;
            let slot = item
                .get("Slot")
                .map(crate::nbt::number)
                .transpose()?
                .unwrap_or(2);
            item.insert(
                "Slot".into(),
                V::Byte(i8::try_from(slot - 2).map_err(|_| "pack inventory slot overflow")?),
            );
        }
    }
    if let Some(value) = data.get_mut("Passengers") {
        for (index, passenger) in list_mut(value)?.iter_mut().enumerate() {
            entity(map_mut(passenger)?, context, level + 1)
                .map_err(|e| format!("Passengers[{index}].{e}"))?;
        }
    }
    if let Some(value) = data.get_mut("Offers") {
        let offers = map_mut(value)?;
        if let Some(value) = offers.get_mut("Recipes") {
            for (index, recipe) in list_mut(value)?.iter_mut().enumerate() {
                let recipe = map_mut(recipe)?;
                for key in ["buy", "buyB", "sell"] {
                    item_field(recipe, key, context, level + 1)
                        .map_err(|e| format!("Offers.Recipes[{index}].{e}"))?;
                }
            }
        }
    }
    for key in [
        "BlockState",
        "DisplayState",
        "carriedBlockState",
        "inBlockState",
        "block_state",
    ] {
        if let Some(value) = data.get_mut(key) {
            blocks::nbt(map_mut(value)?, context).map_err(|e| format!("{key}.{e}"))?;
        }
    }
    if let Some(value) = data.get_mut("TileEntityData") {
        block_entity(map_mut(value)?, context, level + 1)?;
    }
    for (from, to) in [
        ("FlowerPos", "flower_pos"),
        ("HivePos", "hive_pos"),
        ("BeamTarget", "beam_target"),
        ("PatrolTarget", "patrol_target"),
        ("WanderTarget", "wander_target"),
    ] {
        position(data, from, to)?;
    }
    if let Some(value) = data.remove("Leash") {
        let leash = crate::nbt::compound(&value)?;
        let value = if leash.contains_key("X") {
            position_value(leash)?
        } else {
            value
        };
        if data.insert("leash".into(), value).is_some() {
            return Err("Leash: duplicate leash field".into());
        }
    }
    if let Some(value) = data.get_mut("active_effects") {
        effects(value)?;
    }
    if let Some(value) = data.get_mut("CustomName") {
        items::text_component(value)?;
    }
    if let Some(value) = data.get_mut("text") {
        items::text_component(value)?;
    }
    Ok(())
}

pub(super) fn block_entity(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let id = crate::catalog::namespace(&text(data, "id")?);
    let id = id
        .strip_prefix("minecraft:")
        .ok_or("modded block-entity conversion is not implemented")?;
    if !matches!(
        id,
        "chest"
            | "trapped_chest"
            | "barrel"
            | "hopper"
            | "dispenser"
            | "dropper"
            | "shulker_box"
            | "furnace"
            | "blast_furnace"
            | "smoker"
            | "brewing_stand"
            | "campfire"
            | "chiseled_bookshelf"
            | "brushable_block"
            | "decorated_pot"
            | "jukebox"
            | "lectern"
            | "mob_spawner"
            | "beehive"
            | "banner"
            | "skull"
            | "sign"
            | "hanging_sign"
            | "end_gateway"
            | "beacon"
            | "conduit"
            | "bed"
            | "enchanting_table"
            | "ender_chest"
            | "end_portal"
            | "piston"
            | "structure_block"
            | "jigsaw"
            | "command_block"
            | "bell"
            | "sculk_sensor"
            | "sculk_shrieker"
            | "sculk_catalyst"
            | "calibrated_sculk_sensor"
            | "comparator"
            | "daylight_detector"
    ) {
        return Err(format!("{id}: unsupported block-entity schema"));
    }
    if !context.components() {
        return Ok(());
    }
    reject_commands(data)?;
    item_list(data, "Items", context, level + 1, false)?;
    for key in ["RecordItem", "Book", "item"] {
        item_field(data, key, context, level + 1)?;
    }
    if let Some(value) = data.get_mut("SpawnData")
        && let Some(entity_data) = map_mut(value)?.get_mut("entity")
    {
        entity(map_mut(entity_data)?, context, level + 1)?;
    }
    if let Some(value) = data.get_mut("SpawnPotentials") {
        for (index, entry) in list_mut(value)?.iter_mut().enumerate() {
            if let Some(value) = map_mut(entry)?.get_mut("data")
                && let Some(value) = map_mut(value)?.get_mut("entity")
            {
                entity(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("SpawnPotentials[{index}].data.entity.{e}"))?;
            }
        }
    }
    if let Some(value) = data.get_mut("blockState") {
        blocks::nbt(map_mut(value)?, context)?;
    }
    position(data, "FlowerPos", "flower_pos")?;
    position(data, "ExitPortal", "exit_portal")?;
    if let Some(value) = data.remove("Bees") {
        let mut value = value;
        for (index, bee) in list_mut(&mut value)?.iter_mut().enumerate() {
            let bee = map_mut(bee)?;
            move_field(bee, "EntityData", "entity_data")?;
            move_field(bee, "TicksInHive", "ticks_in_hive")?;
            move_field(bee, "MinOccupationTicks", "min_ticks_in_hive")?;
            if let Some(value) = bee.get_mut("entity_data") {
                entity(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("bees[{index}].entity_data.{e}"))?;
            }
        }
        data.insert("bees".into(), value);
    }
    if let Some(value) = data.remove("Patterns") {
        data.insert("patterns".into(), patterns(value)?);
    }
    let owner = data.remove("SkullOwner");
    let extra = data.remove("ExtraType");
    if let Some(value) = owner.or(extra) {
        data.insert("profile".into(), profile(value)?);
    }
    if let Some(value) = data.get_mut("CustomName") {
        items::text_component(value)?;
    }
    if id == "banner"
        && data
            .get("CustomName")
            .map(|v| items::standard_name("minecraft:white_banner", v))
            .transpose()?
            .unwrap_or(false)
    {
        let name = data.remove("CustomName").unwrap();
        let mut components = take_map(data, "components")?;
        components.insert("minecraft:item_name".into(), name);
        components.insert(
            "minecraft:hide_additional_tooltip".into(),
            V::Compound(Compound::new()),
        );
        data.insert("components".into(), V::Compound(components));
    }
    for side in ["front_text", "back_text"] {
        if let Some(value) = data.get_mut(side) {
            let side = map_mut(value)?;
            for field in ["messages", "filtered_messages"] {
                if let Some(value) = side.get_mut(field) {
                    for value in list_mut(value)? {
                        items::text_component(value)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn reject_commands(data: &Compound) -> Result<()> {
    if data
        .get("Command")
        .is_some_and(|v| !matches!(v, V::String(s) if s.is_empty()))
    {
        return Err("Command: command syntax conversion is not implemented".into());
    }
    Ok(())
}

pub(super) fn item_field(
    data: &mut Compound,
    key: &str,
    context: &Context,
    level: usize,
) -> Result<()> {
    if let Some(value) = data.get_mut(key) {
        let item = map_mut(value)?;
        items::convert(item, context, level).map_err(|e| format!("{key}.{e}"))?;
        if item.is_empty() {
            data.remove(key);
        }
    }
    Ok(())
}

fn item_list(
    data: &mut Compound,
    key: &str,
    context: &Context,
    level: usize,
    preserve_empty: bool,
) -> Result<()> {
    if let Some(value) = data.get_mut(key) {
        let values = list_mut(value)?;
        for (index, value) in values.iter_mut().enumerate() {
            items::convert(map_mut(value)?, context, level)
                .map_err(|e| format!("{key}[{index}].{e}"))?;
        }
        if !preserve_empty {
            values.retain(|v| !matches!(v,V::Compound(c) if c.is_empty()));
        }
    }
    Ok(())
}

fn position(data: &mut Compound, from: &str, to: &str) -> Result<()> {
    if let Some(value) = data.remove(from) {
        let value = position_value(crate::nbt::compound(&value)?)?;
        if data.insert(to.into(), value).is_some() {
            return Err(format!("{from}: duplicate {to}"));
        }
    }
    Ok(())
}

pub(super) fn position_value(data: &Compound) -> Result<V> {
    let mut values = Vec::new();
    for key in ["X", "Y", "Z"] {
        values.push(crate::nbt::number(crate::nbt::get(data, key)?)?);
    }
    Ok(V::IntArray(IntArray::new(values)))
}

pub(super) fn effects(value: &mut V) -> Result<()> {
    for value in list_mut(value)? {
        let effect = map_mut(value)?;
        effect.remove("FactorCalculationData");
        if let Some(value) = effect.get_mut("hidden_effect") {
            let mut nested = V::List(vec![value.clone()]);
            effects(&mut nested)?;
            *value = list_mut(&mut nested)?.remove(0);
        }
    }
    Ok(())
}

pub(super) fn profile(value: V) -> Result<V> {
    if let V::String(name) = value {
        return Ok(V::Compound(Compound::from([(
            "name".into(),
            V::String(name),
        )])));
    }
    let V::Compound(mut profile) = value else {
        return Err("SkullOwner: expected string or compound".into());
    };
    move_field(&mut profile, "Name", "name")?;
    move_field(&mut profile, "Id", "id")?;
    if let Some(value) = profile.remove("Properties") {
        let properties = crate::nbt::compound(&value)?;
        let mut output = Vec::new();
        for (name, value) in properties {
            for value in crate::nbt::list(value)? {
                let mut entry = crate::nbt::compound(value)?.clone();
                move_field(&mut entry, "Value", "value")?;
                move_field(&mut entry, "Signature", "signature")?;
                entry.insert("name".into(), V::String(name.clone()));
                output.push(V::Compound(entry));
            }
        }
        profile.insert("properties".into(), V::List(output));
    }
    Ok(V::Compound(profile))
}

pub(super) fn color(value: &V) -> Result<V> {
    let colors = [
        "white",
        "orange",
        "magenta",
        "light_blue",
        "yellow",
        "lime",
        "pink",
        "gray",
        "light_gray",
        "cyan",
        "purple",
        "blue",
        "brown",
        "green",
        "red",
        "black",
    ];
    let index = crate::nbt::number(value)?;
    colors
        .get(usize::try_from(index).map_err(|_| "negative dye color")?)
        .map(|s| V::String((*s).into()))
        .ok_or_else(|| format!("Invalid dye color {index}"))
}

pub(super) fn patterns(mut value: V) -> Result<V> {
    let patterns: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("data/banner_patterns.json"))
            .map_err(|e| e.to_string())?;
    for entry in list_mut(&mut value)? {
        let data = map_mut(entry)?;
        let code = text(data, "Pattern")?;
        let pattern = patterns
            .get(&code)
            .ok_or_else(|| format!("unknown banner pattern {code}"))?;
        let color = color(crate::nbt::get(data, "Color")?)?;
        data.remove("Pattern");
        data.remove("Color");
        data.insert("pattern".into(), V::String(format!("minecraft:{pattern}")));
        data.insert("color".into(), color);
    }
    Ok(value)
}
