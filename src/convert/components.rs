use super::*;

pub(super) fn convert(
    data: &mut Compound,
    context: &Context,
    level: usize,
    id: &str,
) -> Result<()> {
    registered(data, context.source)?;
    super::effects::components(data, context)?;
    if let Some(value) = data.get_mut("minecraft:recipes") {
        references(value, "recipe", context)?;
    }
    if !context.legacy() {
        super::commands::adventure(data, context)?;
    }
    super::tooltips::prepare(data, context)?;
    if let Some(value) = data.get_mut("minecraft:profile") {
        super::profiles::convert(value, context)?;
    }
    for key in ["minecraft:enchantments", "minecraft:stored_enchantments"] {
        if let Some(value) = data.get(key) {
            let value = crate::nbt::compound(value)?;
            let levels = if context.source < crate::versions::NBT_TEXT_COMPONENTS {
                value.get("levels").map(crate::nbt::compound).transpose()?
            } else {
                Some(value)
            };
            if let Some(levels) = levels {
                for (id, level) in levels {
                    super::items::enchantment(id, context)?;
                    let level = crate::nbt::number(level)?;
                    if !(0..=255).contains(&level) {
                        return Err(format!("{key}.{id}: enchantment level outside0..255"));
                    }
                }
            }
        }
    }
    if context.forward() {
        super::item_variants::convert(data, context, id)?;
    }
    super::tooltips::convert(data, context)?;
    super::component_changes::convert(data, context, id)?;
    super::component_changes::inline_values(data, context, level + 1)?;
    sign_components(data, context, level + 1, id)?;
    if let Some(value) = data.get_mut("minecraft:pot_decorations") {
        super::pottery::convert(value, context, level + 1)?;
    }
    if !context.forward() {
        super::item_variants::convert(data, context, id)?;
    }
    if let Some(V::String(variant)) = data.get_mut("minecraft:painting/variant") {
        *variant = context.rename("painting_variant", variant)?;
    }
    if let Some(value) = data.get_mut("minecraft:sulfur_cube_content") {
        let source_id = crate::catalog::namespace(crate::nbt::string(value)?);
        context.source_registry.item(&source_id)?;
        let target_id = context.rename("item", &source_id)?;
        context.target.item(&target_id)?;
        *value = V::String(target_id);
    }
    super::modern::boat_item(data, id, context)?;
    if id == "minecraft:white_banner" {
        super::modern::banner(data, context)?;
    }
    if let Some(value) = data.get_mut("minecraft:lock")
        && !super::modern::lock_value(value, context, level + 1)?
    {
        data.remove("minecraft:lock");
    }
    if id == "minecraft:salmon_bucket"
        && let Some(value) = data.get_mut("minecraft:bucket_entity_data")
    {
        super::modern::salmon(map_mut(value)?, context)?;
    }

    if let Some(value) = data.get_mut("minecraft:attribute_modifiers") {
        super::attributes::item(value, context, id, level + 1)?;
    }
    model_fields(data, context)?;
    for key in ["minecraft:custom_name", "minecraft:item_name"] {
        if let Some(value) = data.get_mut(key) {
            super::text::convert(value, context, level + 1)?;
        }
    }
    if let Some(value) = data.get_mut("minecraft:lore") {
        super::text::lines(value, context, level + 1).map_err(|e| format!("lore{e}"))?;
    }

    if let Some(value) = data.get_mut("minecraft:written_book_content") {
        super::text::book(value, context, level + 1)?;
    }

    if context.target.data_version >= crate::versions::ITEM_COMPONENTS {
        registered(data, context.target.data_version)?;
    }
    Ok(())
}

fn sign_components(data: &mut Compound, context: &Context, level: usize, id: &str) -> Result<()> {
    let downgrade = context.source >= 4996 && context.target.data_version < 4996;
    let mut fields = Compound::new();
    for (component, field) in [
        ("minecraft:sign_text_front", "front_text"),
        ("minecraft:sign_text_back", "back_text"),
    ] {
        if downgrade {
            if data.contains_key(&format!("!{component}")) {
                return Err(format!(
                    "!{component}: component removal has no older block-entity equivalent"
                ));
            }
            if let Some(value) = data.remove(component) {
                fields.insert(field.into(), value);
            }
        } else if let Some(value) = data.get_mut(component) {
            let side = map_mut(value)?;
            for key in ["messages", "filtered_messages"] {
                if let Some(value) = side.get_mut(key) {
                    if crate::nbt::list(value)?.len() != 4 {
                        return Err(format!("{component}.{key}: expected four lines"));
                    }
                    super::text::lines(value, context, level + 1)?;
                }
            }
        }
    }
    if downgrade {
        if data.contains_key("!minecraft:waxed") {
            return Err(
                "!minecraft:waxed: component removal has no older block-entity equivalent".into(),
            );
        }
        if let Some(value) = data.remove("minecraft:waxed") {
            if !crate::nbt::compound(&value)?.is_empty() {
                return Err("waxed: expected an empty component".into());
            }
            fields.insert("is_waxed".into(), V::Byte(1));
        }
        if !fields.is_empty() {
            if !id.ends_with("_sign") {
                return Err(
                    "sign components: this item has no older sign block-entity representation"
                        .into(),
                );
            }
            if data.contains_key("!minecraft:block_entity_data") {
                return Err(
                    "sign components: block entity data conflicts with its explicit removal".into(),
                );
            }
            let block = data
                .entry("minecraft:block_entity_data".into())
                .or_insert_with(|| V::Compound(Compound::new()));
            let block = map_mut(block)?;
            let block_id = if id.ends_with("_hanging_sign") {
                "minecraft:hanging_sign"
            } else {
                "minecraft:sign"
            };
            if let Some(value) = block.get("id") {
                if crate::catalog::namespace(crate::nbt::string(value)?) != block_id {
                    return Err("sign components: conflicting block entity identity".into());
                }
            } else {
                block.insert("id".into(), V::String(block_id.into()));
            }
            for (key, value) in fields {
                super::insert(block, &key, value)?;
            }
        }
    }
    Ok(())
}

pub(super) fn model_fields(data: &mut Compound, context: &Context) -> Result<()> {
    if context.crosses(4175) {
        if let Some(value) = data.get_mut("minecraft:equippable") {
            let value = map_mut(value)?;
            if context.forward() {
                move_field(value, "model", "asset_id")?;
            } else {
                move_field(value, "asset_id", "model")?;
            }
        }
        if let Some(value) = data.get_mut("minecraft:custom_model_data") {
            if context.forward() {
                let old = crate::nbt::number(value)?;
                let new = old as f32;
                if new as f64 != old as f64 {
                    context.loss(
                        "components.minecraft:custom_model_data",
                        "integer custom model value loses precision when represented as a float",
                    );
                }
                *value = V::Compound(Compound::from([(
                    "floats".into(),
                    V::List(vec![V::Float(new)]),
                )]));
            } else {
                let data = crate::nbt::compound(value)?;
                for (key, values) in data {
                    if key != "floats" && !crate::nbt::list(values)?.is_empty() {
                        return Err(format!(
                            "custom_model_data.{key}: cannot be represented as a legacy scalar"
                        ));
                    }
                }
                let floats = crate::nbt::list(crate::nbt::get(data, "floats")?)?;
                if floats.len() != 1 {
                    return Err(
                        "custom_model_data.floats: legacy schema requires exactly one scalar"
                            .into(),
                    );
                }
                let number = match floats[0] {
                    V::Float(n) => n as f64,
                    V::Double(n) => n,
                    _ => return Err("custom_model_data.floats: expected float".into()),
                };
                if !number.is_finite()
                    || number.fract() != 0.0
                    || number < i32::MIN as f64
                    || number > i32::MAX as f64
                {
                    return Err("custom_model_data.floats: noninteger value cannot be represented before1.21.4".into());
                }
                *value = V::Int(number as i32);
            }
        }
    }
    Ok(())
}

pub(super) fn registered(data: &Compound, version: i32) -> Result<()> {
    #[derive(Deserialize)]
    struct Registry {
        data_version: i32,
        ids: std::collections::BTreeSet<String>,
    }
    static REGISTRIES: OnceLock<std::result::Result<Vec<Registry>, String>> = OnceLock::new();
    let registries = REGISTRIES
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/component-ids.json")).map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let registry = registries
        .iter()
        .rev()
        .find(|registry| registry.data_version <= version)
        .ok_or("component schema unavailable before Java1.20.5")?;
    for key in data.keys() {
        let id = crate::catalog::namespace(key.strip_prefix('!').unwrap_or(key));
        if !registry.ids.contains(&id) {
            return Err(format!(
                "{key}: component is unavailable in data version{version} or requires external registry context"
            ));
        }
    }
    Ok(())
}

pub(super) fn normalize(data: &mut Compound) -> Result<()> {
    let mut normalized = Compound::new();
    for (key, value) in std::mem::take(data) {
        let (prefix, id) = key
            .strip_prefix('!')
            .map_or(("", key.as_str()), |id| ("!", id));
        let canonical = crate::catalog::namespace(id);
        if normalized.contains_key(&canonical) || normalized.contains_key(&format!("!{canonical}"))
        {
            return Err(format!("{canonical}: duplicate component patch identity"));
        }
        let id = format!("{prefix}{canonical}");
        if normalized.insert(id.clone(), value).is_some() {
            return Err(format!(
                "{id}: duplicate component identity after namespace normalization"
            ));
        }
    }
    *data = normalized;
    Ok(())
}
