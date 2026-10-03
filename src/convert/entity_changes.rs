use super::*;
use fastnbt::IntArray;

fn position(data: &mut Compound, old: [&str; 3], new: &str, forward: bool) -> Result<()> {
    if forward {
        if !data.contains_key(old[0]) {
            return Ok(());
        }
        let values = old
            .iter()
            .map(|key| {
                data.get(*key)
                    .map(crate::nbt::number)
                    .transpose()
                    .map(|value| value.unwrap_or(0))
            })
            .collect::<Result<Vec<_>>>()?;
        super::insert(data, new, V::IntArray(IntArray::new(values)))?;
        for key in old {
            data.remove(key);
        }
    } else if let Some(value) = data.remove(new) {
        let V::IntArray(values) = value else {
            return Err(format!("{new}: expected three-integer position"));
        };
        if values.len() != 3 {
            return Err(format!("{new}: expected three-integer position"));
        }
        for (key, value) in old.into_iter().zip(values.iter()) {
            super::insert(data, key, V::Int(*value))?;
        }
    }
    Ok(())
}

pub(super) fn identity(data: &mut Compound, context: &Context) -> Result<()> {
    if !context.crosses(4306) {
        return Ok(());
    }
    let id = crate::catalog::namespace(&text(data, "id")?);
    if context.forward() && id == "minecraft:potion" {
        let lingering = data
            .get("Item")
            .map(crate::nbt::compound)
            .transpose()?
            .and_then(|item| item.get("id"))
            .map(crate::nbt::string)
            .transpose()?
            .is_some_and(|id| crate::catalog::namespace(id) == "minecraft:lingering_potion");
        data.insert(
            "id".into(),
            V::String(
                if lingering {
                    "minecraft:lingering_potion"
                } else {
                    "minecraft:splash_potion"
                }
                .into(),
            ),
        );
    } else if !context.forward()
        && matches!(
            id.as_str(),
            "minecraft:splash_potion" | "minecraft:lingering_potion"
        )
    {
        let wanted = if id == "minecraft:lingering_potion" {
            "minecraft:lingering_potion"
        } else {
            "minecraft:splash_potion"
        };
        if let Some(value) = data.get("Item") {
            let item = crate::nbt::compound(value)?;
            let actual = crate::catalog::namespace(&text(item, "id")?);
            if (actual == "minecraft:lingering_potion") != (wanted == "minecraft:lingering_potion")
            {
                return Err(
                    "Item.id: projectile type disagrees with the legacy carried potion type".into(),
                );
            }
        } else if wanted == "minecraft:lingering_potion" {
            data.insert(
                "Item".into(),
                V::Compound(Compound::from([
                    ("id".into(), V::String(wanted.into())),
                    ("count".into(), V::Int(1)),
                ])),
            );
        }
        data.insert("id".into(), V::String("minecraft:potion".into()));
    }
    Ok(())
}

pub(super) fn convert(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    let forward = context.forward();
    variants(data, id, context)?;
    latest(data, id, context)?;
    if context.crosses(4303) {
        let (from, to) = if forward {
            ("FallDistance", "fall_distance")
        } else {
            ("fall_distance", "FallDistance")
        };
        if let Some(value) = data.remove(from) {
            let converted = if forward {
                let V::Float(value) = value else {
                    return Err("FallDistance: expected finite float".into());
                };
                if !value.is_finite() {
                    return Err("FallDistance: expected finite float".into());
                }
                V::Double(f64::from(value))
            } else {
                let V::Double(value) = value else {
                    return Err("fall_distance: expected finite double".into());
                };
                if !value.is_finite() || f64::from(value as f32) != value {
                    return Err(
                        "fall_distance: value cannot be represented exactly by a legacy float"
                            .into(),
                    );
                }
                V::Float(value as f32)
            };
            super::insert(data, to, converted)?;
        }
    }
    if context.crosses(5023)
        && let Some(value) = data.remove("invulnerable_time")
        && crate::nbt::number(&value)? > 0
    {
        return Err(
            "invulnerable_time: temporary invulnerability has no representation before Java26.3"
                .into(),
        );
    }
    if context.crosses(4548)
        && id == "minecraft:player"
        && let Some(value) = data.get_mut("respawn")
    {
        let respawn = map_mut(value)?;
        if forward {
            let yaw = respawn.get("angle").cloned().unwrap_or(V::Float(0.0));
            for (key, expected) in [("yaw", yaw), ("pitch", V::Float(0.0))] {
                if respawn.get(key).is_some_and(|value| *value != expected) {
                    return Err(format!(
                        "respawn.{key}: a previously ignored field would change the target respawn direction"
                    ));
                }
            }
            if respawn.contains_key("angle") {
                respawn.remove("yaw");
            }
            rename(respawn, "angle", "yaw", true)?;
            respawn.entry("yaw".into()).or_insert(V::Float(0.0));
            respawn.entry("pitch".into()).or_insert(V::Float(0.0));
            respawn
                .entry("dimension".into())
                .or_insert(V::String("minecraft:overworld".into()));
        } else {
            let pitch = respawn
                .remove("pitch")
                .ok_or("respawn.pitch: required source field is missing")?;
            if !matches!(pitch, V::Float(value) if value == 0.0) {
                return Err(
                    "respawn.pitch: older respawn data cannot retain a vertical look direction"
                        .into(),
                );
            }
            rename(respawn, "angle", "yaw", false)?;
        }
    }
    if context.crosses(4903) && super::entities::living(id, context) {
        if let Some(value) = data.get_mut("ticks_since_last_hurt_by_mob") {
            let ticks = crate::nbt::number(value)?;
            *value = V::Int(ticks.checked_neg().ok_or("ticks_since_last_hurt_by_mob: source-effective timestamp exceeds the target integer range")?);
        }
        if forward {
            data.remove("HurtByTimestamp");
        }
    }
    if context.crosses(4763) && super::entities::living(id, context) {
        if data.contains_key("current_explosion_impact_pos") {
            return Err("current_explosion_impact_pos: persistent explosion impulse context has no representation before Java26.1".into());
        }
        if let Some(value) = data.remove("current_impulse_context_reset_grace_time")
            && crate::nbt::number(&value)? != 0
        {
            return Err("current_impulse_context_reset_grace_time: active impulse protection requires Java26.1+".into());
        }
    }
    if context.crosses(4671)
        && matches!(
            id,
            "minecraft:bee"
                | "minecraft:iron_golem"
                | "minecraft:polar_bear"
                | "minecraft:wolf"
                | "minecraft:enderman"
                | "minecraft:zombified_piglin"
        )
    {
        rename(data, "AngryAt", "angry_at", forward)?;
        if forward {
            if let Some(value) = data.get("AngerTime") {
                crate::nbt::number(value)?;
            }
            if data.contains_key("anger_end_time") {
                return Err(
                    "anger_end_time: legacy opaque data would activate the target anger clock"
                        .into(),
                );
            }
        } else if let Some(value) = data.remove("anger_end_time") {
            if value != V::Long(-1) {
                return Err("anger_end_time: converting an absolute anger deadline requires source world game time".into());
            }
            if data.contains_key("AngerTime") {
                context.loss("AngerTime", "source loader ignores the countdown when an absolute anger deadline is present");
                data.remove("AngerTime");
            }
        }
    }
    if context.crosses(4325) {
        if super::entities::living(id, context) {
            for key in ["last_hurt_by_mob", "last_hurt_by_player"] {
                if data.contains_key(key) {
                    return Err(format!(
                        "{key}: persistent attacker references have no representation before Java1.21.5"
                    ));
                }
            }
            if let Some(value) = data.remove("ticks_since_last_hurt_by_player")
                && crate::nbt::number(&value)? != 0
            {
                return Err("ticks_since_last_hurt_by_player: persistent player damage memory requires Java1.21.5+".into());
            }
            if forward {
                if let Some(value) = data.remove("HurtByTimestamp") {
                    let ticks = crate::nbt::number(&value)?;
                    super::insert(data, "ticks_since_last_hurt_by_mob", V::Int(ticks))?;
                } else if let Some(value) = data.remove("ticks_since_last_hurt_by_mob")
                    && crate::nbt::number(&value)? != 0
                {
                    return Err("ticks_since_last_hurt_by_mob: legacy opaque field would activate persistent damage memory".into());
                }
            } else {
                if data.remove("HurtByTimestamp").is_some() {
                    context.loss(
                        "HurtByTimestamp",
                        "source loader ignores the obsolete timestamp field",
                    );
                }
                if let Some(value) = data.remove("ticks_since_last_hurt_by_mob") {
                    let ticks = crate::nbt::number(&value)?;
                    super::insert(data, "HurtByTimestamp", V::Int(ticks))?;
                }
            }
        }
        vectors(data, forward)?;
        if !forward && let Some(value) = data.get("Air") {
            let air = crate::nbt::number(value)?;
            if i16::try_from(air).is_err() {
                return Err("Air: value exceeds the legacy short oxygen range".into());
            }
        }
        if id.ends_with("minecart") {
            minecart(data, id, context)?;
        }
        if id != "minecraft:marker"
            && let Some(value) = data.get("data")
        {
            if !crate::nbt::compound(value)?.is_empty() {
                return Err(
                    "data: native custom entity data has no faithful legacy representation".into(),
                );
            }
            data.remove("data");
        }
        match id {
            "minecraft:allay" => allay(data, forward)?,
            "minecraft:area_effect_cloud" => cloud(data, context)?,
            "minecraft:item" => default(data, "Health", V::Short(0), V::Short(5), forward),
            "minecraft:tnt" => default(data, "fuse", V::Short(0), V::Short(80), forward),
            "minecraft:dolphin" => {
                default(data, "Moistness", V::Int(0), V::Int(2400), forward);
                if forward {
                    for key in ["TreasurePosX", "TreasurePosY", "TreasurePosZ"] {
                        if data.remove(key).is_some() {
                            context.loss(
                                key,
                                "the target no longer stores the dolphin treasure destination",
                            );
                        }
                    }
                }
            }
            "minecraft:falling_block" => falling(data, forward)?,
            _ => {}
        }
    }
    if !context.crosses(4314) {
        return Ok(());
    }
    if super::entities::living(id, context) {
        position(
            data,
            ["SleepingX", "SleepingY", "SleepingZ"],
            "sleeping_pos",
            forward,
        )?;
    }
    match id {
        "minecraft:vex" => {
            rename(data, "LifeTicks", "life_ticks", forward)?;
            position(data, ["BoundX", "BoundY", "BoundZ"], "bound_pos", forward)?;
        }
        "minecraft:phantom" => {
            rename(data, "Size", "size", forward)?;
            position(data, ["AX", "AY", "AZ"], "anchor_pos", forward)?;
        }
        "minecraft:turtle" => {
            if forward && !data.contains_key("HomePosX") {
                data.insert("HomePosX".into(), V::Int(0));
            }
            if !forward && !data.contains_key("home_pos") {
                return Err(
                    "home_pos: default turtle home requires source placement context".into(),
                );
            }
            position(
                data,
                ["HomePosX", "HomePosY", "HomePosZ"],
                "home_pos",
                forward,
            )?;
            rename(data, "HasEgg", "has_egg", forward)?;
            if forward {
                for key in ["TravelPosX", "TravelPosY", "TravelPosZ"] {
                    if data.remove(key).is_some() {
                        context.loss(
                            key,
                            "the target no longer stores the turtle travel destination",
                        );
                    }
                }
            }
        }
        "minecraft:item_frame"
        | "minecraft:glow_item_frame"
        | "minecraft:painting"
        | "minecraft:leash_knot" => {
            position(data, ["TileX", "TileY", "TileZ"], "block_pos", forward)?;
        }
        "minecraft:player" => player(data, forward)?,
        _ => {}
    }
    Ok(())
}

fn variants(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    for (owner, boundary, key, default) in [
        ("minecraft:wolf", 3837, "variant", "pale"),
        ("minecraft:pig", 4325, "variant", "temperate"),
        ("minecraft:cow", 4325, "variant", "temperate"),
        ("minecraft:chicken", 4325, "variant", "temperate"),
        ("minecraft:wolf", 4325, "sound_variant", "classic"),
        ("minecraft:cat", 4763, "sound_variant", "classic"),
        ("minecraft:pig", 4763, "sound_variant", "classic"),
        ("minecraft:cow", 4763, "sound_variant", "classic"),
        ("minecraft:chicken", 4763, "sound_variant", "classic"),
        ("minecraft:mooshroom", 1901, "Type", "red"),
    ] {
        if id != owner || !context.crosses(boundary) {
            continue;
        }
        if let Some(value) = data.get(key)
            && crate::catalog::namespace(crate::nbt::string(value)?)
                != format!("minecraft:{default}")
        {
            return Err(format!(
                "{key}: nondefault {owner} variant has no representation before data version {boundary}"
            ));
        }
        if !context.forward() {
            data.remove(key);
        }
    }
    if context.crosses(4763)
        && matches!(
            id,
            "minecraft:armadillo"
                | "minecraft:axolotl"
                | "minecraft:bee"
                | "minecraft:camel"
                | "minecraft:camel_husk"
                | "minecraft:cat"
                | "minecraft:chicken"
                | "minecraft:cow"
                | "minecraft:dolphin"
                | "minecraft:donkey"
                | "minecraft:fox"
                | "minecraft:frog"
                | "minecraft:glow_squid"
                | "minecraft:goat"
                | "minecraft:happy_ghast"
                | "minecraft:hoglin"
                | "minecraft:horse"
                | "minecraft:llama"
                | "minecraft:mooshroom"
                | "minecraft:mule"
                | "minecraft:nautilus"
                | "minecraft:ocelot"
                | "minecraft:panda"
                | "minecraft:parrot"
                | "minecraft:pig"
                | "minecraft:polar_bear"
                | "minecraft:rabbit"
                | "minecraft:sheep"
                | "minecraft:skeleton_horse"
                | "minecraft:sniffer"
                | "minecraft:squid"
                | "minecraft:strider"
                | "minecraft:tadpole"
                | "minecraft:trader_llama"
                | "minecraft:turtle"
                | "minecraft:villager"
                | "minecraft:wandering_trader"
                | "minecraft:wolf"
                | "minecraft:zombie_horse"
                | "minecraft:zombie_nautilus"
        )
        && let Some(value) = data.get("AgeLocked")
    {
        if crate::nbt::number(value)? != 0 {
            return Err("AgeLocked: locked growth has no older representation".into());
        }
        if !context.forward() {
            data.remove("AgeLocked");
        }
    }
    Ok(())
}

fn player(data: &mut Compound, forward: bool) -> Result<()> {
    if forward {
        let mut respawn = Compound::new();
        for key in [
            "SpawnX",
            "SpawnY",
            "SpawnZ",
            "SpawnAngle",
            "SpawnDimension",
            "SpawnForced",
        ] {
            if let Some(value) = data.get(key) {
                respawn.insert(key.into(), value.clone());
            }
        }
        if respawn
            .keys()
            .any(|key| matches!(key.as_str(), "SpawnX" | "SpawnY" | "SpawnZ"))
        {
            position(&mut respawn, ["SpawnX", "SpawnY", "SpawnZ"], "pos", true)?;
            for (old, new) in [
                ("SpawnAngle", "angle"),
                ("SpawnDimension", "dimension"),
                ("SpawnForced", "forced"),
            ] {
                rename(&mut respawn, old, new, true)?;
            }
            super::insert(data, "respawn", V::Compound(respawn))?;
            for key in [
                "SpawnX",
                "SpawnY",
                "SpawnZ",
                "SpawnAngle",
                "SpawnDimension",
                "SpawnForced",
            ] {
                data.remove(key);
            }
        }
        if let Some(value) = data.remove("enteredNetherPosition") {
            let position = crate::nbt::compound(&value)?;
            if position
                .keys()
                .any(|key| !matches!(key.as_str(), "x" | "y" | "z"))
            {
                return Err("enteredNetherPosition: unknown position fields".into());
            }
            let values = ["x", "y", "z"]
                .into_iter()
                .map(|key| match crate::nbt::get(position, key)? {
                    V::Double(n) if n.is_finite() => Ok(V::Double(*n)),
                    _ => Err(format!(
                        "enteredNetherPosition.{key}: expected finite double"
                    )),
                })
                .collect::<Result<Vec<_>>>()?;
            super::insert(data, "entered_nether_pos", V::List(values))?;
        }
    } else {
        if let Some(value) = data.remove("respawn") {
            let mut respawn = crate::nbt::compound(&value)?.clone();
            if respawn
                .keys()
                .any(|key| !matches!(key.as_str(), "pos" | "angle" | "dimension" | "forced"))
            {
                return Err(
                    "respawn: unknown fields cannot be represented by legacy spawn fields".into(),
                );
            }
            position(&mut respawn, ["SpawnX", "SpawnY", "SpawnZ"], "pos", false)?;
            for (old, new) in [
                ("SpawnAngle", "angle"),
                ("SpawnDimension", "dimension"),
                ("SpawnForced", "forced"),
            ] {
                rename(&mut respawn, old, new, false)?;
            }
            for (key, value) in respawn {
                super::insert(data, &key, value)?;
            }
        }
        if let Some(value) = data.remove("entered_nether_pos") {
            let values = crate::nbt::list(&value)?;
            if values.len() != 3
                || values
                    .iter()
                    .any(|value| !matches!(value, V::Double(n) if n.is_finite()))
            {
                return Err("entered_nether_pos: expected three finite doubles".into());
            }
            super::insert(
                data,
                "enteredNetherPosition",
                V::Compound(
                    ["x", "y", "z"]
                        .into_iter()
                        .zip(values.iter().cloned())
                        .map(|(key, value)| (key.into(), value))
                        .collect(),
                ),
            )?;
        }
    }
    Ok(())
}

fn allay(data: &mut Compound, forward: bool) -> Result<()> {
    let cooldown = data
        .get("DuplicationCooldown")
        .map(crate::nbt::number)
        .transpose()?
        .unwrap_or(0);
    if forward {
        let enabled = data
            .get("CanDuplicate")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(0)
            != 0;
        if enabled && cooldown != 0 {
            return Err("CanDuplicate: enabled duplication with a nonzero cooldown has no target representation".into());
        }
        data.remove("CanDuplicate");
        if !enabled && cooldown == 0 {
            data.insert("DuplicationCooldown".into(), V::Long(1));
        }
    } else {
        super::insert(data, "CanDuplicate", V::Byte(i8::from(cooldown == 0)))?;
    }
    Ok(())
}

fn cloud(data: &mut Compound, context: &Context) -> Result<()> {
    let forward = context.forward();
    if !data.contains_key("potion_contents") {
        let particle = data.get("Particle").map(crate::nbt::compound).transpose()?;
        let color_particle = particle
            .and_then(|particle| particle.get("type"))
            .map(crate::nbt::string)
            .transpose()?
            .is_none_or(|id| crate::catalog::namespace(id) == "minecraft:entity_effect");
        if color_particle {
            if forward {
                let color = particle
                    .and_then(|particle| particle.get("color"))
                    .map(crate::nbt::number)
                    .transpose()?
                    .unwrap_or(-1);
                if (color as u32) >> 24 != 255 {
                    return Err(
                        "Particle.color: the target cloud loader forces opaque potion colors"
                            .into(),
                    );
                }
                data.insert(
                    "potion_contents".into(),
                    V::Compound(Compound::from([(
                        "custom_color".into(),
                        V::Int(color & 0xffffff),
                    )])),
                );
            } else {
                data.insert(
                    "Particle".into(),
                    V::Compound(Compound::from([
                        ("type".into(), V::String("minecraft:entity_effect".into())),
                        ("color".into(), V::Int(0xff000000_u32 as i32)),
                    ])),
                );
            }
        }
    }
    let potion = data
        .get("potion_contents")
        .map(crate::nbt::compound)
        .transpose()?;
    let base_effects = potion
        .and_then(|potion| potion.get("potion"))
        .map(crate::nbt::string)
        .transpose()?
        .is_some_and(|id| {
            !matches!(
                crate::catalog::namespace(id).as_str(),
                "minecraft:empty"
                    | "minecraft:water"
                    | "minecraft:mundane"
                    | "minecraft:thick"
                    | "minecraft:awkward"
            )
        });
    let custom_effects = potion
        .and_then(|potion| potion.get("custom_effects"))
        .map(crate::nbt::list)
        .transpose()?
        .is_some_and(|values| !values.is_empty());
    if forward {
        for key in ["Duration", "WaitTime", "ReapplicationDelay"] {
            if !data.contains_key(key) {
                data.insert(key.into(), V::Int(0));
            }
        }
        if !data.contains_key("Radius") {
            data.insert("Radius".into(), V::Float(0.0));
        }
        if base_effects {
            if custom_effects {
                cloud_durations(data, 0.25, true)?;
            }
            super::insert(data, "potion_duration_scale", V::Float(0.25))?;
        }
    } else {
        let scale = match data.get("potion_duration_scale") {
            None => 1.0,
            Some(V::Float(value)) if value.is_finite() && *value >= 0.0 => *value,
            _ => return Err("potion_duration_scale: expected nonnegative finite float".into()),
        };
        if base_effects && scale != 0.25 {
            let potion = map_mut(
                data.get_mut("potion_contents")
                    .ok_or("missing potion contents")?,
            )?;
            let color = super::potions::color(potion)?;
            let reference = text(potion, "potion")?;
            let mut expanded = super::potions::effects(&reference)?;
            if let Some(value) = potion.remove("custom_effects") {
                expanded.extend(crate::nbt::list(&value)?.iter().cloned());
            }
            potion.remove("potion");
            context.loss(
                "potion_contents.potion",
                "expanded the potion registry identity into equivalent custom effects",
            );
            potion
                .entry("custom_color".into())
                .or_insert(V::Int(color & 0xffffff));
            potion.insert("custom_effects".into(), V::List(expanded));
            cloud_durations(data, scale, false)?;
        } else if custom_effects && scale != 1.0 {
            cloud_durations(data, scale, false)?;
        }
        data.remove("potion_duration_scale");
        for (key, value) in [
            ("Duration", V::Int(-1)),
            ("WaitTime", V::Int(20)),
            ("ReapplicationDelay", V::Int(20)),
            ("Radius", V::Float(3.0)),
        ] {
            if !data.contains_key(key) {
                data.insert(key.into(), value);
            }
        }
        if data.get("Duration").map(crate::nbt::number).transpose()? == Some(-1) {
            return Err("Duration: an infinite cloud lifetime cannot be represented by the legacy cloud timer".into());
        }
    }
    Ok(())
}

fn default(data: &mut Compound, key: &str, old: V, new: V, forward: bool) {
    if !data.contains_key(key) {
        data.insert(key.into(), if forward { old } else { new });
    }
}

fn falling(data: &mut Compound, forward: bool) -> Result<()> {
    let state = data
        .get("BlockState")
        .map(crate::nbt::compound)
        .transpose()?;
    let air = state
        .and_then(|state| state.get("Name"))
        .map(crate::nbt::string)
        .transpose()?
        .is_some_and(|id| crate::catalog::namespace(id) == "minecraft:air");
    if forward && (state.is_none() || state.is_some_and(Compound::is_empty) || air) {
        data.insert(
            "BlockState".into(),
            V::Compound(Compound::from([(
                "Name".into(),
                V::String("minecraft:sand".into()),
            )])),
        );
    } else if !forward && air {
        return Err("BlockState: the legacy falling-block loader replaces air with sand".into());
    }
    if data.contains_key("HurtEntities") {
        default(data, "FallHurtMax", V::Int(0), V::Int(40), forward);
    }
    Ok(())
}

pub(super) fn block_entity(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if context.crosses(5023)
        && matches!(id, "furnace" | "blast_furnace" | "smoker" | "brewing_stand")
    {
        let forward = context.forward();
        let fields: &[&str] = if id == "brewing_stand" {
            &["BrewTime", "Fuel"]
        } else {
            &[
                "cooking_time_spent",
                "cooking_total_time",
                "lit_time_remaining",
                "lit_total_time",
            ]
        };
        for key in fields {
            if let Some(value) = data.get_mut(*key) {
                let number = crate::nbt::number(value)?;
                *value =
                    if forward {
                        if *key == "Fuel" {
                            i8::try_from(number)
                                .map_err(|_| "Fuel: source value exceeds the byte codec range")?;
                        } else {
                            i16::try_from(number).map_err(|_| {
                                format!("{key}: source value exceeds the short codec range")
                            })?;
                        }
                        V::Int(number)
                    } else if *key == "Fuel" {
                        V::Byte(
                            i8::try_from(number)
                                .map_err(|_| "Fuel: amount exceeds the legacy byte capacity")?,
                        )
                    } else {
                        V::Short(i16::try_from(number).map_err(|_| {
                            format!("{key}: value exceeds the legacy short capacity")
                        })?)
                    };
            }
        }
        if let Some(value) = data.get("speed_multiplier") {
            let V::Float(speed) = value else {
                return Err("speed_multiplier: expected finite float".into());
            };
            if !speed.is_finite() || (*speed > 0.0 && *speed != 1.0) {
                return Err(
                    "speed_multiplier: changing cooking or brewing speed requires Java26.3+".into(),
                );
            }
            if !forward {
                data.remove("speed_multiplier");
            }
        }
        if id == "brewing_stand" {
            for (key, default) in [("total_brew_time", 400), ("total_fuel", 20)] {
                if let Some(value) = data.get(key) {
                    let number = crate::nbt::number(value)?;
                    if number != default {
                        if forward {
                            return Err(format!(
                                "{key}: legacy opaque field would activate a changed brewing indicator"
                            ));
                        }
                        context.loss(key, "the older brewing interface has a fixed total indicator; current remaining time and fuel are preserved");
                    }
                    if !forward {
                        data.remove(key);
                    }
                }
            }
        }
    }
    if !context.crosses(4325) {
        return Ok(());
    }
    let forward = context.forward();
    match id {
        "hopper" => default(data, "TransferCooldown", V::Int(0), V::Int(-1), forward),
        "chiseled_bookshelf" => {
            default(data, "last_interacted_slot", V::Int(0), V::Int(-1), forward)
        }
        "structure_block" => {
            for key in ["ignoreEntities", "showboundingbox"] {
                default(data, key, V::Byte(0), V::Byte(1), forward);
            }
            default(data, "posY", V::Int(0), V::Int(1), forward);
            if !forward
                && let Some(value) = data.remove("strict")
                && crate::nbt::number(&value)? != 0
            {
                return Err(
                    "strict: strict structure placement is unavailable in the target".into(),
                );
            }
        }
        _ => {}
    }
    Ok(())
}

fn minecart(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if context.forward() {
        let custom = data
            .remove("CustomDisplayTile")
            .map(|value| crate::nbt::number(&value))
            .transpose()?
            .unwrap_or(0)
            != 0;
        if custom {
            if !data.contains_key("DisplayState") {
                data.insert(
                    "DisplayState".into(),
                    V::Compound(Compound::from([(
                        "Name".into(),
                        V::String("minecraft:air".into()),
                    )])),
                );
            }
            default(data, "DisplayOffset", V::Int(0), V::Int(0), true);
        } else {
            for key in ["DisplayState", "DisplayOffset"] {
                if data.remove(key).is_some() {
                    context.loss(
                        key,
                        "the legacy disabled display override was ignored by the source loader",
                    );
                }
            }
        }
    } else if data.contains_key("DisplayState") {
        super::insert(data, "CustomDisplayTile", V::Byte(1))?;
        let offset = match id {
            "minecraft:chest_minecart" => 8,
            "minecraft:hopper_minecart" => 1,
            _ => 6,
        };
        default(data, "DisplayOffset", V::Int(offset), V::Int(offset), false);
    } else if data.contains_key("DisplayOffset") {
        return Err("DisplayOffset: a standalone offset requires materializing the source minecart display block".into());
    }
    Ok(())
}

fn cloud_durations(data: &mut Compound, scale: f32, forward: bool) -> Result<()> {
    let potion = map_mut(
        data.get_mut("potion_contents")
            .ok_or("missing potion contents")?,
    )?;
    let Some(value) = potion.get_mut("custom_effects") else {
        return Ok(());
    };
    for value in list_mut(value)? {
        let effect = map_mut(value)?;
        let duration = effect
            .get("duration")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(0);
        if duration <= 0 {
            continue;
        }
        let converted = if forward {
            let value = duration
                .checked_mul(4)
                .ok_or("custom_effects.duration: scaled duration overflow")?;
            if ((value as f32 * scale).floor() as i32).max(1) != duration {
                return Err("custom_effects.duration: target float scaling cannot preserve this duration exactly".into());
            }
            value
        } else {
            let scaled = (duration as f32 * scale).floor();
            if !scaled.is_finite() || scaled > i32::MAX as f32 {
                return Err("custom_effects.duration: scaled duration overflow".into());
            }
            (scaled as i32).max(1)
        };
        effect.insert("duration".into(), V::Int(converted));
    }
    Ok(())
}

fn vectors(data: &mut Compound, forward: bool) -> Result<()> {
    for (key, length, float) in [
        ("Pos", 3, false),
        ("Motion", 3, false),
        ("Rotation", 2, true),
    ] {
        let Some(value) = data.get_mut(key) else {
            continue;
        };
        let values = list_mut(value)?;
        if values.iter().any(|value| {
            if float {
                !matches!(value, V::Float(n) if n.is_finite())
            } else {
                !matches!(value, V::Double(n) if n.is_finite())
            }
        }) {
            return Err(format!(
                "{key}: expected finite {} coordinates",
                if float { "float" } else { "double" }
            ));
        }
        if forward {
            values.truncate(length);
            values.resize(length, if float { V::Float(0.0) } else { V::Double(0.0) });
        } else if values.len() != length {
            return Err(format!(
                "{key}: malformed vector is ignored by the source but would become active in the target"
            ));
        }
    }
    Ok(())
}

pub(super) fn cloud_particle(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    if context.crosses(4420) {
        if context.forward() {
            let value = data.remove("Particle");
            let potion = data
                .get("potion_contents")
                .map(crate::nbt::compound)
                .transpose()?;
            let empty = potion.map(empty_potion).transpose()?.unwrap_or(true);
            if let Some(value) = value {
                let mut particle = crate::nbt::compound(&value)?.clone();
                let kind = crate::catalog::namespace(&text(&particle, "type")?);
                if matches!(
                    kind.as_str(),
                    "minecraft:entity_effect" | "minecraft:tinted_leaves"
                ) {
                    let color = if empty {
                        0xff000000_u32 as i32
                    } else {
                        super::potions::color(potion.unwrap())?
                    };
                    particle.insert("color".into(), V::Int(color));
                }
                super::insert(data, "custom_particle", V::Compound(particle))?;
            } else if empty {
                super::insert(
                    data,
                    "custom_particle",
                    V::Compound(Compound::from([
                        ("type".into(), V::String("minecraft:entity_effect".into())),
                        ("color".into(), V::Int(0xff000000_u32 as i32)),
                    ])),
                )?;
            }
        } else {
            let value = data.remove("custom_particle");
            let mut particle = value
                .as_ref()
                .map(crate::nbt::compound)
                .transpose()?
                .cloned();
            let kind = particle
                .as_ref()
                .map(|particle| text(particle, "type"))
                .transpose()?
                .map(|id| crate::catalog::namespace(&id));
            let color_particle = kind.as_deref().is_none_or(|id| {
                matches!(id, "minecraft:entity_effect" | "minecraft:tinted_leaves")
            });
            if color_particle {
                let color = if let Some(particle) = &particle {
                    crate::nbt::number(crate::nbt::get(particle, "color")?)?
                } else if let Some(potion) = data.get("potion_contents") {
                    super::potions::color(crate::nbt::compound(potion)?)?
                } else {
                    0xff385dc6_u32 as i32
                };
                if color as u32 >> 24 != 255 {
                    return Err("custom_particle.color: the legacy cloud loader cannot retain transparent overrides".into());
                }
                let potion = data
                    .entry("potion_contents".into())
                    .or_insert_with(|| V::Compound(Compound::new()));
                let potion = map_mut(potion)?;
                if let Some(value) = potion.get("custom_color") {
                    if (crate::nbt::number(value)? & 0xffffff) != color & 0xffffff {
                        return Err("custom_particle.color: retaining the override would replace an explicit potion custom color".into());
                    }
                } else {
                    potion.insert("custom_color".into(), V::Int(color & 0xffffff));
                }
                if particle.is_none() {
                    particle = Some(Compound::from([
                        ("type".into(), V::String("minecraft:entity_effect".into())),
                        ("color".into(), V::Int(color)),
                    ]));
                }
            }
            if let Some(particle) = particle {
                super::insert(data, "Particle", V::Compound(particle))?;
            }
        }
    }
    let key = if context.target.data_version >= 4420 {
        "custom_particle"
    } else {
        "Particle"
    };
    if let Some(value) = data.get_mut(key) {
        super::particles::convert(value, context, level + 1)?;
    }
    Ok(())
}

fn latest(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if !context.crosses(4420) {
        return Ok(());
    }
    if super::entities::living(id, context) {
        if let Some(value) = data.get("home_radius")
            && crate::nbt::number(value)? >= 0
        {
            return Err(
                "home_radius: persistent mob home restriction has no legacy representation".into(),
            );
        }
        if let Some(value) = data.get("locator_bar_icon") {
            let icon = crate::nbt::compound(value)?;
            if icon.keys().any(|key| key != "style")
                || icon
                    .get("style")
                    .map(crate::nbt::string)
                    .transpose()?
                    .is_some_and(|id| crate::catalog::namespace(id) != "minecraft:default")
            {
                return Err(
                    "locator_bar_icon: customized waypoint appearance has no legacy representation"
                        .into(),
                );
            }
            if !context.forward() {
                data.remove("locator_bar_icon");
            }
        }
    }
    if matches!(id, "minecraft:tnt" | "minecraft:vex") && data.contains_key("owner") {
        return Err(
            "owner: this entity's persistent owner reference has no legacy representation".into(),
        );
    }
    Ok(())
}

fn empty_potion(data: &Compound) -> Result<bool> {
    Ok(!data.contains_key("potion")
        && !data.contains_key("custom_color")
        && !data.contains_key("custom_name")
        && data
            .get("custom_effects")
            .map(crate::nbt::list)
            .transpose()?
            .is_none_or(Vec::is_empty))
}
