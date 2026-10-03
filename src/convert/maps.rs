use super::*;

const EXPLORER_MAPS: [(&str, &str, &str); 11] = [
    ("mansion", "woodland_mansion_map", "mansion"),
    ("monument", "ocean_monument_map", "monument"),
    (
        "trial_chambers",
        "buried_trial_chambers_map",
        "trial_chambers",
    ),
    ("jungle_temple", "jungle_pyramid_map", "explorer_jungle"),
    ("swamp_hut", "swamp_hut_map", "explorer_swamp"),
    ("village_desert", "desert_village_map", "village_desert"),
    ("village_plains", "plains_village_map", "village_plains"),
    ("village_savanna", "savanna_village_map", "village_savanna"),
    ("village_snowy", "snowy_village_map", "village_snowy"),
    ("village_taiga", "taiga_village_map", "village_taiga"),
    ("red_x", "buried_treasure_map", "buried_treasure"),
];

pub(super) fn identity(item: &mut Compound, id: &str, context: &Context) -> Result<String> {
    let id = crate::catalog::namespace(id);
    if !context.crosses(5008) {
        return Ok(id);
    }
    if context.forward() {
        if id != "minecraft:filled_map" {
            return Ok(id);
        }
        let Some(value) = item.get_mut("components") else {
            return Ok(id);
        };
        let components = map_mut(value)?;
        super::components::normalize(components)?;
        let Some(V::Compound(markers)) = components.get("minecraft:map_decorations") else {
            return Ok(id);
        };
        let Some(V::Compound(marker)) = markers.get("+") else {
            return Ok(id);
        };
        let kind = text(marker, "type")?;
        let kind = kind.strip_prefix("minecraft:").unwrap_or(&kind);
        let selected = EXPLORER_MAPS.iter().find(|(name, _, _)| *name == kind);
        if let Some((_, name, translation)) = selected {
            if components.get("minecraft:item_name").is_some_and(|value| {
                matches!(value, V::Compound(text) if text.len() == 1
                    && text.get("translate") == Some(&V::String(format!("filled_map.{translation}"))))
            }) {
                components.remove("minecraft:item_name");
            }
            Ok(format!("minecraft:{name}"))
        } else {
            Ok(id)
        }
    } else {
        let Some((_, _, translation)) = EXPLORER_MAPS
            .iter()
            .find(|(_, name, _)| id == format!("minecraft:{name}"))
        else {
            return Ok(id);
        };
        let components = item
            .entry("components".into())
            .or_insert_with(|| V::Compound(Compound::new()));
        let components = map_mut(components)?;
        super::components::normalize(components)?;
        if !components.contains_key("minecraft:item_name")
            && !components.contains_key("!minecraft:item_name")
        {
            components.insert(
                "minecraft:item_name".into(),
                V::Compound(Compound::from([(
                    "translate".into(),
                    V::String(format!("filled_map.{translation}")),
                )])),
            );
        }
        Ok("minecraft:filled_map".into())
    }
}

const TYPES: [&str; 35] = [
    "player",
    "frame",
    "red_marker",
    "blue_marker",
    "target_x",
    "target_point",
    "player_off_map",
    "player_off_limits",
    "mansion",
    "monument",
    "banner_white",
    "banner_orange",
    "banner_magenta",
    "banner_light_blue",
    "banner_yellow",
    "banner_lime",
    "banner_pink",
    "banner_gray",
    "banner_light_gray",
    "banner_cyan",
    "banner_purple",
    "banner_blue",
    "banner_brown",
    "banner_green",
    "banner_red",
    "banner_black",
    "red_x",
    "village_desert",
    "village_plains",
    "village_savanna",
    "village_snowy",
    "village_taiga",
    "jungle_temple",
    "swamp_hut",
    "trial_chambers",
];

fn number(value: &V) -> Result<f64> {
    match value {
        V::Byte(n) => Ok(*n as f64),
        V::Short(n) => Ok(*n as f64),
        V::Int(n) => Ok(*n as f64),
        V::Long(n) => Ok(*n as f64),
        V::Float(n) => Ok(*n as f64),
        V::Double(n) => Ok(*n),
        _ => Err("map decoration: expected number".into()),
    }
}

pub(super) fn forward(value: V, context: &Context) -> Result<V> {
    let mut output = Compound::new();
    for (index, value) in crate::nbt::list(&value)?.iter().enumerate() {
        let data = crate::nbt::compound(value)?;
        if data
            .keys()
            .any(|k| !matches!(k.as_str(), "id" | "type" | "x" | "z" | "rot"))
        {
            return Err(format!("Decorations[{index}]: unsupported fields"));
        }
        let id = text(data, "id")?;
        let kind = crate::nbt::number(crate::nbt::get(data, "type")?)?;
        let name = TYPES
            .get(usize::try_from(kind).map_err(|_| "Decorations.type: negative type")?)
            .ok_or_else(|| format!("Decorations.type: unknown numeric type{kind}"))?;
        let x = number(crate::nbt::get(data, "x")?)?;
        let z = number(crate::nbt::get(data, "z")?)?;
        let rotation = data.get("rot").map(number).transpose()?.unwrap_or(0.0);
        if !x.is_finite() || !z.is_finite() || !rotation.is_finite() {
            return Err(format!(
                "Decorations[{index}]: nonfinite coordinate or rotation"
            ));
        }
        if rotation as f32 as f64 != rotation {
            context.loss(
                &format!("tag.Decorations[{index}].rot"),
                "map rotation loses precision when represented as a float",
            );
        }
        let next = V::Compound(Compound::from([
            ("type".into(), V::String((*name).into())),
            ("x".into(), V::Double(x)),
            ("z".into(), V::Double(z)),
            ("rotation".into(), V::Float(rotation as f32)),
        ]));
        if output.insert(id.clone(), next).is_some() {
            return Err(format!("Decorations[{index}].id: duplicate marker{id}"));
        }
    }
    Ok(V::Compound(output))
}

pub(super) fn reverse(value: V) -> Result<V> {
    let mut output = Vec::new();
    for (id, value) in crate::nbt::compound(&value)? {
        let data = crate::nbt::compound(value)?;
        if data
            .keys()
            .any(|k| !matches!(k.as_str(), "type" | "x" | "z" | "rotation"))
        {
            return Err(format!("map_decorations.{id}: unsupported fields"));
        }
        let kind = text(data, "type")?;
        let kind = kind.strip_prefix("minecraft:").unwrap_or(&kind);
        let index = TYPES
            .iter()
            .position(|name| *name == kind)
            .ok_or_else(|| format!("map_decorations.{id}: type{kind} has no legacy equivalent"))?;
        let x = number(crate::nbt::get(data, "x")?)?;
        let z = number(crate::nbt::get(data, "z")?)?;
        let rotation = data.get("rotation").map(number).transpose()?.unwrap_or(0.0);
        if !x.is_finite() || !z.is_finite() || !rotation.is_finite() {
            return Err(format!(
                "map_decorations.{id}: nonfinite coordinate or rotation"
            ));
        }
        output.push(V::Compound(Compound::from([
            ("id".into(), V::String(id.clone())),
            ("type".into(), V::Byte(index as i8)),
            ("x".into(), V::Double(x)),
            ("z".into(), V::Double(z)),
            ("rot".into(), V::Double(rotation)),
        ])));
    }
    Ok(V::List(output))
}
