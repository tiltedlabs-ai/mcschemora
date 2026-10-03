use super::*;

pub(super) const COLORS: [&str; 16] = [
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

use fastnbt::IntArray;

pub(super) fn entity(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    context
        .source_registry
        .entity_id(&crate::catalog::namespace(&text(data, "id")?))?;
    super::entity_changes::identity(data, context)?;
    identity(data, context)?;
    super::modern::boat(data, context)?;
    let id = context.rename("entity", &text(data, "id")?)?;
    data.insert("id".into(), V::String(id.clone()));
    context.target.entity_id(&id)?;
    if id == "minecraft:player"
        && let Some(value) = data.get_mut("recipeBook")
    {
        let book = map_mut(value)?;
        for key in ["recipes", "toBeDisplayed"] {
            if let Some(value) = book.get_mut(key) {
                super::references(value, "recipe", context)
                    .map_err(|error| format!("recipeBook.{key}.{error}"))?;
            }
        }
    }

    if id == "minecraft:mannequin" {
        if context.source < 4554 || context.target.data_version < 4554 {
            return Err("mannequin: payload requires stable Java1.21.9+".into());
        }
        if let Some(value) = data.get_mut("profile") {
            super::profiles::convert(value, context)?;
        }
        if let Some(value) = data.get_mut("description") {
            super::text::convert(value, context, level + 1)?;
        }
    }
    super::effects::entity(data, &id, context)?;
    if id == "minecraft:area_effect_cloud" {
        cloud_particle(data, context, level)?;
    }
    if id == "minecraft:salmon" {
        super::modern::salmon(data, context)?;
    }
    historical(data, &id, context)?;
    super::entity_changes::convert(data, &id, context)?;
    if id == "minecraft:painting" {
        let key = if context.target.data_version >= 3090 {
            "variant"
        } else {
            "Motive"
        };
        if let Some(V::String(value)) = data.get_mut(key) {
            *value = context.rename("painting_variant", value)?;
        }
    }
    super::modern::projectile(data, &id, context)?;
    if matches!(id.as_str(), "minecraft:warden" | "minecraft:allay") {
        super::game_events::convert(data, context)?;
    }
    super::equipment::convert(data, &id, context)?;
    if !context.forward() && living(&id, context) {
        super::attributes::entity(data, context)?;
    }
    super::uuids::entity(data, &id, context)?;
    if living(&id, context)
        && let Some(value) = data.get_mut("Attributes")
    {
        for value in list_mut(value)? {
            let attribute = map_mut(value)?;
            if let Some(value) = attribute.get_mut("Name") {
                *value = V::String(context.rename("attribute", crate::nbt::string(value)?)?);
            }
        }
    }
    if context.forward() && living(&id, context) {
        super::attributes::entity(data, context)?;
    }
    if matches!(
        id.as_str(),
        "minecraft:villager" | "minecraft:zombie_villager"
    ) {
        super::villagers::convert(data, context)?;
    }
    if id == "minecraft:spawner_minecart" {
        super::spawners::convert(data, context)?;
    }
    if context.legacy() {
        return legacy(data, context, level);
    }
    if !context.components() {
        return children(data, context, level);
    }
    data.insert("id".into(), V::String(id.clone()));
    if id == "minecraft:arrow" {
        arrow_forward(data)?;
    }
    if id == "minecraft:area_effect_cloud" {
        let mut potion = Compound::new();
        for (old, new) in [
            ("Potion", "potion"),
            ("Color", "custom_color"),
            ("effects", "custom_effects"),
        ] {
            if let Some(mut value) = data.remove(old) {
                if old == "effects" {
                    super::effects::remove_interpolation(
                        &mut value,
                        context,
                        "potion_contents.custom_effects",
                    )?;
                }
                potion.insert(new.into(), value);
            }
        }
        if !potion.is_empty() {
            data.insert("potion_contents".into(), V::Compound(potion));
        }
    }
    if living(&id, context)
        && let Some(value) = data.get_mut("Attributes")
    {
        for value in list_mut(value)? {
            let attribute = map_mut(value)?;
            let name = context.rename("attribute", &text(attribute, "Name")?)?;
            attribute.insert("Name".into(), V::String(name));
        }
    }
    reject_commands(data, context)?;
    inventory(data, context, level)?;
    for key in [
        "Item",
        "item",
        "SaddleItem",
        "ArmorItem",
        "DecorItem",
        "FireworksItem",
        "SelectedItem",
        "weapon",
    ] {
        if item_owner(&text(data, "id")?, key, context) {
            item_field(data, key, context, level + 1)?;
        }
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
    passengers_and_trades(data, context, level)?;
    for key in [
        "BlockState",
        "DisplayState",
        "carriedBlockState",
        "inBlockState",
        "block_state",
    ] {
        if state_owner(&text(data, "id")?, key)
            && let Some(value) = data.get_mut(key)
        {
            blocks::nbt(map_mut(value)?, context).map_err(|e| format!("{key}.{e}"))?;
        }
    }
    if text(data, "id")? == "minecraft:falling_block"
        && let Some(value) = data.get_mut("TileEntityData")
    {
        let payload = map_mut(value)?;
        if super::block_entities::removed(payload, context)? {
            data.remove("TileEntityData");
        } else {
            super::block_entities::convert(payload, context, level + 1)?;
        }
    }
    for (from, to) in [
        ("FlowerPos", "flower_pos"),
        ("HivePos", "hive_pos"),
        ("BeamTarget", "beam_target"),
        ("PatrolTarget", "patrol_target"),
        ("WanderTarget", "wander_target"),
    ] {
        if position_owner(&id, from) {
            position(data, from, to)?;
        }
    }
    if living(&id, context)
        && let Some(value) = data.remove("Leash")
    {
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
    if living(&id, context)
        && let Some(value) = data.get_mut("active_effects")
    {
        super::effects::remove_interpolation(value, context, "active_effects")?;
    }
    if let Some(value) = data.get_mut("CustomName") {
        super::text::convert(value, context, level + 1)?;
    }
    if id == "minecraft:text_display"
        && let Some(value) = data.get_mut("text")
    {
        super::text::convert(value, context, level + 1)?;
    }
    Ok(())
}

pub(super) fn reject_commands(data: &mut Compound, context: &Context) -> Result<()> {
    if !matches!(
        text(data, "id")?.as_str(),
        "minecraft:command_block" | "minecraft:command_block_minecart"
    ) {
        return Ok(());
    }
    if let Some(value) = data.get_mut("Command") {
        let command = crate::nbt::string(value)?;
        *value = V::String(
            super::commands::convert(command, context, 0).map_err(|e| format!("Command.{e}"))?,
        );
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
        context
            .scoped(key, || items::convert(item, context, level))
            .map_err(|e| format!("{key}.{e}"))?;
        if item.is_empty() {
            data.remove(key);
        }
    }
    Ok(())
}

pub(super) fn item_list(
    data: &mut Compound,
    key: &str,
    context: &Context,
    level: usize,
    preserve_empty: bool,
) -> Result<()> {
    if let Some(value) = data.get_mut(key) {
        let values = list_mut(value)?;
        for (index, value) in values.iter_mut().enumerate() {
            context
                .scoped(&format!("{key}[{index}]"), || {
                    items::convert(map_mut(value)?, context, level)
                })
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

pub(super) fn color(value: &V) -> Result<V> {
    let index = crate::nbt::number(value)?;
    COLORS
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

fn inventory(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    for key in ["HandItems", "ArmorItems", "Inventory", "Items"] {
        if !item_owner(&text(data, "id")?, key, context) {
            continue;
        }
        item_list(
            data,
            key,
            context,
            level + 1,
            matches!(key, "HandItems" | "ArmorItems"),
        )?;
    }
    Ok(())
}

fn passengers_and_trades(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if let Some(value) = data.get_mut("Passengers") {
        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
            entity(map_mut(value)?, context, level + 1)
                .map_err(|e| format!("Passengers[{index}].{e}"))?;
        }
    }
    if matches!(
        text(data, "id")?.as_str(),
        "minecraft:villager" | "minecraft:wandering_trader"
    ) && let Some(value) = data.get_mut("Offers")
        && let Some(value) = map_mut(value)?.get_mut("Recipes")
    {
        for (index, value) in list_mut(value)?.iter_mut().enumerate() {
            let recipe = map_mut(value)?;
            for key in ["buy", "buyB", "sell"] {
                item_field(recipe, key, context, level + 1)
                    .map_err(|e| format!("Offers.Recipes[{index}].{e}"))?;
            }
        }
    }
    Ok(())
}

fn children(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if data.get("id") == Some(&V::String("minecraft:potion".into()))
        && context.target.data_version < 2511
    {
        item_field(data, "Potion", context, level + 1)?;
    }
    reject_commands(data, context)?;
    if data.get("id") == Some(&V::String("minecraft:spawner_minecart".into())) {
        super::block_entities::spawns(data, context, level)?;
    }

    inventory(data, context, level)?;
    for key in [
        "Item",
        "item",
        "SaddleItem",
        "ArmorItem",
        "DecorItem",
        "FireworksItem",
        "SelectedItem",
        "weapon",
        "body_armor_item",
        "Trident",
    ] {
        if item_owner(&text(data, "id")?, key, context) {
            item_field(data, key, context, level + 1)?;
        }
    }
    if item_owner(&text(data, "id")?, "HandItems", context)
        && let Some(value) = data.get_mut("equipment")
    {
        let equipment = map_mut(value)?;
        for key in [
            "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
        ] {
            item_field(equipment, key, context, level + 1)?;
        }
        if equipment.keys().any(|key| {
            !matches!(
                key.as_str(),
                "mainhand" | "offhand" | "feet" | "legs" | "chest" | "head" | "body" | "saddle"
            )
        }) {
            return Err("equipment: unknown typed slot".into());
        }
    }
    passengers_and_trades(data, context, level)?;
    for key in [
        "BlockState",
        "DisplayState",
        "carriedBlockState",
        "inBlockState",
        "block_state",
    ] {
        if state_owner(&text(data, "id")?, key)
            && let Some(value) = data.get_mut(key)
        {
            blocks::nbt(map_mut(value)?, context)?;
        }
    }
    if text(data, "id")? == "minecraft:falling_block"
        && let Some(value) = data.get_mut("TileEntityData")
    {
        let payload = map_mut(value)?;
        if super::block_entities::removed(payload, context)? {
            data.remove("TileEntityData");
        } else {
            super::block_entities::convert(payload, context, level + 1)?;
        }
    }
    for key in ["CustomName", "text"] {
        if (key == "CustomName" || text(data, "id")? == "minecraft:text_display")
            && let Some(value) = data.get_mut(key)
        {
            super::text::convert(value, context, level + 1)?;
        }
    }
    Ok(())
}

fn legacy(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    let id = text(data, "id")?;
    children(data, context, level)?;
    if id == "minecraft:arrow" {
        arrow_reverse(data)?;
    }
    if living(&id, context)
        && let Some(value) = data.get_mut("Attributes")
    {
        for value in list_mut(value)? {
            let attribute = map_mut(value)?;
            let name = context.rename("attribute", &text(attribute, "Name")?)?;
            attribute.insert("Name".into(), V::String(name));
        }
    }
    if id == "minecraft:wolf" {
        wolf_inverse(data)?;
    }
    for (from, to) in [
        ("flower_pos", "FlowerPos"),
        ("hive_pos", "HivePos"),
        ("beam_target", "BeamTarget"),
        ("patrol_target", "PatrolTarget"),
        ("wander_target", "WanderTarget"),
    ] {
        if position_owner(&id, to) {
            legacy_position(data, from, to)?;
        }
    }
    if living(&id, context)
        && let Some(value) = data.remove("leash")
    {
        let value = if matches!(value, V::IntArray(_)) {
            super::legacy_items::legacy_position(&value)?
        } else {
            value
        };
        super::insert(data, "Leash", value)?;
    }
    if matches!(
        id.as_str(),
        "minecraft:horse" | "minecraft:llama" | "minecraft:trader_llama"
    ) && data.contains_key("body_armor_item")
    {
        let key = if id == "minecraft:horse" {
            "ArmorItem"
        } else {
            "DecorItem"
        };
        move_field(data, "body_armor_item", key)?;
        if let Some(value) = data.remove("body_armor_drop_chance")
            && value != V::Float(2.0)
        {
            return Err("body_armor_drop_chance: cannot represent a nondefault chance".into());
        }
    } else if living(&id, context)
        && (data.contains_key("body_armor_item") || data.contains_key("body_armor_drop_chance"))
    {
        return Err("body armor cannot be represented for this entity".into());
    }
    if matches!(
        id.as_str(),
        "minecraft:llama" | "minecraft:trader_llama" | "minecraft:donkey" | "minecraft:mule"
    ) && let Some(value) = data.get_mut("Items")
    {
        for value in list_mut(value)? {
            let item = map_mut(value)?;
            let slot = crate::nbt::number(crate::nbt::get(item, "Slot")?)?;
            item.insert(
                "Slot".into(),
                V::Byte(i8::try_from(slot + 2).map_err(|_| "pack inventory slot overflow")?),
            );
        }
    }
    if id == "minecraft:area_effect_cloud"
        && let Some(value) = data.remove("potion_contents")
    {
        let mut potion = crate::nbt::compound(&value)?.clone();
        for (new, old) in [
            ("potion", "Potion"),
            ("custom_color", "Color"),
            ("custom_effects", "effects"),
        ] {
            if let Some(value) = potion.remove(new) {
                super::insert(data, old, value)?;
            }
        }
        if !potion.is_empty() {
            return Err("potion_contents: fields cannot be represented".into());
        }
    }

    Ok(())
}

pub(super) fn legacy_position(data: &mut Compound, from: &str, to: &str) -> Result<()> {
    if let Some(value) = data.remove(from) {
        super::insert(data, to, super::legacy_items::legacy_position(&value)?)?;
    }
    Ok(())
}

fn historical(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if id == "minecraft:tnt_minecart" {
        if context.crosses(4173) {
            if context.forward() {
                move_field(data, "TNTFuse", "fuse")?;
            } else {
                move_field(data, "fuse", "TNTFuse")?;
            }
        }
        for (field, boundary, default) in [
            ("explosion_power", 4059, 4.0),
            ("explosion_speed_factor", 4173, 1.0),
        ] {
            if context.crosses(boundary)
                && !context.forward()
                && let Some(value) = data.remove(field)
            {
                let number = match value {
                    V::Float(n) => n as f64,
                    V::Double(n) => n,
                    _ => return Err(format!("{field}: expected floating point value")),
                };
                if number != default {
                    return Err(format!(
                        "{field}: custom explosion behavior cannot be represented before data version{boundary}"
                    ));
                }
            }
        }
    }

    if id == "minecraft:allay" && context.crosses(3117) {
        if context.forward() {
            if data.contains_key("CanDuplicate") || data.contains_key("DuplicationCooldown") {
                return Err(
                    "allay: pre1.19.1 custom fields collide with new duplication fields".into(),
                );
            }
        } else {
            if data
                .get("CanDuplicate")
                .map(crate::nbt::number)
                .transpose()?
                .is_some_and(|n| n != 0)
            {
                return Err(
                    "CanDuplicate: duplication-enabled allay cannot be represented before1.19.1"
                        .into(),
                );
            }
            if let Some(value) = data.get("DuplicationCooldown") {
                let cooldown = match value {
                    V::Long(value) => *value,
                    V::Int(value) => i64::from(*value),
                    _ => return Err("DuplicationCooldown: expected integer ticks".into()),
                };
                if cooldown != 0 {
                    return Err("DuplicationCooldown: active duplication timer cannot be represented before1.19.1".into());
                }
            }
            data.remove("CanDuplicate");
            data.remove("DuplicationCooldown");
        }
    }
    if id == "minecraft:shulker"
        && context.crosses(2535)
        && let Some(value) = data.get_mut("Rotation")
    {
        let values = list_mut(value)?;
        if values.len() != 2 {
            return Err("Rotation: expected yaw and pitch".into());
        }
        for (index, value) in values.iter_mut().enumerate() {
            let number = match value {
                V::Float(n) => *n,
                _ => return Err("Rotation: expected float angles".into()),
            };
            if !number.is_finite() {
                return Err("Rotation: angle must be finite".into());
            }
            *value = V::Float(if index == 0 { -number } else { number });
        }
    }
    if matches!(
        id,
        "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
    ) && context.crosses(2702)
    {
        if context.forward() {
            if let Some(value) = data.remove("player") {
                let player = crate::nbt::number(&value)? != 0;
                data.entry("pickup".into())
                    .or_insert(V::Byte(i8::from(player)));
            }
        } else if let Some(value) = data.get("pickup") {
            let pickup = crate::nbt::number(value)?;
            if !(0..=2).contains(&pickup) {
                return Err("pickup: invalid arrow pickup mode".into());
            }
            super::insert(data, "player", V::Byte(i8::from(pickup == 1)))?;
        }
    }
    if id == "minecraft:potion" && context.crosses(2511) {
        if context.forward() {
            move_field(data, "Potion", "Item")?;
        } else {
            move_field(data, "Item", "Potion")?;
        }
    }
    if context.crosses(2505)
        && let Some(value) = data.get_mut("Brain")
        && let Some(value) = map_mut(value)?.get_mut("memories")
    {
        for (name, value) in map_mut(value)? {
            if context.forward() {
                *value = V::Compound(Compound::from([("value".into(), value.clone())]));
            } else {
                let memory = map_mut(value)?;
                if memory.keys().any(|key| key != "value") {
                    return Err(format!(
                        "Brain.memories.{name}: expiring memory cannot be represented before1.16"
                    ));
                }
                *value = memory
                    .remove("value")
                    .ok_or_else(|| format!("Brain.memories.{name}.value: missing memory"))?;
            }
        }
    }
    if id == "minecraft:cat" && context.crosses(3086) {
        let variants = [
            "tabby",
            "black",
            "red",
            "siamese",
            "british_shorthair",
            "calico",
            "persian",
            "ragdoll",
            "white",
            "jellie",
            "all_black",
        ];
        if context.forward() {
            let old = data
                .remove("CatType")
                .map(|v| crate::nbt::number(&v))
                .transpose()?
                .unwrap_or(0);
            let variant = usize::try_from(old)
                .ok()
                .and_then(|i| variants.get(i))
                .ok_or("CatType: unknown cat variant")?;
            super::insert(data, "variant", V::String(format!("minecraft:{variant}")))?;
        } else {
            let variant = data
                .remove("variant")
                .unwrap_or_else(|| V::String("minecraft:tabby".into()));
            let variant = crate::catalog::namespace(crate::nbt::string(&variant)?);
            let index = variants
                .iter()
                .position(|v| variant == format!("minecraft:{v}"))
                .ok_or("variant: cat variant cannot be represented before1.19")?;
            super::insert(data, "CatType", V::Int(index as i32))?;
        }
    }
    if id == "minecraft:painting" && context.crosses(3090) {
        let (old, new) = if context.forward() {
            ("Motive", "variant")
        } else {
            ("variant", "Motive")
        };
        if let Some(value) = data.remove(old) {
            let variant = crate::catalog::namespace(crate::nbt::string(&value)?);
            let name = variant
                .strip_prefix("minecraft:")
                .ok_or("painting.variant: unresolved datapack painting")?;
            if !matches!(
                name,
                "kebab"
                    | "aztec"
                    | "alban"
                    | "aztec2"
                    | "bomb"
                    | "plant"
                    | "wasteland"
                    | "pool"
                    | "courbet"
                    | "sea"
                    | "sunset"
                    | "creebet"
                    | "wanderer"
                    | "graham"
                    | "match"
                    | "bust"
                    | "stage"
                    | "void"
                    | "skull_and_roses"
                    | "wither"
                    | "fighters"
                    | "pointer"
                    | "pigscene"
                    | "burning_skull"
                    | "skeleton"
                    | "donkey_kong"
            ) {
                return Err(format!(
                    "painting.variant: {variant} cannot be represented before1.19"
                ));
            }
            super::insert(data, new, V::String(variant))?;
        }
    }
    if id == "minecraft:goat" && context.crosses(3093) {
        for field in ["HasLeftHorn", "HasRightHorn"] {
            if context.forward() {
                super::insert(data, field, V::Byte(1))?;
            } else if let Some(value) = data.remove(field)
                && crate::nbt::number(&value)? == 0
            {
                return Err(format!(
                    "{field}: missing goat horn cannot be represented before1.19"
                ));
            }
        }
    }
    if context.crosses(3683) && id == "minecraft:tnt" {
        if context.forward() {
            move_field(data, "Fuse", "fuse")?;
            super::insert(
                data,
                "block_state",
                V::Compound(Compound::from([(
                    "Name".into(),
                    V::String("minecraft:tnt".into()),
                )])),
            )?;
        } else {
            move_field(data, "fuse", "Fuse")?;
            if let Some(value) = data.remove("block_state") {
                let state = crate::nbt::compound(&value)?;
                if text(state, "Name")? != "minecraft:tnt"
                    || state.keys().any(|k| k != "Name" && k != "Properties")
                    || state
                        .get("Properties")
                        .map(crate::nbt::compound)
                        .transpose()?
                        .is_some_and(|p| !p.is_empty())
                {
                    return Err(
                        "block_state: nondefault TNT state cannot be represented before1.20.3"
                            .into(),
                    );
                }
            }
        }
    }
    if context.crosses(3685) {
        if id == "minecraft:trident" {
            if context.forward() {
                move_field(data, "Trident", "item")?;
            } else {
                move_field(data, "item", "Trident")?;
            }
        }
        if matches!(id, "minecraft:arrow" | "minecraft:spectral_arrow") {
            let item_id = if id == "minecraft:spectral_arrow" {
                "minecraft:spectral_arrow"
            } else if data
                .get("Potion")
                .is_some_and(|v| v != &V::String("minecraft:empty".into()))
            {
                "minecraft:tipped_arrow"
            } else {
                "minecraft:arrow"
            };
            if context.forward() {
                super::insert(
                    data,
                    "item",
                    V::Compound(Compound::from([
                        ("id".into(), V::String(item_id.into())),
                        ("Count".into(), V::Byte(1)),
                    ])),
                )?;
            } else if let Some(value) = data.remove("item") {
                let item = crate::nbt::compound(&value)?;
                if text(item, "id")? != item_id
                    || crate::nbt::number(crate::nbt::get(item, "Count")?)? != 1
                    || item.keys().any(|k| !matches!(k.as_str(), "id" | "Count"))
                {
                    return Err(
                        "item: custom projectile item cannot be represented before1.20.3".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn arrow_forward(data: &mut Compound) -> Result<()> {
    let potion = data.remove("Potion");
    let effects = data.remove("custom_potion_effects");
    let color = data.remove("Color");
    let item = data.entry("item".into()).or_insert_with(|| {
        V::Compound(Compound::from([
            (
                "id".into(),
                V::String(
                    if potion
                        .as_ref()
                        .is_some_and(|p| p != &V::String("minecraft:empty".into()))
                    {
                        "minecraft:tipped_arrow"
                    } else {
                        "minecraft:arrow"
                    }
                    .into(),
                ),
            ),
            ("Count".into(), V::Byte(1)),
        ]))
    });
    let item = map_mut(item)?;
    let mut tag = take_map(item, "tag")?;
    for (key, value) in [
        ("Potion", potion),
        ("custom_potion_effects", effects),
        ("CustomPotionColor", color),
    ] {
        if let Some(value) = value {
            if let Some(existing) = tag.get(key) {
                if existing != &value {
                    return Err(format!(
                        "item.tag.{key}: carried potion data conflicts with arrow effect data"
                    ));
                }
            } else {
                tag.insert(key.into(), value);
            }
        }
    }
    if !tag.is_empty() {
        item.insert("tag".into(), V::Compound(tag));
    }
    Ok(())
}

fn arrow_reverse(data: &mut Compound) -> Result<()> {
    let Some(value) = data.get("item") else {
        return Ok(());
    };
    let item = crate::nbt::compound(value)?;
    let tag = item
        .get("tag")
        .map(crate::nbt::compound)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    for (old, new) in [
        ("Potion", "Potion"),
        ("custom_potion_effects", "custom_potion_effects"),
        ("CustomPotionColor", "Color"),
    ] {
        if let Some(value) = tag.get(old) {
            super::insert(data, new, value.clone())?;
        }
    }
    Ok(())
}

fn identity(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(1904) {
        return Ok(());
    }
    let id = crate::catalog::namespace(&text(data, "id")?);
    if context.forward() && id == "minecraft:ocelot" {
        let kind = data
            .get("CatType")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(0);
        match kind {
            1..=3 => {
                data.insert("id".into(), V::String("minecraft:cat".into()));
            }
            0 => {
                if data
                    .get("OwnerUUID")
                    .map(crate::nbt::string)
                    .transpose()?
                    .is_some_and(|v| !v.is_empty())
                {
                    return Err(
                        "OwnerUUID: owned wild-texture ocelot cannot be represented after1.13"
                            .into(),
                    );
                }
                data.remove("CatType");
            }
            _ => return Err("CatType: invalid ocelot skin".into()),
        }
    } else if !context.forward() {
        if id == "minecraft:cat" {
            if context.crosses(3086)
                && let Some(value) = data.remove("variant")
            {
                let variant = crate::catalog::namespace(crate::nbt::string(&value)?);
                let kind = match variant.as_str() {
                    "minecraft:black" => 1,
                    "minecraft:red" => 2,
                    "minecraft:siamese" => 3,
                    _ => return Err("variant: cat skin cannot be represented before1.14".into()),
                };
                super::insert(data, "CatType", V::Int(kind))?;
            }
            let kind = data
                .get("CatType")
                .map(crate::nbt::number)
                .transpose()?
                .unwrap_or(0);
            if !(1..=3).contains(&kind) {
                return Err("CatType: cat skin cannot be represented before1.14".into());
            }
            data.insert("id".into(), V::String("minecraft:ocelot".into()));
        } else if id == "minecraft:ocelot" {
            if let Some(value) = data.remove("Trusting")
                && crate::nbt::number(&value)? != 0
            {
                return Err(
                    "Trusting: trusting wild ocelot cannot be represented before1.14".into(),
                );
            }
            super::insert(data, "CatType", V::Int(0))?;
        }
    }
    Ok(())
}

fn item_owner(id: &str, key: &str, context: &Context) -> bool {
    let living = living(id, context);
    match key {
        "HandItems" | "ArmorItems" => living,
        "body_armor_item" => matches!(
            id,
            "minecraft:horse" | "minecraft:llama" | "minecraft:trader_llama" | "minecraft:wolf"
        ),
        "Inventory" => matches!(
            id,
            "minecraft:villager"
                | "minecraft:player"
                | "minecraft:wandering_trader"
                | "minecraft:piglin"
                | "minecraft:allay"
        ),
        "Items" => {
            matches!(
                id,
                "minecraft:llama"
                    | "minecraft:trader_llama"
                    | "minecraft:donkey"
                    | "minecraft:mule"
                    | "minecraft:chest_minecart"
                    | "minecraft:hopper_minecart"
                    | "minecraft:chest_boat"
            ) || id.ends_with("_chest_boat")
                || id.ends_with("_chest_raft")
        }
        "Item" => matches!(
            id,
            "minecraft:item"
                | "minecraft:item_frame"
                | "minecraft:glow_item_frame"
                | "minecraft:potion"
                | "minecraft:splash_potion"
                | "minecraft:lingering_potion"
        ),
        "item" => matches!(
            id,
            "minecraft:arrow"
                | "minecraft:spectral_arrow"
                | "minecraft:trident"
                | "minecraft:item_display"
        ),
        "SaddleItem" => matches!(
            id,
            "minecraft:horse"
                | "minecraft:skeleton_horse"
                | "minecraft:zombie_horse"
                | "minecraft:donkey"
                | "minecraft:mule"
                | "minecraft:camel"
        ),
        "ArmorItem" => id == "minecraft:horse",
        "DecorItem" => matches!(id, "minecraft:llama" | "minecraft:trader_llama"),
        "FireworksItem" => id == "minecraft:firework_rocket",
        "SelectedItem" => id == "minecraft:player",
        "weapon" => matches!(
            id,
            "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
        ),
        "Trident" => id == "minecraft:trident",
        _ => false,
    }
}

fn state_owner(id: &str, key: &str) -> bool {
    match key {
        "BlockState" => id == "minecraft:falling_block",
        "DisplayState" => id.ends_with("minecart"),
        "carriedBlockState" => id == "minecraft:enderman",
        "inBlockState" => matches!(
            id,
            "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
        ),
        "block_state" => matches!(id, "minecraft:tnt" | "minecraft:block_display"),
        _ => false,
    }
}

pub(super) fn living(id: &str, context: &Context) -> bool {
    context.target.mob_id(id).is_ok()
        || matches!(
            id,
            "minecraft:armor_stand" | "minecraft:player" | "minecraft:mannequin"
        )
}

fn position_owner(id: &str, key: &str) -> bool {
    match key {
        "FlowerPos" | "HivePos" => id == "minecraft:bee",
        "BeamTarget" => id == "minecraft:end_crystal",
        "PatrolTarget" => matches!(
            id,
            "minecraft:pillager"
                | "minecraft:vindicator"
                | "minecraft:evoker"
                | "minecraft:illusioner"
                | "minecraft:witch"
                | "minecraft:ravager"
        ),
        "WanderTarget" => id == "minecraft:wandering_trader",
        _ => false,
    }
}

fn wolf_inverse(data: &mut Compound) -> Result<()> {
    let Some(owner) = data.get("Owner") else {
        return Ok(());
    };
    if !matches!(owner, V::IntArray(values) if values.len() == 4) {
        return Err("Owner: resolving a named wolf owner requires player-profile context".into());
    }
    let health = match data.get("Health") {
        Some(V::Float(value)) => *value,
        Some(V::Double(value)) => *value as f32,
        _ => return Err("Health: tamed-wolf downgrade requires explicit source health".into()),
    };
    let attributes = data
        .get_mut("Attributes")
        .ok_or("Attributes: tamed-wolf downgrade requires explicit maximum health")?;
    let mut maximum = None;
    for value in list_mut(attributes)? {
        let attribute = map_mut(value)?;
        let name = text(attribute, "Name")?;
        if name == "minecraft:generic.attack_damage"
            && attribute.get("Base") != Some(&V::Double(4.0))
        {
            return Err(
                "Attributes.generic.attack_damage: older taming overwrites a custom attack base"
                    .into(),
            );
        }
        if name != "minecraft:generic.max_health" {
            continue;
        }
        if maximum.is_some() {
            return Err("Attributes.generic.max_health: duplicate maximum health".into());
        }
        if let Some(value) = attribute.get("Modifiers")
            && !crate::nbt::list(value)?.is_empty()
        {
            return Err("Attributes.generic.max_health.Modifiers: older tamed-wolf loading cannot retain source health with these modifiers".into());
        }
        let base = match attribute.get("Base") {
            Some(V::Double(base)) if *base == 20.0 => *base,
            _ => return Err("Attributes.generic.max_health.Base: older taming forces20 and cannot retain a different maximum".into()),
        };
        if health != base as f32 {
            return Err("Health: older tamed-wolf loading heals to20 and cannot preserve partial source health".into());
        }
        maximum = Some(base);
    }
    if maximum.is_none() {
        return Err("Attributes.generic.max_health: omitted modern default is not an old tamed-wolf default".into());
    }
    data.insert("Health".into(), V::Float(20.0));
    Ok(())
}

fn cloud_particle(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if context.crosses(3837) {
        if context.forward() {
            if !data.contains_key("Particle")
                && !["Potion", "Color", "effects"]
                    .iter()
                    .any(|key| data.contains_key(*key))
            {
                data.insert(
                    "Particle".into(),
                    V::String("minecraft:entity_effect".into()),
                );
            }
        } else {
            let potion = data.contains_key("potion_contents");
            if let Some(value) = data.get_mut("Particle") {
                let particle = map_mut(value)?;
                if text(particle, "type")? == "minecraft:entity_effect" {
                    let color = particle
                        .remove("color")
                        .ok_or("Particle.color: missing entity-effect color")?;
                    let color = crate::nbt::number(&color)?;
                    if !potion {
                        if color as u32 >> 24 != 255 {
                            return Err("Particle.color: transparent entity-effect color cannot be represented before1.20.5".into());
                        }
                        super::insert(data, "Color", V::Int(color & 0xffffff))?;
                    }
                }
            } else if !potion {
                super::insert(data, "Color", V::Int(0xffffff))?;
            }
        }
    }
    super::entity_changes::cloud_particle(data, context, level)
}
