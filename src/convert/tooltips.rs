use super::*;
use std::collections::BTreeSet;

const FIELDS: [(&str, Option<&str>); 9] = [
    ("minecraft:attribute_modifiers", Some("modifiers")),
    ("minecraft:dyed_color", Some("rgb")),
    ("minecraft:can_break", Some("predicates")),
    ("minecraft:can_place_on", Some("predicates")),
    ("minecraft:enchantments", Some("levels")),
    ("minecraft:stored_enchantments", Some("levels")),
    ("minecraft:jukebox_playable", None),
    ("minecraft:trim", None),
    ("minecraft:unbreakable", None),
];

const ADDITIONAL: [&str; 17] = [
    "minecraft:banner_patterns",
    "minecraft:bees",
    "minecraft:block_entity_data",
    "minecraft:block_state",
    "minecraft:bundle_contents",
    "minecraft:charged_projectiles",
    "minecraft:container",
    "minecraft:container_loot",
    "minecraft:firework_explosion",
    "minecraft:fireworks",
    "minecraft:instrument",
    "minecraft:map_id",
    "minecraft:painting/variant",
    "minecraft:pot_decorations",
    "minecraft:potion_contents",
    "minecraft:tropical_fish/pattern",
    "minecraft:written_book_content",
];

pub(super) fn prepare(data: &mut Compound, context: &Context) -> Result<()> {
    if context.source >= crate::versions::NBT_TEXT_COMPONENTS {
        return Ok(());
    }
    for (key, field) in FIELDS {
        let Some(value) = data.get_mut(key) else {
            continue;
        };
        let Some(field) = field else {
            continue;
        };
        let simplified = match key {
            "minecraft:attribute_modifiers" => matches!(value, V::List(_)),
            "minecraft:dyed_color" => !matches!(value, V::Compound(_)),
            "minecraft:enchantments" | "minecraft:stored_enchantments" => {
                let fields = crate::nbt::compound(value)?;
                !fields.contains_key("levels") && !fields.contains_key("show_in_tooltip")
            }
            _ => false,
        };
        if simplified {
            *value = V::Compound(Compound::from([(field.into(), value.clone())]));
        }
        if field == "levels" && !map_mut(value)?.contains_key("levels") {
            return Err(format!("{key}.levels: missing enchantment levels"));
        }
        if field == "predicates" {
            let fields = map_mut(value)?;
            if !matches!(fields.get("predicates"), Some(V::List(values)) if !values.is_empty()) {
                let mut predicate = Compound::new();
                for field in ["blocks", "state", "nbt"] {
                    if let Some(value) = fields.remove(field) {
                        predicate.insert(field.into(), value);
                    }
                }
                if !fields.is_empty() {
                    context.loss(key, "fields ignored by the source's single-predicate codec fallback are omitted");
                }
                *value = V::Compound(Compound::from([(
                    "predicates".into(),
                    V::List(vec![V::Compound(predicate)]),
                )]));
            }
        }
    }
    Ok(())
}

pub(super) fn convert(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
        return Ok(());
    }
    if context.forward() {
        forward(data, context)
    } else {
        reverse(data, context)
    }
}

fn flag(value: &V, field: &str) -> Result<bool> {
    match crate::nbt::number(value)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("{field}: expected boolean")),
    }
}

fn unit(data: &mut Compound, key: &str) -> Result<bool> {
    match data.remove(key) {
        Some(V::Compound(value)) if value.is_empty() => Ok(true),
        Some(_) => Err(format!("{key}: expected empty component")),
        None => Ok(false),
    }
}

fn forward(data: &mut Compound, context: &Context) -> Result<()> {
    let mut hidden = BTreeSet::new();
    for (key, field) in FIELDS {
        let Some(value) = data.get_mut(key) else {
            continue;
        };
        if let V::Compound(fields) = value {
            if let Some(show) = fields.remove("show_in_tooltip")
                && !flag(&show, "show_in_tooltip")?
            {
                hidden.insert(key.to_string());
            }
            if let Some(field) = field
                && let Some(inner) = fields.remove(field)
            {
                if !fields.is_empty() {
                    return Err(format!("{key}: unrecognized wrapper fields"));
                }
                *value = inner;
            }
        }
    }
    let hide = unit(data, "minecraft:hide_tooltip")?;
    if unit(data, "minecraft:hide_additional_tooltip")? {
        hidden.extend(
            ADDITIONAL
                .iter()
                .filter(|key| data.contains_key(**key))
                .map(|key| key.to_string()),
        );
    }
    for key in [
        "!minecraft:hide_tooltip",
        "!minecraft:hide_additional_tooltip",
    ] {
        if unit(data, key)? {
            context.loss(key, "removal of an absent vanilla default has no distinct representation after tooltip fields are combined");
        }
    }
    if hide || !hidden.is_empty() {
        super::insert(
            data,
            "minecraft:tooltip_display",
            V::Compound(Compound::from([
                ("hide_tooltip".into(), V::Byte(i8::from(hide))),
                (
                    "hidden_components".into(),
                    V::List(hidden.into_iter().map(V::String).collect()),
                ),
            ])),
        )?;
    }
    Ok(())
}

fn reverse(data: &mut Compound, context: &Context) -> Result<()> {
    let mut hidden = BTreeSet::new();
    if let Some(value) = data.remove("minecraft:tooltip_display") {
        let mut display = crate::nbt::compound(&value)?.clone();
        if let Some(value) = display.remove("hide_tooltip")
            && flag(&value, "tooltip_display.hide_tooltip")?
        {
            super::insert(data, "minecraft:hide_tooltip", V::Compound(Compound::new()))?;
        }
        if let Some(value) = display.remove("hidden_components") {
            for value in crate::nbt::list(&value)? {
                hidden.insert(crate::catalog::namespace(crate::nbt::string(value)?));
            }
        }
        if !display.is_empty() {
            return Err("tooltip_display: unknown fields cannot be represented".into());
        }
    }
    if unit(data, "!minecraft:tooltip_display")? {
        context.loss("!minecraft:tooltip_display", "removal of an absent vanilla default has no distinct representation before tooltip fields were combined");
    }
    for (key, field) in FIELDS {
        if let Some(value) = data.get_mut(key) {
            if let Some(field) = field {
                let mut inner = value.clone();
                if field == "predicates" && matches!(&inner, V::List(values) if values.is_empty()) {
                    return Err(format!("{key}: predicate list must not be empty"));
                }
                if field == "predicates" && matches!(inner, V::Compound(_)) {
                    inner = V::List(vec![inner]);
                }
                if key == "minecraft:dyed_color" {
                    inner = rgb(&inner)?;
                }
                *value = V::Compound(Compound::from([(field.into(), inner)]));
            }
            if hidden.remove(key) {
                map_mut(value)?.insert("show_in_tooltip".into(), V::Byte(0));
            }
        } else if hidden.contains(key) {
            return Err(format!(
                "tooltip_display.{key}: hiding an omitted component needs its source-effective default"
            ));
        }
    }
    if hidden.iter().any(|key| ADDITIONAL.contains(&key.as_str())) {
        if let Some(key) = ADDITIONAL
            .iter()
            .find(|key| data.contains_key(**key) && !hidden.contains(**key))
        {
            return Err(format!(
                "tooltip_display: hiding additional tooltips would also hide visible {key}"
            ));
        }
        for key in ADDITIONAL {
            hidden.remove(key);
        }
        super::insert(
            data,
            "minecraft:hide_additional_tooltip",
            V::Compound(Compound::new()),
        )?;
    }
    if let Some(key) = hidden.first() {
        return Err(format!(
            "tooltip_display: individual hiding of {key} cannot be represented before 1.21.5"
        ));
    }
    Ok(())
}

fn rgb(value: &V) -> Result<V> {
    if let V::List(values) = value {
        if values.len() != 3 {
            return Err("dyed_color: expected three color channels".into());
        }
        let mut color = 0;
        for value in values {
            let channel = match value {
                V::Float(n) => f64::from(*n),
                V::Double(n) => *n,
                _ => return Err("dyed_color: expected floating-point color channels".into()),
            };
            if !channel.is_finite() || !(0.0..=1.0).contains(&channel) {
                return Err("dyed_color: color channel outside 0..1".into());
            }
            color = (color << 8) | (channel * 255.0) as i32;
        }
        Ok(V::Int(color | 0xff000000u32 as i32))
    } else {
        Ok(V::Int(crate::nbt::number(value)?))
    }
}
