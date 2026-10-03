use super::*;
use std::collections::BTreeMap;

fn foods() -> Result<&'static BTreeMap<String, Compound>> {
    static FOODS: OnceLock<std::result::Result<BTreeMap<String, Compound>, String>> =
        OnceLock::new();
    FOODS
        .get_or_init(|| {
            let data: BTreeMap<String, String> =
                serde_json::from_str(include_str!("data/food-1.21.1.json"))
                    .map_err(|e| e.to_string())?;
            data.into_iter()
                .map(|(id, snbt)| Ok((id, crate::nbt::from_snbt(&snbt)?)))
                .collect()
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn insert(data: &mut Compound, key: &str, value: V) -> Result<()> {
    let opposite = if let Some(key) = key.strip_prefix('!') {
        key.to_owned()
    } else {
        format!("!{key}")
    };
    if data.contains_key(&opposite) {
        return Err(format!("{key}: component value conflicts with its removal"));
    }
    super::insert(data, key, value)
}

fn unit(value: &V, field: &str) -> Result<()> {
    if !crate::nbt::compound(value)?.is_empty() {
        return Err(format!("{field}: expected an empty component"));
    }
    Ok(())
}

pub(super) fn convert(data: &mut Compound, context: &Context, id: &str) -> Result<()> {
    if context.crosses(5007) {
        animations(data, context)?;
    }
    if context.crosses(5008) && context.forward() {
        for key in ["minecraft:map_color", "!minecraft:map_color"] {
            if data.remove(key).is_some() {
                context.loss(
                    key,
                    "the target format no longer stores map color component overrides",
                );
            }
        }
    }
    if context.crosses(4786) {
        registry_sets(data, context)?;
    }
    if context.crosses(4649)
        && let Some(value) = data.get_mut("minecraft:consumable")
        && let Some(value) = map_mut(value)?.get_mut("animation")
    {
        let animation = crate::nbt::string(value)?;
        if context.forward() {
            if animation == "trident" {
                return Err(
                    "consumable.animation.trident: unavailable in the source schema".into(),
                );
            }
            if animation == "spear" {
                *value = V::String("trident".into());
            }
        } else {
            if animation == "spear" {
                return Err("consumable.animation.spear: the new spear animation has no equivalent before 1.21.11".into());
            }
            if animation == "trident" {
                *value = V::String("spear".into());
            }
        }
    }
    if context.crosses(4435) {
        summer_components(data, context)?;
    }
    if context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
        tool_and_equipment(data, context, id)?;
    }
    if context.crosses(4064) {
        resistant(data, context.forward())?;
    }
    if context.crosses(4059) {
        food(data, context, id)?;
    }
    Ok(())
}

fn animations(data: &mut Compound, context: &Context) -> Result<()> {
    for prefix in ["", "!"] {
        let old = format!("{prefix}minecraft:swing_animation");
        let attack = format!("{prefix}minecraft:attack_animation");
        let interact = format!("{prefix}minecraft:interact_animation");
        if context.forward() {
            if let Some(value) = data.remove(&old) {
                if prefix == "!" {
                    unit(&value, &old)?;
                }
                insert(data, &attack, value.clone())?;
                insert(data, &interact, value)?;
            }
        } else {
            let attack_value = data.remove(&attack);
            let interact_value = data.remove(&interact);
            match (attack_value, interact_value) {
                (None, None) => {},
                (Some(attack_value), Some(interact_value)) if attack_value == interact_value => {
                    if prefix == "!" {
                        unit(&attack_value, &attack)?;
                    }
                    insert(data, &old, attack_value)?;
                },
                _ => return Err("attack_animation/interact_animation: independently configured animations cannot be represented by one older swing animation".into()),
            }
        }
    }
    Ok(())
}

pub(super) fn inline_values(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    if let Some(V::Compound(instrument)) = data.get_mut("minecraft:instrument") {
        if context.crosses(4996) {
            if context.forward() && instrument.contains_key("durability_damage") {
                return Err(
                    "instrument.durability_damage: unavailable in the source schema".into(),
                );
            }
            if !context.forward() {
                if let Some(value) = instrument.remove("durability_damage")
                    && crate::nbt::number(&value)? != 0
                {
                    return Err("instrument.durability_damage: instrument durability consumption has no older equivalent".into());
                }
                if let Some(value) = instrument.get("use_duration") {
                    let duration = match value {
                        V::Float(value) => f64::from(*value),
                        V::Double(value) => *value,
                        _ => f64::from(crate::nbt::number(value)?),
                    };
                    if duration <= 0.0 || !duration.is_finite() {
                        return Err("instrument.use_duration: the older schema requires a positive duration".into());
                    }
                }
            }
        }
        if let Some(value) = instrument.get_mut("description") {
            super::text::convert(value, context, level + 1)?;
        }
    }
    if let Some(V::Compound(consumable)) = data.get_mut("minecraft:consumable")
        && let Some(effects) = consumable.get_mut("on_consume_effects")
    {
        for effect in list_mut(effects)? {
            let effect = map_mut(effect)?;
            if context.crosses(4996)
                && effect
                    .get("type")
                    .and_then(|v| crate::nbt::string(v).ok())
                    .is_some_and(|id| {
                        crate::catalog::namespace(id) == "minecraft:teleport_randomly"
                    })
                && let Some(value) = effect.get("directional_particles")
            {
                if context.forward() {
                    return Err(
                        "teleport_randomly.directional_particles: unavailable in the source schema"
                            .into(),
                    );
                }
                if crate::nbt::number(value)? != 0 {
                    return Err("teleport_randomly.directional_particles: directional particles have no older equivalent".into());
                }
                effect.remove("directional_particles");
            }
        }
    }
    for key in ["minecraft:trim", "minecraft:provides_trim_material"] {
        let Some(mut value) = data.get_mut(key) else {
            continue;
        };
        if key == "minecraft:trim" {
            let Some(material) = map_mut(value)?.get_mut("material") else {
                continue;
            };
            value = material;
        }
        let V::Compound(material) = value else {
            continue;
        };
        if context.crosses(4996) {
            if context.forward() {
                let asset = text(material, "asset_name")?;
                let asset = vanilla_palette(&asset)?;
                let overrides = material
                    .get("override_armor_assets")
                    .map(crate::nbt::compound)
                    .transpose()?
                    .cloned()
                    .unwrap_or_default();
                if overrides != palette_overrides(&asset) {
                    return Err("trim material.override_armor_assets: embedded armor overrides require equivalent target resource-pack palettes".into());
                }
                material.remove("override_armor_assets");
                material.remove("asset_name");
                insert(
                    material,
                    "palette_id",
                    V::String(format!("minecraft:trim/{asset}")),
                )?;
            } else {
                let palette = text(material, "palette_id")?;
                let palette = crate::catalog::namespace(&palette);
                let asset = palette.strip_prefix("minecraft:trim/").ok_or(
                    "trim material: custom palette conversion requires resource-pack context",
                )?;
                let asset = vanilla_palette(asset)?;
                material.remove("palette_id");
                let overrides = palette_overrides(&asset);
                if !overrides.is_empty() {
                    insert(material, "override_armor_assets", V::Compound(overrides))?;
                }
                insert(material, "asset_name", V::String(asset))?;
            }
        }
        if let Some(value) = material.get_mut("description") {
            super::text::convert(value, context, level + 1)?;
        }
    }
    Ok(())
}

fn vanilla_palette(id: &str) -> Result<String> {
    let id = crate::catalog::namespace(id);
    if !matches!(
        id.as_str(),
        "minecraft:quartz"
            | "minecraft:iron"
            | "minecraft:netherite"
            | "minecraft:redstone"
            | "minecraft:copper"
            | "minecraft:gold"
            | "minecraft:emerald"
            | "minecraft:diamond"
            | "minecraft:lapis"
            | "minecraft:amethyst"
            | "minecraft:resin"
    ) {
        return Err(
            "trim material: custom palette conversion requires resource-pack context".into(),
        );
    }
    Ok(id.trim_start_matches("minecraft:").into())
}

fn palette_overrides(asset: &str) -> Compound {
    if matches!(asset, "iron" | "netherite" | "copper" | "gold" | "diamond") {
        Compound::from([(
            format!("minecraft:{asset}"),
            V::String(format!("{asset}_darker")),
        )])
    } else {
        Compound::new()
    }
}

fn registry_sets(data: &mut Compound, context: &Context) -> Result<()> {
    #[derive(Deserialize)]
    struct Tags {
        registries: BTreeMap<String, BTreeMap<String, std::collections::BTreeSet<String>>>,
    }
    static TAGS: OnceLock<std::result::Result<Tags, String>> = OnceLock::new();
    for (component, field, registry) in [
        ("minecraft:provides_banner_patterns", None, "banner_pattern"),
        ("minecraft:damage_resistant", Some("types"), "damage_type"),
        (
            "minecraft:blocks_attacks",
            Some("bypassed_by"),
            "damage_type",
        ),
    ] {
        let Some(mut value) = data.get_mut(component) else {
            continue;
        };
        if let Some(field) = field {
            let Some(nested) = map_mut(value)?.get_mut(field) else {
                continue;
            };
            value = nested;
        }
        if matches!(value, V::String(id) if id.starts_with('#')) {
            continue;
        }
        let path = format!(
            "{component}{}",
            field.map(|field| format!(".{field}")).unwrap_or_default()
        );
        if context.forward() {
            return Err(format!(
                "{path}: the source schema accepts registry tags only"
            ));
        }
        if context.target.data_version != 4671 {
            return Err(format!(
                "{path}: exact registry-set conversion requires the Java 1.21.11 target tag snapshot"
            ));
        }
        let values = if let V::List(values) = value {
            values.as_slice()
        } else {
            std::slice::from_ref(value)
        };
        let ids = values
            .iter()
            .map(|value| {
                let id = crate::nbt::string(value)?;
                if id.starts_with('#') {
                    return Err("registry set: tags cannot occur inside an explicit list".into());
                }
                Ok(crate::catalog::namespace(id))
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        let tags = TAGS
            .get_or_init(|| {
                serde_json::from_str(include_str!("data/registry-tags-1.21.11.json"))
                    .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(Clone::clone)?;
        let (tag, _) = tags.registries[registry]
            .iter()
            .find(|(_, values)| **values == ids)
            .ok_or_else(|| {
                format!("{path}: explicit entries have no equivalent vanilla target tag")
            })?;
        context.loss(&path, "explicit registry entries become an equivalent vanilla tag, losing their distinct reference form");
        *value = V::String(format!("#{tag}"));
    }
    Ok(())
}

fn summer_components(data: &mut Compound, context: &Context) -> Result<()> {
    if context.target.data_version >= 4435
        && let Some(value) = data.get("minecraft:painting/variant")
        && !matches!(value, V::String(_))
    {
        return Err("painting/variant: inline definitions require a target registry entry from Java 1.21.6 onward".into());
    }
    if !context.forward()
        && let Some(value) = data.get_mut("minecraft:attribute_modifiers")
    {
        let modifiers = list_mut(value)?;
        let all_hidden = !modifiers.is_empty()
            && modifiers.iter().all(|value| {
                matches!(value, V::Compound(fields)
                if matches!(fields.get("display"), Some(V::Compound(display))
                    if display.get("type") == Some(&V::String("hidden".into()))))
            });
        if all_hidden {
            for value in modifiers {
                map_mut(value)?.remove("display");
            }
            if data.contains_key("!minecraft:tooltip_display") {
                return Err("attribute_modifiers.display: hidden modifiers conflict with tooltip_display removal".into());
            }
            let tooltip = data
                .entry("minecraft:tooltip_display".into())
                .or_insert_with(|| V::Compound(Compound::new()));
            let hidden = map_mut(tooltip)?
                .entry("hidden_components".into())
                .or_insert_with(|| V::List(Vec::new()));
            let hidden = list_mut(hidden)?;
            let attribute = V::String("minecraft:attribute_modifiers".into());
            if !hidden.contains(&attribute) {
                hidden.push(attribute);
            }
        }
    }
    if let Some(value) = data.get_mut("minecraft:equippable") {
        let equipment = map_mut(value)?;
        if context.forward() {
            for key in ["can_be_sheared", "shearing_sound"] {
                if equipment.contains_key(key) {
                    return Err(format!(
                        "equippable.{key}: field is unavailable in the source schema"
                    ));
                }
            }
        } else {
            if let Some(value) = equipment.remove("can_be_sheared")
                && crate::nbt::number(&value)? != 0
            {
                return Err("equippable.can_be_sheared: shearing equipment cannot be represented before 1.21.6".into());
            }
            if let Some(value) = equipment.remove("shearing_sound")
                && crate::nbt::string(&value)? != "minecraft:item.shears.snip"
            {
                context.loss(
                    "equippable.shearing_sound",
                    "the disabled equipment shearing sound is absent from the target schema",
                );
            }
        }
    }
    Ok(())
}

fn tool_and_equipment(data: &mut Compound, context: &Context, id: &str) -> Result<()> {
    let creative =
        !(id.ends_with("_sword") || matches!(id, "minecraft:mace" | "minecraft:trident"));
    if !creative && data.contains_key("!minecraft:tool") {
        return Err("!minecraft:tool: removing tool behavior cannot preserve this item's old creative-mode block-breaking restriction".into());
    }
    if let Some(value) = data.get_mut("minecraft:tool") {
        let tool = map_mut(value)?;
        let field = "can_destroy_blocks_in_creative";
        if context.forward() {
            if tool.contains_key(field) {
                return Err(format!(
                    "tool.{field}: field is unavailable in the source schema"
                ));
            }
            if !creative {
                tool.insert(field.into(), V::Byte(0));
            }
        } else {
            let allowed = tool
                .remove(field)
                .map(|value| crate::nbt::number(&value))
                .transpose()?
                .unwrap_or(1);
            if allowed != i32::from(creative) {
                return Err(format!(
                    "tool.{field}: this item's older hardcoded behavior cannot represent the selected value"
                ));
            }
        }
    }
    if let Some(value) = data.get_mut("minecraft:equippable") {
        let equipment = map_mut(value)?;
        if !context.forward() {
            if equipment.get("slot") == Some(&V::String("saddle".into())) {
                return Err(
                    "equippable.slot: saddle equipment cannot be represented before 1.21.5".into(),
                );
            }
            if let Some(value) = equipment.remove("equip_on_interact")
                && crate::nbt::number(&value)? != 0
            {
                return Err("equippable.equip_on_interact: interaction equipping cannot be represented before 1.21.5".into());
            }
        } else if equipment.contains_key("equip_on_interact") {
            return Err(
                "equippable.equip_on_interact: field is unavailable in the source schema".into(),
            );
        }
    }
    Ok(())
}

fn resistant(data: &mut Compound, forward: bool) -> Result<()> {
    let (old, new) = if forward {
        ("minecraft:fire_resistant", "minecraft:damage_resistant")
    } else {
        ("minecraft:damage_resistant", "minecraft:fire_resistant")
    };
    if let Some(value) = data.remove(old) {
        let result = if forward {
            unit(&value, old)?;
            V::Compound(Compound::from([(
                "types".into(),
                V::String("#minecraft:is_fire".into()),
            )]))
        } else {
            let fields = crate::nbt::compound(&value)?;
            if fields.len() != 1
                || fields.get("types") != Some(&V::String("#minecraft:is_fire".into()))
            {
                return Err("damage_resistant: only the vanilla fire damage tag can be represented before Java 1.21.2".into());
            }
            V::Compound(Compound::new())
        };
        insert(data, new, result)?;
    }
    if let Some(value) = data.remove(&format!("!{old}")) {
        unit(&value, old)?;
        insert(data, &format!("!{new}"), value)?;
    }
    Ok(())
}

fn native_consumption(id: &str) -> bool {
    matches!(
        id,
        "minecraft:honey_bottle"
            | "minecraft:potion"
            | "minecraft:milk_bucket"
            | "minecraft:ominous_bottle"
    )
}

fn food(data: &mut Compound, context: &Context, id: &str) -> Result<()> {
    let touched = [
        "minecraft:food",
        "!minecraft:food",
        "minecraft:consumable",
        "!minecraft:consumable",
        "minecraft:use_remainder",
        "!minecraft:use_remainder",
    ]
    .iter()
    .any(|key| data.contains_key(*key));
    if !touched {
        return Ok(());
    }
    if native_consumption(id) {
        return Err(format!(
            "{id}: customized consumption needs its native item behavior to be resolved"
        ));
    }
    let defaults = foods()?.get(id);
    if context.forward() {
        if let Some(value) = data.remove("minecraft:food") {
            let mut food = crate::nbt::compound(&value)?.clone();
            let seconds = food.remove("eat_seconds").unwrap_or(V::Float(1.6));
            let mut effects = Vec::new();
            if let Some(value) = food.remove("effects") {
                for value in crate::nbt::list(&value)? {
                    let mut old = crate::nbt::compound(value)?.clone();
                    let effect = old
                        .remove("effect")
                        .ok_or("food.effects.effect is missing")?;
                    let probability = old.remove("probability").unwrap_or(V::Float(1.0));
                    if !old.is_empty() {
                        return Err("food.effects: unknown fields cannot be translated".into());
                    }
                    effects.push(V::Compound(Compound::from([
                        ("type".into(), V::String("minecraft:apply_effects".into())),
                        ("effects".into(), V::List(vec![effect])),
                        ("probability".into(), probability),
                    ])));
                }
            }
            if id == "minecraft:chorus_fruit" {
                effects.push(V::Compound(Compound::from([(
                    "type".into(),
                    V::String("minecraft:teleport_randomly".into()),
                )])));
            }
            if let Some(remainder) = food.remove("using_converts_to") {
                insert(data, "minecraft:use_remainder", remainder)?;
            } else if defaults.is_some_and(|d| d.contains_key("using_converts_to")) {
                insert(
                    data,
                    "!minecraft:use_remainder",
                    V::Compound(Compound::new()),
                )?;
            }
            insert(data, "minecraft:food", V::Compound(food))?;
            insert(
                data,
                "minecraft:consumable",
                V::Compound(Compound::from([
                    ("consume_seconds".into(), seconds),
                    ("on_consume_effects".into(), V::List(effects)),
                ])),
            )?;
        }
        if let Some(value) = data.get("!minecraft:food") {
            unit(value, "!minecraft:food")?;
            insert(data, "!minecraft:consumable", V::Compound(Compound::new()))?;
        }
        return Ok(());
    }
    if let Some(value) = data.remove("!minecraft:consumable") {
        unit(&value, "!minecraft:consumable")?;
        if data.contains_key("minecraft:food") || data.contains_key("minecraft:consumable") {
            return Err("food without consumption cannot retain its explicit nutrition data before Java 1.21.2".into());
        }
        if let Some(value) = data.get("!minecraft:food") {
            unit(value, "!minecraft:food")?;
        } else {
            insert(data, "!minecraft:food", value)?;
        }
        if data.contains_key("minecraft:use_remainder")
            || data.contains_key("!minecraft:use_remainder")
        {
            return Err(
                "use_remainder without consumption cannot be represented before Java 1.21.2".into(),
            );
        }
        return Ok(());
    }
    if data.contains_key("!minecraft:food") {
        return Err("consumption without food needs an explicit legacy nutrition policy".into());
    }
    let explicit_food = data.remove("minecraft:food");
    let mut food = if let Some(value) = explicit_food {
        crate::nbt::compound(&value)?.clone()
    } else {
        defaults.cloned().ok_or(
            "consumption on a nonfood item cannot be downgraded without explicit nutrition data",
        )?
    };
    if let Some(value) = data.remove("minecraft:consumable") {
        let mut consumable = crate::nbt::compound(&value)?.clone();
        for (key, expected) in [
            ("animation", V::String("eat".into())),
            ("sound", V::String("minecraft:entity.generic.eat".into())),
            ("has_consume_particles", V::Byte(1)),
        ] {
            if let Some(value) = consumable.remove(key)
                && value != expected
            {
                return Err(format!(
                    "consumable.{key}: nondefault consumption behavior cannot be represented before Java 1.21.2"
                ));
            }
        }
        let seconds = consumable
            .remove("consume_seconds")
            .unwrap_or(V::Float(1.6));
        food.insert("eat_seconds".into(), seconds);
        let mut old_effects = Vec::new();
        let mut effects = consumable
            .remove("on_consume_effects")
            .map(|value| crate::nbt::list(&value).cloned())
            .transpose()?
            .unwrap_or_default();
        if id == "minecraft:chorus_fruit" {
            let value = effects.pop().ok_or(
                "chorus_fruit: legacy consumption always teleports after applying food effects",
            )?;
            let mut teleport = crate::nbt::compound(&value)?.clone();
            let kind = teleport
                .remove("type")
                .ok_or("consume effect.type is missing")?;
            if crate::catalog::namespace(crate::nbt::string(&kind)?)
                != "minecraft:teleport_randomly"
            {
                return Err(
                    "chorus_fruit: legacy consumption requires a final teleport effect".into(),
                );
            }
            if let Some(value) = teleport.remove("diameter") {
                let diameter = match value {
                    V::Float(n) => f64::from(n),
                    V::Double(n) => n,
                    _ => return Err("teleport diameter must be a floating point value".into()),
                };
                if diameter != 16.0 {
                    return Err("chorus_fruit: customized teleport distance cannot be represented before Java 1.21.2".into());
                }
            }
            if !teleport.is_empty() {
                return Err("chorus_fruit: unsupported teleport fields".into());
            }
        }
        for value in &effects {
            let mut effect = crate::nbt::compound(value)?.clone();
            if crate::catalog::namespace(&text(&effect, "type")?) != "minecraft:apply_effects" {
                return Err(
                    "consumable.on_consume_effects: this effect type has no legacy food equivalent"
                        .into(),
                );
            }
            effect.remove("type");
            let effects = effect
                .remove("effects")
                .ok_or("consume effect.effects is missing")?;
            let effects = crate::nbt::list(&effects)?;
            let probability = effect.remove("probability").unwrap_or(V::Float(1.0));
            if !effect.is_empty() {
                return Err("consume effect: unknown fields cannot be translated".into());
            }
            if effects.len() > 1 && probability != V::Float(1.0) {
                return Err("consume effect: correlated probabilities cannot be represented by independent legacy effects".into());
            }
            for effect in effects {
                old_effects.push(V::Compound(Compound::from([
                    ("effect".into(), effect.clone()),
                    ("probability".into(), probability.clone()),
                ])));
            }
        }
        if !consumable.is_empty() {
            return Err("consumable: unknown fields cannot be represented".into());
        }
        food.insert("effects".into(), V::List(old_effects));
    } else {
        let defaults = defaults.ok_or(
            "food on a nonfood item without consumable cannot be represented before Java 1.21.2",
        )?;
        for key in ["eat_seconds", "effects"] {
            if let Some(value) = defaults.get(key) {
                food.insert(key.into(), value.clone());
            }
        }
    }
    if let Some(value) = data.remove("minecraft:use_remainder") {
        food.insert("using_converts_to".into(), value);
    } else if let Some(value) = data.remove("!minecraft:use_remainder") {
        unit(&value, "!minecraft:use_remainder")?;
        food.remove("using_converts_to");
    } else if let Some(value) = defaults.and_then(|d| d.get("using_converts_to")) {
        food.insert("using_converts_to".into(), value.clone());
    }
    insert(data, "minecraft:food", V::Compound(food))
}
