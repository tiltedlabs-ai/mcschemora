use super::*;

pub(super) fn convert(item: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if item.contains_key("tag") || item.contains_key("Count") {
        return Err("modern item contains legacy fields".into());
    }
    let count = item
        .remove("count")
        .map(|v| crate::nbt::number(&v))
        .transpose()?
        .unwrap_or(1);
    item.insert(
        "Count".into(),
        V::Byte(i8::try_from(count).map_err(|_| "count does not fit legacy byte")?),
    );
    let mut components = take_map(item, "components")?;
    let mut tag = take_map(&mut components, "minecraft:custom_data")?;
    let mut display = take_map(&mut tag, "display")?;
    let mut hide = 0;
    let fields: Vec<[String; 2]> =
        serde_json::from_str(include_str!("data/item_fields.json")).map_err(|e| e.to_string())?;
    for [old, new] in fields {
        if let Some(value) = components.remove(&format!("minecraft:{new}")) {
            items::scalar(&new, &value)?;
            insert(&mut tag, &old, value)?;
        }
    }
    for (new, old, mask) in [
        ("enchantments", "Enchantments", 1),
        ("stored_enchantments", "StoredEnchantments", 32),
    ] {
        if let Some(value) = components.remove(&format!("minecraft:{new}")) {
            let mut data = crate::nbt::compound(&value)?.clone();
            tooltip(&mut data, &mut hide, mask)?;
            let levels = take_map(&mut data, "levels")?;
            empty(&data, new)?;
            let mut entries = Vec::new();
            for (id, value) in levels {
                let level = crate::nbt::number(&value)?;
                if !(0..=255).contains(&level) {
                    return Err(format!("{new}: invalid level {level}"));
                }
                entries.push(V::Compound(Compound::from([
                    ("id".into(), V::String(context.rename("enchantment", &id)?)),
                    ("lvl".into(), V::Short(level as i16)),
                ])));
            }
            insert(&mut tag, old, V::List(entries))?;
        }
    }
    if let Some(value) = components.remove("minecraft:unbreakable") {
        let mut value = crate::nbt::compound(&value)?.clone();
        tooltip(&mut value, &mut hide, 4)?;
        empty(&value, "unbreakable")?;
        insert(&mut tag, "Unbreakable", V::Byte(1))?;
    }
    for (new, old) in [
        ("custom_name", "Name"),
        ("lore", "Lore"),
        ("map_color", "MapColor"),
    ] {
        if let Some(mut value) = components.remove(&format!("minecraft:{new}")) {
            if new == "lore" {
                for line in list_mut(&mut value)? {
                    items::text_component(line)?;
                }
            } else if new == "custom_name" {
                items::text_component(&mut value)?;
            }
            insert(&mut display, old, value)?;
        }
    }
    if let Some(value) = components.remove("minecraft:item_name") {
        let id = text(item, "id")?;
        if !items::standard_name(&id, &value)? {
            return Err("minecraft:item_name: nonstandard item name cannot be represented by a legacy custom name".into());
        }
        insert(&mut display, "Name", value)?;
    }
    if let Some(value) = components.remove("minecraft:dyed_color") {
        let mut value = crate::nbt::compound(&value)?.clone();
        tooltip(&mut value, &mut hide, 64)?;
        let color = value.remove("rgb").ok_or("dyed_color.rgb: missing color")?;
        empty(&value, "dyed_color")?;
        insert(&mut display, "color", color)?;
    }
    if !display.is_empty() {
        insert(&mut tag, "display", V::Compound(display))?;
    }
    for (new, old, mask) in [
        ("can_break", "CanDestroy", 8),
        ("can_place_on", "CanPlaceOn", 16),
    ] {
        if let Some(value) = components.remove(&format!("minecraft:{new}")) {
            let mut data = crate::nbt::compound(&value)?.clone();
            tooltip(&mut data, &mut hide, mask)?;
            let predicates = data
                .remove("predicates")
                .ok_or("missing block predicates")?;
            empty(&data, new)?;
            let mut values = Vec::new();
            for predicate in crate::nbt::list(&predicates)? {
                let data = crate::nbt::compound(predicate)?;
                if let Some(V::List(blocks)) = data.get("blocks") {
                    for block in blocks {
                        let mut expanded = data.clone();
                        expanded.insert("blocks".into(), block.clone());
                        values.push(commands::legacy_predicate(&V::Compound(expanded), context)?);
                    }
                } else {
                    values.push(commands::legacy_predicate(predicate, context)?);
                }
            }
            insert(&mut tag, old, V::List(values))?;
        }
    }
    if let Some(value) = components.remove("minecraft:attribute_modifiers") {
        let mut data = crate::nbt::compound(&value)?.clone();
        tooltip(&mut data, &mut hide, 2)?;
        let mut values = data
            .remove("modifiers")
            .ok_or("attribute_modifiers.modifiers: missing list")?;
        empty(&data, "attribute_modifiers")?;
        if crate::nbt::list(&values)?.is_empty() && super::defaults::fallback(&text(item, "id")?)? {
            if text(item, "id")?.ends_with("_horse_armor") {
                return Err("attribute_modifiers: horse armor fallback requires source-effective body defaults".into());
            }
            let mut defaults = super::defaults::attributes(&text(item, "id")?)?;
            values = defaults
                .remove("modifiers")
                .ok_or("item default modifiers are missing")?;
        }
        for value in list_mut(&mut values)? {
            let modifier = map_mut(value)?;
            let operation = match text(modifier, "operation")?.as_str() {
                "add_value" => 0,
                "add_multiplied_base" => 1,
                "add_multiplied_total" => 2,
                _ => return Err("attribute_modifiers: invalid operation".into()),
            };
            modifier.remove("operation");
            insert(modifier, "Operation", V::Int(operation))?;
            let attribute = context.rename("attribute", &text(modifier, "type")?)?;
            modifier.insert("type".into(), V::String(attribute));
            if !matches!(modifier.get("uuid"), Some(V::IntArray(v)) if v.len()==4) {
                return Err("attribute_modifiers: expected four-integer UUID".into());
            }
            if modifier.get("slot") == Some(&V::String("any".into())) {
                modifier.remove("slot");
            }
            if let Some(V::String(slot)) = modifier.get("slot")
                && !matches!(
                    slot.as_str(),
                    "mainhand" | "offhand" | "feet" | "legs" | "chest" | "head"
                )
            {
                return Err(format!(
                    "attribute_modifiers.slot: {slot} cannot be represented"
                ));
            }
            for (new, old) in [
                ("type", "AttributeName"),
                ("name", "Name"),
                ("amount", "Amount"),
                ("uuid", "UUID"),
                ("slot", "Slot"),
            ] {
                move_field(modifier, new, old)?;
            }
        }
        insert(&mut tag, "AttributeModifiers", values)?;
    }
    if let Some(mut value) = components.remove("minecraft:trim") {
        tooltip(map_mut(&mut value)?, &mut hide, 128)?;
        insert(&mut tag, "Trim", value)?;
    }
    if let Some(value) = components.remove("minecraft:map_decorations") {
        insert(&mut tag, "Decorations", super::maps::reverse(value)?)?;
    }
    if let Some(value) = components.remove("minecraft:potion_contents") {
        let mut data = crate::nbt::compound(&value)?.clone();
        for (new, old) in [
            ("potion", "Potion"),
            ("custom_color", "CustomPotionColor"),
            ("custom_effects", "custom_potion_effects"),
        ] {
            if let Some(value) = data.remove(new) {
                insert(&mut tag, old, value)?;
            }
        }
        empty(&data, "potion_contents")?;
    }
    if let Some(value) = components.remove("minecraft:suspicious_stew_effects") {
        insert(&mut tag, "effects", value)?;
    }
    if let Some(value) = components.remove("minecraft:profile") {
        insert(&mut tag, "SkullOwner", super::profiles::reverse(value)?)?;
    }
    if let Some(value) = components.remove("minecraft:firework_explosion") {
        insert(&mut tag, "Explosion", explosion(value)?)?;
    }
    if let Some(value) = components.remove("minecraft:fireworks") {
        let mut data = crate::nbt::compound(&value)?.clone();
        if let Some(value) = data.remove("flight_duration") {
            let flight = match value {
                V::Byte(n) => n as u8 as i32,
                _ => crate::nbt::number(&value)?,
            };
            if !(0..=255).contains(&flight) {
                return Err("fireworks.flight_duration: outside 0..255".into());
            }
            insert(&mut data, "Flight", V::Byte(flight as u8 as i8))?;
        }
        if let Some(value) = data.remove("explosions") {
            insert(
                &mut data,
                "Explosions",
                V::List(
                    crate::nbt::list(&value)?
                        .iter()
                        .cloned()
                        .map(explosion)
                        .collect::<Result<_>>()?,
                ),
            )?;
        }
        insert(&mut tag, "Fireworks", V::Compound(data))?;
    }
    for new in ["written_book_content", "writable_book_content"] {
        if let Some(value) = components.remove(&format!("minecraft:{new}")) {
            let mut book = crate::nbt::compound(&value)?.clone();
            let mut filtered = Compound::new();
            if let Some(value) = book.remove("pages") {
                let mut pages = Vec::new();
                for (index, value) in crate::nbt::list(&value)?.iter().enumerate() {
                    let mut page = crate::nbt::compound(value)?.clone();
                    let raw = page.remove("raw").ok_or("book page.raw: missing text")?;
                    if let Some(value) = page.remove("filtered") {
                        filtered.insert(index.to_string(), value);
                    }
                    empty(&page, "book page")?;
                    pages.push(raw);
                }
                insert(&mut tag, "pages", V::List(pages))?;
            }
            if !filtered.is_empty() {
                insert(&mut tag, "filtered_pages", V::Compound(filtered))?;
            }
            if let Some(value) = book.remove("title") {
                let mut title = crate::nbt::compound(&value)?.clone();
                insert(
                    &mut tag,
                    "title",
                    title.remove("raw").ok_or("book title.raw: missing text")?,
                )?;
                if let Some(value) = title.remove("filtered") {
                    insert(&mut tag, "filtered_title", value)?;
                }
                empty(&title, "book title")?;
            }
            for field in ["author", "generation", "resolved"] {
                if let Some(value) = book.remove(field) {
                    insert(&mut tag, field, value)?;
                }
            }
            empty(&book, new)?;
        }
    }
    if let Some(value) = components.remove("minecraft:lodestone_tracker") {
        let mut data = crate::nbt::compound(&value)?.clone();
        if let Some(value) = data.remove("target") {
            let mut target = crate::nbt::compound(&value)?.clone();
            let pos = target
                .remove("pos")
                .ok_or("lodestone_tracker.target.pos: missing position")?;
            insert(&mut tag, "LodestonePos", legacy_position(&pos)?)?;
            insert(
                &mut tag,
                "LodestoneDimension",
                target
                    .remove("dimension")
                    .ok_or("lodestone_tracker.target.dimension: missing dimension")?,
            )?;
            empty(&target, "lodestone_tracker.target")?;
        }
        if let Some(value) = data.remove("tracked") {
            insert(&mut tag, "LodestoneTracked", value)?;
        }
        empty(&data, "lodestone_tracker")?;
    }
    for (new, old) in [
        ("charged_projectiles", "ChargedProjectiles"),
        ("bundle_contents", "Items"),
    ] {
        if let Some(mut value) = components.remove(&format!("minecraft:{new}")) {
            for (index, value) in list_mut(&mut value)?.iter_mut().enumerate() {
                items::convert(map_mut(value)?, context, level + 1)
                    .map_err(|e| format!("{new}[{index}].{e}"))?;
            }
            if new == "charged_projectiles" {
                insert(
                    &mut tag,
                    "Charged",
                    V::Byte(!crate::nbt::list(&value)?.is_empty() as i8),
                )?;
            }
            insert(&mut tag, old, value)?;
        }
    }
    if let Some(mut value) = components.remove("minecraft:entity_data") {
        entities::entity(map_mut(&mut value)?, context, level + 1)?;
        insert(&mut tag, "EntityTag", value)?;
    }
    if let Some(value) = components.remove("minecraft:bucket_entity_data") {
        for (key, value) in crate::nbt::compound(&value)? {
            insert(&mut tag, key, value.clone())?;
        }
    }
    if let Some(value) = components.remove("minecraft:block_state") {
        insert(&mut tag, "BlockStateTag", value)?;
    }
    block_entity(&mut components, &mut tag, context, level + 1)?;
    if let Some(value) = components.remove("minecraft:hide_additional_tooltip") {
        empty(crate::nbt::compound(&value)?, "hide_additional_tooltip")?;
        hide |= 32;
    }
    if hide != 0 {
        insert(&mut tag, "HideFlags", V::Int(hide))?;
    }
    empty(&components, "components without a legacy representation")?;
    if !tag.is_empty() {
        item.insert("tag".into(), V::Compound(tag));
    }
    Ok(())
}

fn empty(data: &Compound, field: &str) -> Result<()> {
    if data.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{field}: unsupported fields {:?}",
            data.keys().collect::<Vec<_>>()
        ))
    }
}

fn tooltip(data: &mut Compound, hide: &mut i32, mask: i32) -> Result<()> {
    if let Some(value) = data.remove("show_in_tooltip")
        && crate::nbt::number(&value)? == 0
    {
        *hide |= mask;
    }
    Ok(())
}

pub(super) fn legacy_position(value: &V) -> Result<V> {
    let p = crate::nbt::xyz(value)?;
    Ok(V::Compound(
        ["X", "Y", "Z"]
            .into_iter()
            .zip(p)
            .map(|(k, v)| (k.into(), V::Int(v)))
            .collect(),
    ))
}

fn explosion(value: V) -> Result<V> {
    let mut data = crate::nbt::compound(&value)?.clone();
    let shape = match data.remove("shape") {
        Some(V::String(s)) => match s.as_str() {
            "small_ball" => 0,
            "large_ball" => 1,
            "star" => 2,
            "creeper" => 3,
            "burst" => 4,
            _ => return Err("firework_explosion.shape: unknown shape".into()),
        },
        None => 0,
        _ => return Err("firework_explosion.shape: expected string".into()),
    };
    insert(&mut data, "Type", V::Byte(shape))?;
    for (new, old) in [
        ("colors", "Colors"),
        ("fade_colors", "FadeColors"),
        ("has_trail", "Trail"),
        ("has_twinkle", "Flicker"),
    ] {
        move_field(&mut data, new, old)?;
    }
    Ok(V::Compound(data))
}

fn block_entity(
    components: &mut Compound,
    tag: &mut Compound,
    context: &Context,
    level: usize,
) -> Result<()> {
    let mut data = take_map(components, "minecraft:block_entity_data")?;
    if !data.is_empty() {
        super::block_entities::convert(&mut data, context, level)?;
    }
    if let Some(value) = components.remove("minecraft:container") {
        let mut items = Vec::new();
        let mut slots = std::collections::BTreeSet::new();
        for (index, value) in crate::nbt::list(&value)?.iter().enumerate() {
            let mut entry = crate::nbt::compound(value)?.clone();
            let slot =
                crate::nbt::number(&entry.remove("slot").ok_or("container.slot: missing slot")?)?;
            if !(0..=255).contains(&slot) || !slots.insert(slot) {
                return Err("container.slot: out of range or duplicate slot".into());
            }
            let mut item = entry
                .remove("item")
                .ok_or("container.item: missing stack")?;
            empty(&entry, "container entry")?;
            items::convert(map_mut(&mut item)?, context, level)
                .map_err(|e| format!("container[{index}].{e}"))?;
            if !crate::nbt::compound(&item)?.is_empty() {
                insert(map_mut(&mut item)?, "Slot", V::Byte(slot as u8 as i8))?;
                items.push(item);
            }
        }
        insert(&mut data, "Items", V::List(items))?;
    }
    if let Some(value) = components.remove("minecraft:banner_patterns") {
        insert(&mut data, "Patterns", legacy_patterns(value)?)?;
    }
    if let Some(value) = components.remove("minecraft:base_color") {
        insert(&mut data, "Base", legacy_color(&value)?)?;
    }
    for (new, old) in [("pot_decorations", "sherds"), ("lock", "Lock")] {
        if let Some(value) = components.remove(&format!("minecraft:{new}")) {
            insert(&mut data, old, value)?;
        }
    }
    if let Some(value) = components.remove("minecraft:bees") {
        let mut hive = Compound::from([
            ("id".into(), V::String("minecraft:beehive".into())),
            ("bees".into(), value),
        ]);
        super::block_entities::convert(&mut hive, context, level)?;
        insert(
            &mut data,
            "Bees",
            hive.remove("Bees")
                .ok_or("bees: failed legacy conversion")?,
        )?;
    }
    if let Some(value) = components.remove("minecraft:container_loot") {
        let mut loot = crate::nbt::compound(&value)?.clone();
        insert(
            &mut data,
            "LootTable",
            loot.remove("loot_table")
                .ok_or("container_loot.loot_table: missing table")?,
        )?;
        if let Some(value) = loot.remove("seed") {
            insert(&mut data, "LootTableSeed", value)?;
        }
        empty(&loot, "container_loot")?;
    }
    if !data.is_empty() {
        insert(tag, "BlockEntityTag", V::Compound(data))?;
    }
    Ok(())
}

pub(super) fn legacy_color(value: &V) -> Result<V> {
    let name = crate::nbt::string(value)?;
    entities::COLORS
        .iter()
        .position(|color| *color == name)
        .map(|index| V::Int(index as i32))
        .ok_or_else(|| format!("unknown dye color {name}"))
}

pub(super) fn legacy_patterns(mut value: V) -> Result<V> {
    let patterns: std::collections::BTreeMap<String, String> =
        serde_json::from_str(include_str!("data/banner_patterns.json"))
            .map_err(|e| e.to_string())?;
    for value in list_mut(&mut value)? {
        let entry = map_mut(value)?;
        let pattern = crate::catalog::namespace(&text(entry, "pattern")?);
        let code = patterns
            .iter()
            .find(|(_, name)| pattern == format!("minecraft:{name}"))
            .map(|(code, _)| code)
            .ok_or_else(|| format!("unknown banner pattern {pattern}"))?;
        let color = legacy_color(crate::nbt::get(entry, "color")?)?;
        entry.remove("pattern");
        entry.remove("color");
        insert(entry, "Pattern", V::String(code.clone()))?;
        insert(entry, "Color", color)?;
    }
    Ok(value)
}
