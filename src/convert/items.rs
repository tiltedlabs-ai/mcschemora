use super::*;

pub(super) fn convert(item: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    if item.is_empty() {
        return Ok(());
    }
    let mut source_id = text(item, "id")?;
    let original_id = source_id.clone();
    if context.crosses(3209)
        && context.forward()
        && source_id.ends_with("_spawn_egg")
        && let Some(V::Compound(tag)) = item.get("tag")
        && let Some(V::Compound(entity)) = tag.get("EntityTag")
        && let Some(value) = entity.get("id")
    {
        let id = context.rename("entity", crate::nbt::string(value)?)?;
        let corrected = format!("{id}_spawn_egg");
        if crate::catalog::namespace(&source_id) != corrected {
            context.loss(
                "id",
                "spawn egg item identity is corrected to its embedded entity identity",
            );
        }
        source_id = corrected;
    }
    let count = match item.get(if context.source < crate::versions::ITEM_COMPONENTS {
        "Count"
    } else {
        "count"
    }) {
        Some(value) => crate::nbt::number(value)?,
        None if context.source >= crate::versions::ITEM_COMPONENTS => 1,
        None => return Err("Count: missing item count".into()),
    };
    if crate::catalog::namespace(&source_id) == "minecraft:air" || count <= 0 {
        item.clear();
        return Ok(());
    }
    if count > 99
        && (context.source >= crate::versions::ITEM_COMPONENTS
            || context.target.data_version >= crate::versions::ITEM_COMPONENTS)
    {
        return Err("Count: count above 99 cannot be represented".into());
    }
    context
        .source_registry
        .item(&crate::catalog::namespace(&original_id))?;
    source_id = super::maps::identity(item, &source_id, context)?;
    let id = context.rename("item", &source_id)?;
    context.target.item(&id)?;
    item.insert("id".into(), V::String(id.clone()));
    super::effects::item(item, &id, context)?;
    if context.source < crate::versions::ITEM_COMPONENTS
        && let Some(value) = item.get_mut("tag")
        && let Some(value) = map_mut(value)?.get_mut("BlockStateTag")
    {
        blocks::item(
            value,
            &crate::catalog::namespace(&original_id),
            &id,
            context,
        )?;
    }
    if context.source >= crate::versions::ITEM_COMPONENTS
        && let Some(value) = item.get_mut("components")
    {
        let components = map_mut(value)?;
        super::components::normalize(components)?;
        if let Some(value) = components.get_mut("minecraft:block_state") {
            blocks::item(
                value,
                &crate::catalog::namespace(&original_id),
                &id,
                context,
            )?;
        }
        super::components::convert(components, context, level, &id)?;
    }
    if context.legacy() {
        super::legacy_items::convert(item, context, level)?;
        return historical(item, &id, context);
    }
    historical(item, &id, context)?;
    if !context.components() {
        return children(item, &id, context, level);
    }
    if item.contains_key("components") || item.contains_key("count") {
        return Err("source item already contains modern fields".into());
    }
    item.remove("Count");
    item.insert("count".into(), V::Int(count));
    let mut tag = take_map(item, "tag")?;
    let mut components = Compound::new();
    let hide = tag
        .remove("HideFlags")
        .map(|v| crate::nbt::number(&v))
        .transpose()?
        .unwrap_or(0);
    let fields: Vec<[String; 2]> =
        serde_json::from_str(include_str!("data/item_fields.json")).map_err(|e| e.to_string())?;
    for [old, new] in fields {
        if let Some(value) = tag.remove(&old) {
            scalar(&new, &value)?;
            put(&mut components, &new, value);
        }
    }
    if let Some(value) = tag.remove("Unbreakable")
        && crate::nbt::number(&value)? != 0
    {
        put(
            &mut components,
            "unbreakable",
            V::Compound(tooltip(Compound::new(), hide & 4 != 0)),
        );
    }
    for (old, new, mask) in [
        ("Enchantments", "enchantments", 1),
        ("StoredEnchantments", "stored_enchantments", 32),
    ] {
        if let Some(value) = tag
            .remove(old)
            .or_else(|| (hide & mask != 0).then_some(V::List(Vec::new())))
        {
            let mut levels = Compound::new();
            for value in crate::nbt::list(&value)? {
                let value = crate::nbt::compound(value)?;
                let id = context.rename("enchantment", &text(value, "id")?)?;
                let level = crate::nbt::number(crate::nbt::get(value, "lvl")?)?;
                if !(0..=255).contains(&level) {
                    return Err(format!(
                        "{old}: enchantment level {level} is outside 0..255"
                    ));
                }
                if levels
                    .insert(crate::catalog::namespace(&id), V::Int(level))
                    .is_some()
                {
                    return Err(format!("{old}: duplicate enchantment {id}"));
                }
            }
            put(
                &mut components,
                new,
                V::Compound(tooltip(
                    Compound::from([("levels".into(), V::Compound(levels))]),
                    hide & mask != 0,
                )),
            );
        }
    }
    let mut display = take_map(&mut tag, "display")?;
    if let Some(mut name) = display.remove("Name") {
        super::text::convert(&mut name, context, level + 1)?;
        put(&mut components, "custom_name", name);
    }
    if let Some(mut lore) = display.remove("Lore") {
        for line in list_mut(&mut lore)? {
            super::text::convert(line, context, level + 1)?;
        }
        put(&mut components, "lore", lore);
    }
    if let Some(value) = display.remove("color") {
        put(
            &mut components,
            "dyed_color",
            V::Compound(tooltip(
                Compound::from([("rgb".into(), value)]),
                hide & 64 != 0,
            )),
        );
    }
    if let Some(value) = display.remove("MapColor") {
        put(&mut components, "map_color", value);
    }
    if !display.is_empty() {
        tag.insert("display".into(), V::Compound(display));
    }
    for (old, new, mask) in [
        ("CanDestroy", "can_break", 8),
        ("CanPlaceOn", "can_place_on", 16),
    ] {
        if let Some(value) = tag.remove(old) {
            let mut predicates = Vec::new();
            for value in crate::nbt::list(&value)? {
                let predicate = crate::nbt::string(value)?;
                predicates.push(super::commands::predicate(predicate, context)?);
            }
            if predicates.is_empty() {
                predicates.push(V::Compound(Compound::from([(
                    "blocks".into(),
                    V::List(Vec::new()),
                )])));
            }
            put(
                &mut components,
                new,
                V::Compound(tooltip(
                    Compound::from([("predicates".into(), V::List(predicates))]),
                    hide & mask != 0,
                )),
            );
        }
    }
    if hide & 2 != 0 && !tag.contains_key("AttributeModifiers") {
        if id.ends_with("_horse_armor") {
            return Err(
                "HideFlags: horse armor requires source-effective body attribute defaults".into(),
            );
        }
        put(
            &mut components,
            "attribute_modifiers",
            V::Compound(tooltip(super::defaults::attributes(&id)?, true)),
        );
    }
    if let Some(value) = tag.remove("AttributeModifiers") {
        let mut modifiers = Vec::new();
        for value in crate::nbt::list(&value)? {
            let mut modifier = crate::nbt::compound(value)?.clone();
            let operation = crate::nbt::number(crate::nbt::get(&modifier, "Operation")?)?;
            let operation = match operation {
                0 => "add_value",
                1 => "add_multiplied_base",
                2 => "add_multiplied_total",
                _ => return Err("AttributeModifiers: invalid Operation".into()),
            };
            modifier.remove("Operation");
            modifier.insert("operation".into(), V::String(operation.into()));
            for (old, new) in [
                ("AttributeName", "type"),
                ("Name", "name"),
                ("Amount", "amount"),
                ("UUID", "uuid"),
                ("Slot", "slot"),
            ] {
                move_field(&mut modifier, old, new)?;
            }
            let attribute = context.rename("attribute", &text(&modifier, "type")?)?;
            modifier.insert("type".into(), V::String(attribute));
            if !matches!(modifier.get("uuid"), Some(V::IntArray(v)) if v.len()==4) {
                return Err("AttributeModifiers: expected four-integer UUID".into());
            }
            modifiers.push(V::Compound(modifier));
        }
        if modifiers.is_empty() && super::defaults::fallback(&id)? {
            return Err("AttributeModifiers: explicit empty override cannot suppress the target item's armor fallback".into());
        }
        put(
            &mut components,
            "attribute_modifiers",
            V::Compound(tooltip(
                Compound::from([("modifiers".into(), V::List(modifiers))]),
                hide & 2 != 0,
            )),
        );
    }
    if let Some(mut value) = tag.remove("Trim") {
        if hide & 128 != 0 {
            map_mut(&mut value)?.insert("show_in_tooltip".into(), V::Byte(0));
        }
        put(&mut components, "trim", value);
    }
    let mut potion = Compound::new();
    for (old, new) in [
        ("Potion", "potion"),
        ("CustomPotionColor", "custom_color"),
        ("custom_potion_effects", "custom_effects"),
    ] {
        if let Some(mut value) = tag.remove(old) {
            if new == "custom_effects" {
                super::effects::remove_interpolation(
                    &mut value,
                    context,
                    "components.minecraft:potion_contents.custom_effects",
                )?;
            }
            potion.insert(new.into(), value);
        }
    }
    if !potion.is_empty() {
        put(&mut components, "potion_contents", V::Compound(potion));
    }
    if let Some(mut value) = tag.remove("effects") {
        super::effects::remove_interpolation(
            &mut value,
            context,
            "components.minecraft:suspicious_stew_effects",
        )?;
        put(&mut components, "suspicious_stew_effects", value);
    }
    if let Some(value) = tag.remove("SkullOwner") {
        put(&mut components, "profile", profiles::forward(value)?);
    }
    if let Some(value) = tag.remove("Explosion") {
        put(&mut components, "firework_explosion", explosion(value)?);
    }
    if let Some(value) = tag.remove("Fireworks") {
        let V::Compound(mut fireworks) = value else {
            return Err("Fireworks: expected compound".into());
        };
        if let Some(value) = fireworks.remove("Flight") {
            let flight = match value {
                V::Byte(value) => i32::from(value as u8),
                _ => crate::nbt::number(&value)?,
            };
            if !(0..=255).contains(&flight) {
                return Err("Fireworks.Flight: expected duration in 0..255".into());
            }
            fireworks.insert("flight_duration".into(), V::Byte(flight as u8 as i8));
        }
        if let Some(value) = fireworks.remove("Explosions") {
            let explosions = crate::nbt::list(&value)?
                .iter()
                .cloned()
                .map(explosion)
                .collect::<Result<Vec<_>>>()?;
            fireworks.insert("explosions".into(), V::List(explosions));
        }
        put(&mut components, "fireworks", V::Compound(fireworks));
    }
    if id == "minecraft:written_book" || id == "minecraft:writable_book" {
        let mut book = Compound::new();
        let filtered = take_map(&mut tag, "filtered_pages")?;
        if let Some(value) = tag.remove("pages") {
            let mut pages = Vec::new();
            for (index, value) in crate::nbt::list(&value)?.iter().enumerate() {
                let mut value = value.clone();
                if id == "minecraft:written_book" {
                    super::text::convert(&mut value, context, level + 1)?;
                }
                let mut page = Compound::from([("raw".into(), value)]);
                if let Some(value) = filtered.get(&index.to_string()) {
                    let mut value = value.clone();
                    if id == "minecraft:written_book" {
                        super::text::convert(&mut value, context, level + 1)?;
                    }
                    page.insert("filtered".into(), value);
                }
                pages.push(V::Compound(page));
            }
            book.insert("pages".into(), V::List(pages));
        }
        if id == "minecraft:written_book" {
            {
                let value = tag
                    .remove("title")
                    .unwrap_or_else(|| V::String(String::new()));
                let mut title = Compound::from([("raw".into(), value)]);
                if let Some(value) = tag.remove("filtered_title") {
                    title.insert("filtered".into(), value);
                }
                book.insert("title".into(), V::Compound(title));
            }
            for field in ["author", "generation", "resolved"] {
                if let Some(value) = tag.remove(field) {
                    book.insert(field.into(), value);
                }
            }
        }
        if !book.is_empty() {
            put(
                &mut components,
                if id == "minecraft:written_book" {
                    "written_book_content"
                } else {
                    "writable_book_content"
                },
                V::Compound(book),
            );
        }
    }
    if tag.contains_key("LodestonePos")
        || tag.contains_key("LodestoneDimension")
        || tag.contains_key("LodestoneTracked")
    {
        let mut tracker = Compound::new();
        if tag.contains_key("LodestonePos") != tag.contains_key("LodestoneDimension") {
            return Err("LodestonePos and LodestoneDimension must both be present".into());
        }
        if let (Some(pos), Some(dimension)) =
            (tag.remove("LodestonePos"), tag.remove("LodestoneDimension"))
        {
            let pos = entities::position_value(crate::nbt::compound(&pos)?)?;
            tracker.insert(
                "target".into(),
                V::Compound(Compound::from([
                    ("pos".into(), pos),
                    ("dimension".into(), dimension),
                ])),
            );
        }
        if let Some(value) = tag.remove("LodestoneTracked") {
            tracker.insert("tracked".into(), value);
        }
        put(&mut components, "lodestone_tracker", V::Compound(tracker));
    }
    for (old, new) in [
        ("ChargedProjectiles", "charged_projectiles"),
        ("Items", "bundle_contents"),
    ] {
        if old == "Items" && id != "minecraft:bundle" {
            continue;
        }
        if let Some(mut value) = tag.remove(old) {
            let values = list_mut(&mut value)?;
            for (index, value) in values.iter_mut().enumerate() {
                convert(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("{old}[{index}].{e}"))?;
            }
            values.retain(|v| !matches!(v,V::Compound(c) if c.is_empty()));
            put(&mut components, new, value);
        }
    }
    if id == "minecraft:crossbow" {
        tag.remove("Charged");
    }
    if let Some(mut value) = tag.remove("EntityTag") {
        let data = map_mut(&mut value)?;
        if !data.contains_key("id") {
            let entity_id = if let Some(id) = id.strip_suffix("_spawn_egg") {
                Some(id)
            } else {
                match id.as_str() {
                    "minecraft:armor_stand" => Some("minecraft:armor_stand"),
                    "minecraft:item_frame" => Some("minecraft:item_frame"),
                    "minecraft:glow_item_frame" => Some("minecraft:glow_item_frame"),
                    "minecraft:painting" => Some("minecraft:painting"),
                    _ => None,
                }
            };
            data.insert(
                "id".into(),
                V::String(entity_id.ok_or("EntityTag: cannot infer entity ID")?.into()),
            );
        }
        entities::entity(data, context, level + 1)?;
        put(&mut components, "entity_data", value);
    }
    if let Some(value) = tag.remove("BlockEntityTag") {
        block_entity_components(value, &id, &mut components, context, level + 1)?;
    }
    if let Some(mut value) = tag.remove("BlockStateTag") {
        for state in map_mut(&mut value)?.values_mut() {
            *state = V::String(match state {
                V::String(s) => s.clone(),
                V::Byte(n) => n.to_string(),
                V::Short(n) => n.to_string(),
                V::Int(n) => n.to_string(),
                V::Long(n) => n.to_string(),
                _ => return Err("BlockStateTag: unsupported property type".into()),
            });
        }
        put(&mut components, "block_state", value);
    }
    if id.ends_with("_bucket") {
        let mut bucket = Compound::new();
        for field in [
            "NoAI",
            "Silent",
            "NoGravity",
            "Glowing",
            "Invulnerable",
            "Health",
            "Age",
            "Variant",
            "BucketVariantTag",
            "HuntingCooldown",
        ] {
            if let Some(value) = tag.remove(field) {
                bucket.insert(field.into(), value);
            }
        }
        if !bucket.is_empty() {
            put(&mut components, "bucket_entity_data", V::Compound(bucket));
        }
    }
    if let Some(value) = tag.remove("Decorations") {
        put(
            &mut components,
            "map_decorations",
            super::maps::forward(value, context)?,
        );
    }
    if hide & 32 != 0 {
        put(
            &mut components,
            "hide_additional_tooltip",
            V::Compound(Compound::new()),
        );
    }
    if !tag.is_empty() {
        put(&mut components, "custom_data", V::Compound(tag));
    }
    if let Some(value) = components.get("minecraft:custom_name")
        && standard_name(&id, value)?
    {
        move_field(
            &mut components,
            "minecraft:custom_name",
            "minecraft:item_name",
        )?;
    }
    if !components.is_empty() {
        item.insert("components".into(), V::Compound(components));
    }
    Ok(())
}

fn put(data: &mut Compound, key: &str, value: V) {
    data.insert(format!("minecraft:{key}"), value);
}
fn tooltip(mut data: Compound, hidden: bool) -> Compound {
    if hidden {
        data.insert("show_in_tooltip".into(), V::Byte(0));
    }
    data
}

fn explosion(value: V) -> Result<V> {
    let V::Compound(mut value) = value else {
        return Err("Explosion: expected compound".into());
    };
    let shape = value
        .remove("Type")
        .map(|v| crate::nbt::number(&v))
        .transpose()?
        .unwrap_or(0);
    let shape = match shape {
        0 => "small_ball",
        1 => "large_ball",
        2 => "star",
        3 => "creeper",
        4 => "burst",
        _ => return Err("Explosion: invalid Type".into()),
    };
    value.insert("shape".into(), V::String(shape.into()));
    for (old, new) in [
        ("Colors", "colors"),
        ("FadeColors", "fade_colors"),
        ("Trail", "has_trail"),
        ("Flicker", "has_twinkle"),
    ] {
        move_field(&mut value, old, new)?;
    }
    Ok(V::Compound(value))
}

fn block_entity_components(
    value: V,
    id: &str,
    components: &mut Compound,
    context: &Context,
    level: usize,
) -> Result<()> {
    let V::Compound(mut data) = value else {
        return Err("BlockEntityTag: expected compound".into());
    };
    block_entity_id(&mut data, id)?;
    if super::block_entities::removed(&data, context)? {
        return Ok(());
    }
    super::block_entities::convert(&mut data, context, level)?;
    for (old, new) in [
        ("patterns", "banner_patterns"),
        ("sherds", "pot_decorations"),
        ("bees", "bees"),
        ("Lock", "lock"),
    ] {
        if let Some(value) = data.remove(old) {
            put(components, new, value);
        }
    }
    if let Some(value) = data.remove("Base") {
        put(components, "base_color", entities::color(&value)?);
    }
    if let Some(value) = data.remove("Items") {
        let mut container = Vec::new();
        for value in crate::nbt::list(&value)? {
            let mut item = crate::nbt::compound(value)?.clone();
            if item.is_empty() {
                continue;
            }
            let slot = match item.remove("Slot") {
                Some(V::Byte(n)) => i32::from(n as u8),
                _ => return Err("BlockEntityTag.Items.Slot: expected byte".into()),
            };
            container.push(V::Compound(Compound::from([
                ("slot".into(), V::Int(slot)),
                ("item".into(), V::Compound(item)),
            ])));
        }
        put(components, "container", V::List(container));
    }
    if let Some(table) = data.remove("LootTable") {
        let mut loot = Compound::from([("loot_table".into(), table)]);
        if let Some(seed) = data.remove("LootTableSeed") {
            loot.insert("seed".into(), seed);
        }
        put(components, "container_loot", V::Compound(loot));
    }
    if data.len() > 1 {
        put(components, "block_entity_data", V::Compound(data));
    }
    Ok(())
}

pub(super) fn text_component(value: &mut V) -> Result<()> {
    let text = crate::nbt::string(value)?;
    if text.len() > 1024 * 1024 {
        return Err("text exceeds 1 MiB".into());
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map_err(|e| format!("invalid text component: {e}"))?;
    Ok(())
}

pub(super) fn standard_name(id: &str, value: &V) -> Result<bool> {
    let V::String(value) = value else {
        return Ok(false);
    };
    let names: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(include_str!("data/item_names.json")).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(value).map_err(|e| e.to_string())?;
    Ok(json
        .get("translate")
        .and_then(|v| v.as_str())
        .is_some_and(|name| {
            names
                .get(id)
                .is_some_and(|names| names.iter().any(|n| n == name))
        }))
}

pub(super) fn scalar(name: &str, value: &V) -> Result<()> {
    match name {
        "damage" | "repair_cost" | "map_id" => {
            let number = crate::nbt::number(value).map_err(|e| format!("{name}: {e}"))?;
            if number < 0 {
                return Err(format!("{name}: expected nonnegative integer"));
            }
        }
        "custom_model_data" => {
            crate::nbt::number(value).map_err(|e| format!("{name}: {e}"))?;
        }
        "instrument" => {
            crate::nbt::string(value).map_err(|e| format!("{name}: {e}"))?;
        }
        "recipes" => {
            for value in crate::nbt::list(value)? {
                crate::nbt::string(value)?;
            }
        }
        "debug_stick_state" => {
            for value in crate::nbt::compound(value)?.values() {
                crate::nbt::string(value)?;
            }
        }
        _ => return Err(format!("unknown scalar component {name}")),
    }
    Ok(())
}

fn block_entity_id(data: &mut Compound, id: &str) -> Result<()> {
    if !data.contains_key("id") {
        let name = id.strip_prefix("minecraft:").unwrap_or(id);
        let kind = if name.ends_with("shulker_box") {
            Some("shulker_box")
        } else if name.ends_with("_banner") || name == "shield" {
            Some("banner")
        } else if name.ends_with("_head") || name.ends_with("_skull") {
            Some("skull")
        } else if name.ends_with("_hanging_sign") {
            Some("hanging_sign")
        } else if name.ends_with("_sign") {
            Some("sign")
        } else {
            match name {
                "chest" | "trapped_chest" | "barrel" | "hopper" | "furnace" | "blast_furnace"
                | "smoker" | "brewing_stand" | "dispenser" | "dropper" | "lectern" | "jukebox"
                | "decorated_pot" | "chiseled_bookshelf" | "beehive" | "beacon" | "conduit"
                | "command_block" | "structure_block" | "jigsaw" => Some(name),
                "bee_nest" => Some("beehive"),
                "spawner" => Some("mob_spawner"),
                _ => None,
            }
        };
        data.insert(
            "id".into(),
            V::String(format!(
                "minecraft:{}",
                kind.ok_or("BlockEntityTag: cannot infer block-entity ID")?
            )),
        );
    }
    Ok(())
}

fn children(item: &mut Compound, id: &str, context: &Context, level: usize) -> Result<()> {
    if context.source < crate::versions::ITEM_COMPONENTS {
        if let Some(value) = item.get_mut("tag") {
            let tag = map_mut(value)?;
            for key in ["ChargedProjectiles", "Items"] {
                if key == "Items" && id != "minecraft:bundle" {
                    continue;
                }
                if let Some(value) = tag.get_mut(key) {
                    for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                        convert(map_mut(value)?, context, level + 1)
                            .map_err(|e| format!("tag.{key}[{index}].{e}"))?;
                    }
                }
            }
            if let Some(value) = tag.get_mut("BlockEntityTag") {
                let data = map_mut(value)?;
                block_entity_id(data, id)?;
                super::block_entities::convert(data, context, level + 1)
                    .map_err(|e| format!("tag.BlockEntityTag.{e}"))?;
            }
            if let Some(value) = tag.get_mut("EntityTag") {
                let data = map_mut(value)?;
                if !data.contains_key("id") {
                    let entity = id
                        .strip_suffix("_spawn_egg")
                        .or_else(|| {
                            matches!(
                                id,
                                "minecraft:armor_stand"
                                    | "minecraft:item_frame"
                                    | "minecraft:glow_item_frame"
                                    | "minecraft:painting"
                            )
                            .then_some(id)
                        })
                        .ok_or("EntityTag: cannot infer entity ID")?;
                    data.insert("id".into(), V::String(entity.into()));
                }
                entities::entity(data, context, level + 1)
                    .map_err(|e| format!("tag.EntityTag.{e}"))?;
            }
            if id == "minecraft:written_book" {
                if let Some(value) = tag.get_mut("pages") {
                    for value in list_mut(value)? {
                        super::text::convert(value, context, level + 1)?;
                    }
                }
                if let Some(value) = tag.get_mut("filtered_pages") {
                    for value in map_mut(value)?.values_mut() {
                        super::text::convert(value, context, level + 1)?;
                    }
                }
            }
            if let Some(value) = tag.get_mut("display") {
                let display = map_mut(value)?;
                if let Some(value) = display.get_mut("Name") {
                    super::text::convert(value, context, level + 1)?;
                }
                if context.target.data_version >= 1803
                    && let Some(value) = display.get_mut("Lore")
                {
                    for value in list_mut(value)? {
                        super::text::convert(value, context, level + 1)?;
                    }
                }
            }
        }
    } else if let Some(value) = item.get_mut("components") {
        let components = map_mut(value)?;
        if let Some(value) = components.get_mut("minecraft:use_remainder") {
            convert(map_mut(value)?, context, level + 1)
                .map_err(|e| format!("components.minecraft:use_remainder.{e}"))?;
        }
        if let Some(value) = components.get_mut("minecraft:food")
            && let Some(value) = map_mut(value)?.get_mut("using_converts_to")
        {
            convert(map_mut(value)?, context, level + 1)
                .map_err(|e| format!("components.minecraft:food.using_converts_to.{e}"))?;
        }
        for key in ["minecraft:charged_projectiles", "minecraft:bundle_contents"] {
            if let Some(value) = components.get_mut(key) {
                for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                    convert(map_mut(value)?, context, level + 1)
                        .map_err(|e| format!("components.{key}[{index}].{e}"))?;
                }
            }
        }
        if let Some(value) = components.get_mut("minecraft:container") {
            for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                if let Some(value) = map_mut(value)?.get_mut("item") {
                    convert(map_mut(value)?, context, level + 1)
                        .map_err(|e| format!("components.container[{index}].item.{e}"))?;
                }
            }
        }
        if let Some(value) = components.get_mut("minecraft:entity_data") {
            entities::entity(map_mut(value)?, context, level + 1)?;
        }
        if let Some(value) = components.get_mut("minecraft:block_entity_data") {
            let data = map_mut(value)?;
            if super::block_entities::removed(data, context)? {
                components.remove("minecraft:block_entity_data");
            } else {
                super::block_entities::convert(data, context, level + 1)?;
            }
        }
        if let Some(value) = components.get_mut("minecraft:bees") {
            for (index, value) in list_mut(value)?.iter_mut().enumerate() {
                if let Some(value) = map_mut(value)?.get_mut("entity_data") {
                    entities::entity(map_mut(value)?, context, level + 1)
                        .map_err(|e| format!("components.bees[{index}].entity_data.{e}"))?;
                }
            }
        }
    }
    Ok(())
}

fn historical(item: &mut Compound, id: &str, context: &Context) -> Result<()> {
    let Some(value) = item.get_mut("tag") else {
        return Ok(());
    };
    let tag = map_mut(value)?;
    if context.source < crate::versions::ITEM_COMPONENTS
        && let Some(value) = tag.get_mut("Recipes")
    {
        references(value, "recipe", context)?;
    }
    if context.source < crate::versions::ITEM_COMPONENTS
        && context.target.data_version < crate::versions::ITEM_COMPONENTS
    {
        for key in ["CanDestroy", "CanPlaceOn"] {
            if let Some(value) = tag.get_mut(key) {
                for value in list_mut(value)? {
                    *value = commands::historical_predicate(crate::nbt::string(value)?, context)?;
                }
            }
        }
    }

    for key in ["Enchantments", "StoredEnchantments"] {
        if let Some(value) = tag.get(key) {
            for (index, value) in crate::nbt::list(value)?.iter().enumerate() {
                let effect = crate::nbt::compound(value)?;
                enchantment(&text(effect, "id")?, context)
                    .map_err(|e| format!("{key}[{index}].{e}"))?;
            }
        }
    }
    if context.target.data_version < 2724
        && matches!(id, "minecraft:written_book" | "minecraft:writable_book")
    {
        if let Some(value) = tag.remove("filtered_title")
            && tag.get("title") != Some(&value)
        {
            return Err(
                "filtered_title: distinct filtered text cannot be represented before Java1.17"
                    .into(),
            );
        }
        if let Some(value) = tag.remove("filtered_pages") {
            let pages = tag
                .get("pages")
                .map(crate::nbt::list)
                .transpose()?
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for (index, filtered) in crate::nbt::compound(&value)? {
                let index: usize = index
                    .parse()
                    .map_err(|_| "filtered_pages: invalid page index")?;
                if pages.get(index) != Some(filtered) {
                    return Err("filtered_pages: distinct filtered text cannot be represented before Java1.17".into());
                }
            }
        }
    }
    if context.crosses(1803)
        && let Some(value) = tag.get_mut("display")
        && let Some(value) = map_mut(value)?.get_mut("Lore")
    {
        for value in list_mut(value)? {
            let text = crate::nbt::string(value)?;
            let text = if context.forward() {
                serde_json::to_string(&serde_json::json!({"text": text}))
                    .map_err(|e| e.to_string())?
            } else {
                let parsed: serde_json::Value =
                    serde_json::from_str(text).map_err(|e| format!("display.Lore: {e}"))?;
                match parsed {
                    serde_json::Value::String(text) => text,
                    serde_json::Value::Object(mut object) if object.len() == 1 => object
                        .remove("text")
                        .and_then(|v| v.as_str().map(str::to_owned))
                        .ok_or("display.Lore: rich text cannot be represented before1.14")?,
                    _ => {
                        return Err(
                            "display.Lore: rich text cannot be represented before1.14".into()
                        );
                    }
                }
            };
            *value = V::String(text);
        }
    }
    if let Some(value) = tag.get_mut("AttributeModifiers") {
        for value in list_mut(value)? {
            let modifier = map_mut(value)?;
            if context.crosses(2514) {
                super::uuids::pair(modifier, "UUIDMost", "UUIDLeast", "UUID", context.forward())?;
            }
            if let Some(value) = modifier.get_mut("AttributeName") {
                *value = V::String(context.rename("attribute", crate::nbt::string(value)?)?);
            }
        }
    }
    if id == "minecraft:player_head"
        && context.crosses(2514)
        && let Some(V::Compound(owner)) = tag.get_mut("SkullOwner")
    {
        super::uuids::string(owner, "Id", "Id", context.forward())?;
    }
    Ok(())
}

pub(super) fn enchantment(id: &str, context: &Context) -> Result<()> {
    static ENCHANTMENTS: OnceLock<
        std::result::Result<std::collections::BTreeMap<String, i32>, String>,
    > = OnceLock::new();
    let table = ENCHANTMENTS
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/enchantments.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let mut id = crate::catalog::namespace(id);
    if id == "minecraft:sweeping_edge" {
        id = "minecraft:sweeping".into();
    }
    let minimum = table
        .get(&id)
        .ok_or_else(|| format!("unresolved external enchantment {id}"))?;
    if context.source < *minimum || context.target.data_version < *minimum {
        return Err(format!(
            "enchantment {id} is unavailable in the source or target release"
        ));
    }
    Ok(())
}
