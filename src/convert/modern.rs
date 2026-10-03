use super::*;

fn long(value: &V) -> Result<i64> {
    match value {
        V::Long(value) => Ok(*value),
        V::Int(value) => Ok(i64::from(*value)),
        _ => Err("expected an integer tick count".into()),
    }
}

pub(super) fn jukebox(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(3945) {
        return Ok(());
    }
    if context.forward() {
        let tick = data.get("TickCount").map(long).transpose()?.unwrap_or(0);
        let start = data
            .get("RecordStartTick")
            .map(long)
            .transpose()?
            .unwrap_or(0);
        let playing = data
            .get("IsPlaying")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(0)
            != 0;
        let elapsed = tick
            .checked_sub(start)
            .ok_or("jukebox tick count overflows")?;
        if playing && elapsed >= 0 {
            super::insert(data, "ticks_since_song_started", V::Long(elapsed))?;
        } else if playing {
            return Err("playing jukebox has a song start later than its current tick".into());
        }
        for key in ["TickCount", "RecordStartTick", "IsPlaying"] {
            data.remove(key);
        }
    } else {
        let elapsed = data
            .remove("ticks_since_song_started")
            .map(|v| long(&v))
            .transpose()?;
        if elapsed.is_some_and(|v| v < 0) {
            return Err("jukebox song progress must be nonnegative".into());
        }
        super::insert(data, "TickCount", V::Long(elapsed.unwrap_or(0)))?;
        super::insert(data, "RecordStartTick", V::Long(0))?;
        super::insert(data, "IsPlaying", V::Byte(i8::from(elapsed.is_some())))?;
    }
    Ok(())
}

pub(super) fn projectile(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if !context.crosses(3938) {
        return Ok(());
    }
    if matches!(
        id,
        "minecraft:dragon_fireball"
            | "minecraft:fireball"
            | "minecraft:small_fireball"
            | "minecraft:wither_skull"
            | "minecraft:wind_charge"
            | "minecraft:breeze_wind_charge"
    ) {
        return acceleration(data, context);
    }
    if !matches!(id, "minecraft:arrow" | "minecraft:spectral_arrow") {
        return Ok(());
    }
    if context.forward() {
        if let Some(value) = data.remove("ShotFromCrossbow") {
            let crossbow = crate::nbt::number(&value)? != 0;
            if crossbow {
                super::insert(
                    data,
                    "weapon",
                    V::Compound(Compound::from([
                        ("id".into(), V::String("minecraft:crossbow".into())),
                        ("count".into(), V::Int(1)),
                    ])),
                )?;
            }
        }
    } else if let Some(value) = data.remove("weapon") {
        let weapon = crate::nbt::compound(&value)?;
        if weapon.is_empty() {
            return Ok(());
        }
        let weapon_id = crate::catalog::namespace(&text(weapon, "id")?);
        if !matches!(weapon_id.as_str(), "minecraft:bow" | "minecraft:crossbow") {
            return Err(format!(
                "projectile weapon {weapon_id} cannot be represented before Java 1.21"
            ));
        }
        for (key, value) in weapon {
            let supported = match key.as_str() {
                "id" => true,
                "count" => crate::nbt::number(value)? == 1,
                "components" => crate::nbt::compound(value)?.is_empty(),
                _ => false,
            };
            if !supported {
                return Err(format!(
                    "weapon.{key}: customized projectile weapon cannot be represented before Java 1.21"
                ));
            }
        }
        super::insert(
            data,
            "ShotFromCrossbow",
            V::Byte(i8::from(weapon_id == "minecraft:crossbow")),
        )?;
    }
    Ok(())
}

fn acceleration(data: &mut Compound, context: &Context) -> Result<()> {
    let motion = data
        .get("Motion")
        .map(crate::nbt::doubles)
        .transpose()?
        .unwrap_or([0.0; 3]);
    let speed = motion[0].hypot(motion[1]).hypot(motion[2]);
    let direction = if speed > 0.0 {
        motion.map(|v| v / speed)
    } else {
        [0.0; 3]
    };
    if context.forward() {
        let power = data
            .get("power")
            .map(crate::nbt::doubles)
            .transpose()?
            .unwrap_or([0.0; 3]);
        let magnitude = power[0].hypot(power[1]).hypot(power[2]);
        let signed = if power.iter().zip(direction).map(|(a, b)| a * b).sum::<f64>() < 0.0 {
            -magnitude
        } else {
            magnitude
        };
        if !signed.is_finite() {
            return Err("power: acceleration magnitude is not finite".into());
        }
        if power
            .iter()
            .zip(direction)
            .any(|(a, b)| (a - b * signed).abs() > magnitude * 1e-12)
        {
            return Err("power: acceleration direction differs from Motion and cannot be represented by a scalar".into());
        }
        super::insert(data, "acceleration_power", V::Double(signed))?;
        data.remove("power");
    } else {
        let acceleration = match data.get("acceleration_power") {
            Some(V::Double(n)) => *n,
            Some(V::Float(n)) => f64::from(*n),
            None => 0.1,
            _ => return Err("acceleration_power: expected a floating point value".into()),
        };
        if !acceleration.is_finite() {
            return Err("acceleration_power: value must be finite".into());
        }
        if speed == 0.0 && acceleration != 0.0 {
            return Err("acceleration_power: nonzero acceleration needs Motion to reconstruct its direction".into());
        }
        super::insert(
            data,
            "power",
            crate::nbt::double_list(direction.map(|v| v * acceleration)),
        )?;
        data.remove("acceleration_power");
    }
    Ok(())
}

const WOODS: [&str; 9] = [
    "oak", "spruce", "birch", "jungle", "acacia", "dark_oak", "mangrove", "cherry", "bamboo",
];

pub(super) fn boat(data: &mut Compound, context: &Context) -> Result<()> {
    let id = crate::catalog::namespace(&text(data, "id")?);
    if !context.crosses(4067) {
        if context.target.data_version < 4067
            && matches!(id.as_str(), "minecraft:boat" | "minecraft:chest_boat")
        {
            let wood = data
                .get("Type")
                .map(crate::nbt::string)
                .transpose()?
                .unwrap_or("oak");
            let suffix = match (wood == "bamboo", id == "minecraft:chest_boat") {
                (true, true) => "chest_raft",
                (true, false) => "raft",
                (false, true) => "chest_boat",
                (false, false) => "boat",
            };
            context.target.item(&format!("minecraft:{wood}_{suffix}"))?;
        }
        return Ok(());
    }
    if context.forward() && matches!(id.as_str(), "minecraft:boat" | "minecraft:chest_boat") {
        let wood = data
            .get("Type")
            .map(crate::nbt::string)
            .transpose()?
            .unwrap_or("oak");
        if !WOODS.contains(&wood) {
            return Err(format!("boat.Type: unknown wood {wood}"));
        }
        let suffix = match (wood == "bamboo", id == "minecraft:chest_boat") {
            (true, true) => "chest_raft",
            (true, false) => "raft",
            (false, true) => "chest_boat",
            (false, false) => "boat",
        };
        data.insert("id".into(), V::String(format!("minecraft:{wood}_{suffix}")));
        data.remove("Type");
    } else if !context.forward() {
        let Some(name) = id.strip_prefix("minecraft:") else {
            return Ok(());
        };
        let found = ["chest_boat", "chest_raft", "boat", "raft"]
            .iter()
            .find_map(|suffix| {
                name.strip_suffix(&format!("_{suffix}"))
                    .map(|wood| (wood, suffix.starts_with("chest_")))
            });
        if let Some((wood, chest)) = found {
            if !WOODS.contains(&wood) {
                return Err(format!(
                    "boat wood {wood} has no representation before Java 1.21.2"
                ));
            }
            context.target.item(&id)?;
            super::insert(data, "Type", V::String(wood.into()))?;
            data.insert(
                "id".into(),
                V::String(
                    if chest {
                        "minecraft:chest_boat"
                    } else {
                        "minecraft:boat"
                    }
                    .into(),
                ),
            );
        }
    }
    Ok(())
}

pub(super) fn boat_item(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if !context.crosses(4067) || !context.forward() {
        return Ok(());
    }
    let Some(name) = id.strip_prefix("minecraft:") else {
        return Ok(());
    };
    let wood = ["chest_boat", "chest_raft", "boat", "raft"]
        .iter()
        .find_map(|suffix| name.strip_suffix(&format!("_{suffix}")));
    if let Some(wood) = wood.filter(|wood| WOODS.contains(wood))
        && let Some(value) = data.get_mut("minecraft:entity_data")
    {
        let entity = map_mut(value)?;
        if let Some(old) = entity.get("Type")
            && crate::nbt::string(old)? != wood
        {
            context.loss(
                "entity_data.Type",
                "boat item wood determines the spawned boat variant",
            );
        }
        entity.insert("Type".into(), V::String(wood.into()));
    }
    Ok(())
}

pub(super) fn salmon(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(4081) {
        return Ok(());
    }
    if context.forward() {
        if let Some(value) = data.get("type") {
            let old = crate::nbt::string(value)?;
            if old == "large" {
                return Ok(());
            }
            if old != "medium" {
                context.loss(
                    "type",
                    "the older salmon size encoding is normalized to medium",
                );
            }
        }
        data.insert("type".into(), V::String("medium".into()));
    } else if context.target.data_version < 4080 {
        if let Some(value) = data.remove("type")
            && crate::nbt::string(&value)? != "medium"
        {
            return Err(
                "salmon.type: only medium salmon can be represented before Java 1.21.2".into(),
            );
        }
    } else if data.get("type") != Some(&V::String("large".into())) {
        return Err("salmon.type: the Java 1.21.2 inverse size encoding is ambiguous".into());
    }
    Ok(())
}

pub(super) fn lock_value(value: &mut V, context: &Context, level: usize) -> Result<bool> {
    depth(level)?;
    if !context.crosses(4068) {
        if context.source >= 4068 {
            lock_predicate(value, context, level + 1)?;
        }
        return Ok(true);
    }
    if context.forward() {
        let name = crate::nbt::string(value)?;
        if name.is_empty() {
            return Ok(false);
        }
        let text = serde_json::to_string(name).map_err(|e| e.to_string())?;
        *value = V::Compound(Compound::from([(
            "components".into(),
            V::Compound(Compound::from([(
                "minecraft:custom_name".into(),
                V::String(text),
            )])),
        )]));
    } else {
        lock_predicate(value, context, level + 1)?;
        let predicate = crate::nbt::compound(value)?;
        let components = crate::nbt::compound(crate::nbt::get(predicate, "components")?)?;
        if predicate.len() != 1 || components.len() != 1 {
            return Err(
                "lock: only a single custom-name condition can be represented by a legacy lock"
                    .into(),
            );
        }
        let encoded = text(components, "minecraft:custom_name")?;
        let text: serde_json::Value = serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
        let name = match text {
            serde_json::Value::String(name) => name,
            serde_json::Value::Object(data) if data.len() == 1 => data
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("lock: expected literal custom name")?
                .into(),
            _ => {
                return Err(
                    "lock: rich text conditions cannot be represented by a legacy name string"
                        .into(),
                );
            }
        };
        if name.is_empty() {
            return Err("lock: an empty-name predicate cannot be represented by the unlocked legacy empty string".into());
        }
        *value = V::String(name);
    }
    Ok(true)
}

fn lock_predicate(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let predicate = map_mut(value)?;
    if let Some(value) = predicate.get_mut("items") {
        fn item(value: &mut V, context: &Context) -> Result<()> {
            let id = crate::nbt::string(value)?;
            if !id.starts_with('#') {
                context
                    .source_registry
                    .item(&crate::catalog::namespace(id))?;
                let id = context.rename("item", id)?;
                context.target.item(&id)?;
                *value = V::String(id);
            }
            Ok(())
        }
        if let V::List(values) = value {
            for value in values {
                item(value, context)?;
            }
        } else {
            item(value, context)?;
        }
    }
    let Some(components) = predicate.get_mut("components") else {
        return Ok(());
    };
    let components = map_mut(components)?;
    super::components::normalize(components)?;
    super::components::registered(components, context.source)?;
    super::components::model_fields(components, context)?;
    super::component_changes::inline_values(components, context, level + 1)?;
    if context.crosses(4996) && components.contains_key("minecraft:pot_decorations") {
        return Err("lock.components.minecraft:pot_decorations: exact equality across the expanded component schema requires predicate conversion".into());
    }
    if context.crosses(4786) {
        for (component, field) in [
            ("minecraft:provides_banner_patterns", None),
            ("minecraft:damage_resistant", Some("types")),
            ("minecraft:blocks_attacks", Some("bypassed_by")),
        ] {
            let Some(mut value) = components.get(component) else {
                continue;
            };
            if let Some(field) = field {
                let Some(nested) = crate::nbt::compound(value)?.get(field) else {
                    continue;
                };
                value = nested;
            }
            if !matches!(value, V::String(id) if id.starts_with('#')) {
                return Err(format!(
                    "lock.components.{component}: exact matching of explicit registry entries cannot be preserved by an older tag reference"
                ));
            }
        }
    }
    if context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
        for key in [
            "attribute_modifiers",
            "dyed_color",
            "can_break",
            "can_place_on",
            "enchantments",
            "stored_enchantments",
            "jukebox_playable",
            "trim",
            "unbreakable",
            "hide_tooltip",
            "hide_additional_tooltip",
            "tooltip_display",
        ] {
            if components.contains_key(&format!("minecraft:{key}")) {
                return Err(format!(
                    "lock.components.minecraft:{key}: exact matching across the tooltip schema change requires predicate conversion"
                ));
            }
        }
    }
    for key in ["minecraft:custom_name", "minecraft:item_name"] {
        if let Some(value) = components.get_mut(key) {
            super::text::convert(value, context, level + 1)?;
        }
    }
    if let Some(value) = components.get_mut("minecraft:lore") {
        super::text::lines(value, context, level + 1)?;
    }
    if let Some(value) = components.get_mut("minecraft:written_book_content") {
        super::text::book(value, context, level + 1)?;
    }
    if let Some(value) = components.get_mut("minecraft:profile") {
        super::profiles::convert(value, context)?;
    }
    if let Some(value) = components.get_mut("minecraft:attribute_modifiers") {
        super::attributes::item(value, context, "", level + 1)?;
    }
    super::components::registered(components, context.target.data_version)?;
    Ok(())
}

pub(super) fn lock_fields(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    let (old, new) = if context.crosses(4068) {
        if context.forward() {
            ("Lock", "lock")
        } else {
            ("lock", "Lock")
        }
    } else if context.source >= 4068 {
        ("lock", "lock")
    } else {
        return Ok(());
    };
    if let Some(mut value) = data.remove(old)
        && lock_value(&mut value, context, level + 1)?
    {
        super::insert(data, new, value)?;
    }
    Ok(())
}

pub(super) fn banner(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(4054) || !context.forward() {
        return Ok(());
    }
    let Some(V::String(name)) = data.get("minecraft:item_name") else {
        return Ok(());
    };
    let text: serde_json::Value = serde_json::from_str(name).map_err(|e| e.to_string())?;
    if text.get("translate").and_then(|v| v.as_str()) == Some("block.minecraft.ominous_banner")
        && !data.contains_key("minecraft:rarity")
        && !data.contains_key("!minecraft:rarity")
    {
        data.insert("minecraft:rarity".into(), V::String("uncommon".into()));
    }
    Ok(())
}
