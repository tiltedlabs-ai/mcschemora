use super::*;

const IDS: [&str; 34] = [
    "",
    "speed",
    "slowness",
    "haste",
    "mining_fatigue",
    "strength",
    "instant_health",
    "instant_damage",
    "jump_boost",
    "nausea",
    "regeneration",
    "resistance",
    "fire_resistance",
    "water_breathing",
    "invisibility",
    "blindness",
    "night_vision",
    "hunger",
    "weakness",
    "poison",
    "wither",
    "health_boost",
    "absorption",
    "saturation",
    "glowing",
    "levitation",
    "luck",
    "unluck",
    "slow_falling",
    "conduit_power",
    "dolphins_grace",
    "bad_omen",
    "hero_of_the_village",
    "darkness",
];

pub(super) fn identifier(
    data: &mut Compound,
    old: &str,
    new: &str,
    context: &Context,
) -> Result<()> {
    let forward = context.forward();
    let source = if forward { old } else { new };
    let target = if forward { new } else { old };
    if let Some(value) = data.remove(source) {
        if forward
            && matches!(old, "Primary" | "Secondary")
            && matches!(crate::nbt::number(&value)?, -1 | 0)
        {
            return Ok(());
        }
        let value = if forward {
            let id = crate::nbt::number(&value)?;
            let name = IDS
                .get(usize::try_from(id).map_err(|_| "negative effect ID")?)
                .filter(|name| !name.is_empty())
                .ok_or_else(|| format!("unmapped numeric effect ID {id}"))?;
            registered(name, context)?;
            V::String(format!("minecraft:{name}"))
        } else {
            let id = crate::catalog::namespace(crate::nbt::string(&value)?);
            registered(&id, context)?;
            let number = IDS
                .iter()
                .position(|name| id == format!("minecraft:{name}"))
                .ok_or_else(|| format!("effect {id} has no legacy numeric ID"))?;
            if old == "Id" {
                V::Byte(number as i8)
            } else {
                V::Int(number as i32)
            }
        };
        super::insert(data, target, value)?;
    }
    Ok(())
}

fn entry(data: &mut Compound, context: &Context, stew: bool, level: usize) -> Result<()> {
    depth(level)?;
    identifier(data, if stew { "EffectId" } else { "Id" }, "id", context)?;
    let fields: &[(&str, &str)] = if stew {
        &[("EffectDuration", "duration")]
    } else {
        &[
            ("Ambient", "ambient"),
            ("Amplifier", "amplifier"),
            ("Duration", "duration"),
            ("ShowParticles", "show_particles"),
            ("ShowIcon", "show_icon"),
            ("FactorCalculationData", "factor_calculation_data"),
            ("HiddenEffect", "hidden_effect"),
        ]
    };
    for (old, new) in fields {
        if context.forward() {
            move_field(data, old, new)?;
        } else {
            move_field(data, new, old)?;
        }
    }
    let key = if context.forward() {
        "hidden_effect"
    } else {
        "HiddenEffect"
    };
    if let Some(value) = data.get_mut(key) {
        entry(map_mut(value)?, context, false, level + 1)?;
    }
    Ok(())
}

pub(super) fn list(
    data: &mut Compound,
    old: &str,
    new: &str,
    context: &Context,
    stew: bool,
) -> Result<()> {
    let (source, target) = if context.forward() {
        (old, new)
    } else {
        (new, old)
    };
    if let Some(mut value) = data.remove(source) {
        for (index, value) in list_mut(&mut value)?.iter_mut().enumerate() {
            entry(map_mut(value)?, context, stew, 0)
                .map_err(|e| format!("{source}[{index}].{e}"))?;
        }
        super::insert(data, target, value)?;
    }
    Ok(())
}

pub(super) fn entity(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    let legacy = context.source < 3568;
    if context.legacy() && matches!(id, "minecraft:area_effect_cloud" | "minecraft:arrow") {
        let effect_key = if id == "minecraft:arrow" {
            "custom_potion_effects"
        } else {
            "effects"
        };
        if let Some(key) = ["Potion", "Color", effect_key]
            .iter()
            .find(|key| data.contains_key(**key))
        {
            return Err(format!(
                "{key}: ignored source field would become active in the older potion schema"
            ));
        }
    }
    if context.components()
        && id == "minecraft:area_effect_cloud"
        && data.contains_key("potion_contents")
    {
        return Err("potion_contents: source already contains the destination potion field".into());
    }
    if context.source < 3837
        && matches!(id, "minecraft:area_effect_cloud" | "minecraft:arrow")
        && let Some(value) = data.get("Potion")
    {
        super::potions::registered(crate::nbt::string(value)?, context)
            .map_err(|error| format!("Potion.{error}"))?;
    }

    if context.source >= 3837
        && id == "minecraft:area_effect_cloud"
        && let Some(value) = data.get("potion_contents")
    {
        potion(crate::nbt::compound(value)?, context)
            .map_err(|error| format!("potion_contents.{error}"))?;
    }
    if id == "minecraft:mooshroom"
        && !legacy
        && let Some(value) = data.get("stew_effects")
    {
        validate(value, context, true, 0).map_err(|error| format!("stew_effects.{error}"))?;
    }
    let fields: &[&str] = if id == "minecraft:area_effect_cloud" {
        if legacy {
            &["Effects"]
        } else if context.source < 3837 {
            &["effects"]
        } else {
            &[]
        }
    } else if id == "minecraft:arrow" {
        if legacy {
            &["CustomPotionEffects"]
        } else if context.source < 3837 {
            &["custom_potion_effects"]
        } else {
            &[]
        }
    } else if super::entities::living(id, context) {
        if legacy {
            &["ActiveEffects"]
        } else {
            &["active_effects"]
        }
    } else {
        &[]
    };
    for field in fields {
        if let Some(value) = data.get(*field) {
            validate(value, context, false, 0).map_err(|e| format!("{field}.{e}"))?;
        }
    }

    if context.crosses(3322) {
        let fields: &[&str] = if id == "minecraft:area_effect_cloud" {
            &["Effects"]
        } else if id == "minecraft:arrow" {
            &["CustomPotionEffects"]
        } else if super::entities::living(id, context) {
            &["ActiveEffects"]
        } else {
            &[]
        };
        for field in fields {
            if let Some(value) = data.get_mut(*field) {
                clock_list(value, context, 0).map_err(|e| format!("{field}.{e}"))?;
            }
        }
    }
    if !context.crosses(3568) {
        return Ok(());
    }
    if super::entities::living(id, context) {
        if context.forward() {
            if data
                .get("AbsorptionAmount")
                .is_some_and(|v| matches!(v,V::Float(n) if *n!=0.0))
            {
                return Err("AbsorptionAmount: maximum-absorption default migration requires effective attribute context".into());
            }
        } else if let Some(value) = data.get_mut("Attributes") {
            let values = list_mut(value)?;
            for value in values.iter() {
                let attribute = crate::nbt::compound(value)?;
                if attribute.get("Name")
                    == Some(&V::String("minecraft:generic.max_absorption".into()))
                    && (attribute.get("Base") != Some(&V::Double(0.0))
                        || attribute.contains_key("Modifiers"))
                {
                    return Err("Attributes.generic.max_absorption: nondefault cap cannot be represented before1.20.2".into());
                }
            }
            values.retain(|value|!matches!(value,V::Compound(c) if c.get("Name")==Some(&V::String("minecraft:generic.max_absorption".into()))));
        }
        list(data, "ActiveEffects", "active_effects", context, false)?;
    }
    if id == "minecraft:arrow" {
        list(
            data,
            "CustomPotionEffects",
            "custom_potion_effects",
            context,
            false,
        )?;
    }
    if id == "minecraft:area_effect_cloud" {
        list(data, "Effects", "effects", context, false)?;
    }
    if id == "minecraft:mooshroom" {
        if context.forward() {
            if data.contains_key("EffectId") || data.contains_key("EffectDuration") {
                let mut effect = Compound::new();
                for key in ["EffectId", "EffectDuration"] {
                    if let Some(value) = data.remove(key) {
                        effect.insert(key.into(), value);
                    }
                }
                entry(&mut effect, context, true, 0)?;
                super::insert(data, "stew_effects", V::List(vec![V::Compound(effect)]))?;
            }
        } else if let Some(value) = data.remove("stew_effects") {
            let values = crate::nbt::list(&value)?;
            if values.len() > 1 {
                return Err(
                    "stew_effects: multiple effects cannot be represented before1.20.2".into(),
                );
            }
            if let Some(value) = values.first() {
                let mut effect = crate::nbt::compound(value)?.clone();
                entry(&mut effect, context, true, 0)?;
                for key in ["EffectId", "EffectDuration"] {
                    if let Some(value) = effect.remove(key) {
                        super::insert(data, key, value)?;
                    }
                }
                if !effect.is_empty() {
                    return Err("stew_effects: extra fields cannot be represented".into());
                }
            }
        }
    }
    Ok(())
}

pub(super) fn item(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if let Some(value) = data.get("tag") {
        let tag = crate::nbt::compound(value)?;
        if context.source < 3837
            && let Some(value) = tag.get("Potion")
        {
            super::potions::registered(crate::nbt::string(value)?, context)
                .map_err(|error| format!("tag.Potion.{error}"))?;
        }

        let legacy = context.source < 3568;
        let field = if id == "minecraft:suspicious_stew" {
            Some((if legacy { "Effects" } else { "effects" }, true))
        } else if matches!(
            id,
            "minecraft:potion"
                | "minecraft:splash_potion"
                | "minecraft:lingering_potion"
                | "minecraft:tipped_arrow"
        ) {
            Some((
                if legacy {
                    "CustomPotionEffects"
                } else {
                    "custom_potion_effects"
                },
                false,
            ))
        } else {
            None
        };
        if let Some((field, stew)) = field
            && let Some(value) = tag.get(field)
        {
            validate(value, context, stew, 0).map_err(|e| format!("tag.{field}.{e}"))?;
        }
    }

    if context.crosses(3322)
        && let Some(value) = data.get_mut("tag")
        && let Some(value) = map_mut(value)?.get_mut("CustomPotionEffects")
    {
        clock_list(value, context, 0).map_err(|e| format!("tag.CustomPotionEffects.{e}"))?;
    }
    if !context.crosses(3568) {
        return Ok(());
    }
    if let Some(value) = data.get_mut("tag") {
        let data = map_mut(value)?;
        if id == "minecraft:suspicious_stew" {
            list(data, "Effects", "effects", context, true)?;
        }
        if matches!(
            id,
            "minecraft:potion"
                | "minecraft:splash_potion"
                | "minecraft:lingering_potion"
                | "minecraft:tipped_arrow"
        ) {
            list(
                data,
                "CustomPotionEffects",
                "custom_potion_effects",
                context,
                false,
            )?;
        }
    }
    Ok(())
}

fn clock_list(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    for (index, value) in list_mut(value)?.iter_mut().enumerate() {
        clock(map_mut(value)?, context, level + 1).map_err(|e| format!("[{index}].{e}"))?;
    }
    Ok(())
}

fn clock(data: &mut Compound, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let duration = data
        .get("Duration")
        .map(crate::nbt::number)
        .transpose()?
        .unwrap_or(0);
    if let Some(value) = data.get_mut("FactorCalculationData") {
        let factor = map_mut(value)?;
        let (old, new) = if context.forward() {
            ("effect_changed_timestamp", "ticks_active")
        } else {
            ("ticks_active", "effect_changed_timestamp")
        };
        if let Some(value) = factor.remove(old) {
            let value = crate::nbt::number(&value)?;
            let value = if context.forward() {
                value.checked_sub(duration)
            } else {
                value.checked_add(duration)
            }
            .ok_or("FactorCalculationData: effect clock overflow")?;
            super::insert(factor, new, V::Int(value))?;
        }
    }
    if let Some(value) = data.get_mut("HiddenEffect") {
        clock(map_mut(value)?, context, level + 1)?;
    }
    Ok(())
}

pub(super) fn registered(id: &str, context: &Context) -> Result<()> {
    let id = crate::catalog::namespace(id);
    let minimum = if let Some(number) = IDS
        .iter()
        .position(|name| id == format!("minecraft:{name}"))
    {
        match number {
            1..=30 => 1519,
            31..=32 => 1952,
            33 => 3105,
            _ => return Err(format!("unknown effect {id}")),
        }
    } else {
        match id.as_str() {
            "minecraft:raid_omen"
            | "minecraft:trial_omen"
            | "minecraft:wind_charged"
            | "minecraft:weaving"
            | "minecraft:oozing"
            | "minecraft:infested" => 3837,
            "minecraft:breath_of_the_nautilus" => 4671,
            _ => {
                return Err(format!(
                    "effect {id}: unknown or external effect registry reference"
                ));
            }
        }
    };
    if context.source < minimum || context.target.data_version < minimum {
        return Err(format!(
            "effect {id} is unavailable in the source or target release"
        ));
    }
    Ok(())
}

fn validate_entry(
    effect: &Compound,
    context: &Context,
    stew: bool,
    legacy: bool,
    level: usize,
) -> Result<()> {
    depth(level)?;
    if legacy {
        let number = crate::nbt::number(crate::nbt::get(
            effect,
            if stew { "EffectId" } else { "Id" },
        )?)?;
        let name = usize::try_from(number)
            .ok()
            .and_then(|index| IDS.get(index))
            .filter(|name| !name.is_empty())
            .ok_or_else(|| format!("unknown numeric effect {number}"))?;
        registered(name, context)?;
    } else {
        registered(crate::nbt::string(crate::nbt::get(effect, "id")?)?, context)?;
    }
    if let Some(hidden) = effect.get(if legacy {
        "HiddenEffect"
    } else {
        "hidden_effect"
    }) {
        validate_entry(
            crate::nbt::compound(hidden)?,
            context,
            false,
            legacy,
            level + 1,
        )
        .map_err(|error| format!("hidden_effect.{error}"))?;
    }
    Ok(())
}

pub(super) fn validate(value: &V, context: &Context, stew: bool, level: usize) -> Result<()> {
    for (index, value) in crate::nbt::list(value)?.iter().enumerate() {
        validate_entry(
            crate::nbt::compound(value)?,
            context,
            stew,
            context.source < 3568,
            level,
        )
        .map_err(|error| format!("[{index}].{error}"))?;
    }
    Ok(())
}

fn potion(data: &Compound, context: &Context) -> Result<()> {
    if let Some(value) = data.get("potion") {
        super::potions::registered(crate::nbt::string(value)?, context)
            .map_err(|error| format!("potion.{error}"))?;
    }
    if let Some(value) = data.get("custom_effects") {
        validate(value, context, false, 0).map_err(|error| format!("custom_effects.{error}"))?;
    }
    Ok(())
}

pub(super) fn components(data: &Compound, context: &Context) -> Result<()> {
    if let Some(value) = data.get("minecraft:potion_contents") {
        potion(crate::nbt::compound(value)?, context)
            .map_err(|error| format!("potion_contents.{error}"))?;
    }
    if let Some(value) = data.get("minecraft:suspicious_stew_effects") {
        validate(value, context, true, 0)
            .map_err(|error| format!("suspicious_stew_effects.{error}"))?;
    }
    if context.source < 4059
        && let Some(V::Compound(food)) = data.get("minecraft:food")
        && let Some(value) = food.get("effects")
    {
        for (index, value) in crate::nbt::list(value)?.iter().enumerate() {
            let effect = crate::nbt::get(crate::nbt::compound(value)?, "effect")?;
            validate_entry(crate::nbt::compound(effect)?, context, false, false, 0)
                .map_err(|error| format!("food.effects[{index}].effect.{error}"))?;
        }
    }
    if let Some(V::Compound(consumable)) = data.get("minecraft:consumable")
        && let Some(value) = consumable.get("on_consume_effects")
    {
        for (index, value) in crate::nbt::list(value)?.iter().enumerate() {
            let effect = crate::nbt::compound(value)?;
            let kind = crate::catalog::namespace(&text(effect, "type")?);
            let checked = match kind.as_str() {
                "minecraft:apply_effects" => {
                    validate(crate::nbt::get(effect, "effects")?, context, false, 0)
                }
                "minecraft:remove_effects" => {
                    let value = crate::nbt::get(effect, "effects")?;
                    match value {
                        V::String(id) => registered(id, context),
                        V::List(values) => values
                            .iter()
                            .try_for_each(|value| registered(crate::nbt::string(value)?, context)),
                        _ => Err("effects: expected registered effect ID or list".into()),
                    }
                }
                "minecraft:clear_all_effects"
                | "minecraft:play_sound"
                | "minecraft:teleport_randomly" => Ok(()),
                _ => Err(format!("unknown consume effect type {kind}")),
            };
            checked.map_err(|error| format!("consumable.on_consume_effects[{index}].{error}"))?;
        }
    }
    Ok(())
}

pub(super) fn remove_interpolation(value: &mut V, context: &Context, field: &str) -> Result<()> {
    for (index, value) in list_mut(value)?.iter_mut().enumerate() {
        remove_interpolation_entry(map_mut(value)?, context, &format!("{field}[{index}]"), 0)?;
    }
    Ok(())
}

fn remove_interpolation_entry(
    effect: &mut Compound,
    context: &Context,
    field: &str,
    level: usize,
) -> Result<()> {
    depth(level)?;
    for key in ["FactorCalculationData", "factor_calculation_data"] {
        if effect.remove(key).is_some() {
            context.loss(
                &format!("{field}.{key}"),
                "effect interpolation state has no representation in Java1.20.5",
            );
        }
    }
    if let Some(value) = effect.get_mut("hidden_effect") {
        remove_interpolation_entry(
            map_mut(value)?,
            context,
            &format!("{field}.hidden_effect"),
            level + 1,
        )?;
    }
    Ok(())
}
