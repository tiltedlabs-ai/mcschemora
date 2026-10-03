use super::*;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Facts {
    effect_colors: BTreeMap<String, i32>,
    potions: BTreeMap<String, Vec<Effect>>,
}

#[derive(Clone, Deserialize)]
struct Effect {
    id: String,
    duration: i32,
    amplifier: i32,
    show_particles: bool,
}

fn facts() -> Result<&'static Facts> {
    static FACTS: OnceLock<std::result::Result<Facts, String>> = OnceLock::new();
    FACTS
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/potions-1.21.5.json"))
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)
}

pub(super) fn registered(id: &str, context: &Context) -> Result<()> {
    let id = crate::catalog::namespace(id);
    if id == "minecraft:empty" {
        if context.source < 3837 && context.target.data_version < 3837 {
            return Ok(());
        }
        return Err("potion minecraft:empty has no registered component-era representation".into());
    }
    let entries = facts()?
        .potions
        .get(&id)
        .ok_or_else(|| format!("potion {id}: unknown or external potion registry reference"))?;
    for effect in entries {
        super::effects::registered(&effect.id, context)?;
    }
    Ok(())
}

pub(super) fn effects(id: &str) -> Result<Vec<V>> {
    let id = crate::catalog::namespace(id);
    let entries = facts()?.potions.get(&id).ok_or_else(|| {
        format!("potion_contents.potion: unresolved potion registry reference {id}")
    })?;
    Ok(entries
        .iter()
        .map(|effect| {
            V::Compound(Compound::from([
                ("id".into(), V::String(effect.id.clone())),
                ("duration".into(), V::Int(effect.duration)),
                ("amplifier".into(), V::Int(effect.amplifier)),
                (
                    "show_particles".into(),
                    V::Byte(i8::from(effect.show_particles)),
                ),
            ]))
        })
        .collect())
}

pub(super) fn color(data: &Compound) -> Result<i32> {
    if let Some(value) = data.get("custom_color") {
        return Ok(crate::nbt::number(value)? | 0xff000000_u32 as i32);
    }
    let mut values = if let Some(value) = data.get("potion") {
        effects(crate::nbt::string(value)?)?
    } else {
        Vec::new()
    };
    if let Some(value) = data.get("custom_effects") {
        values.extend(crate::nbt::list(value)?.iter().cloned());
    }
    let mut channels = [0_i64; 3];
    let mut weights = 0_i64;
    for value in values {
        let effect = crate::nbt::compound(&value)?;
        if effect
            .get("show_particles")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(1)
            == 0
        {
            continue;
        }
        let id = crate::catalog::namespace(&text(effect, "id")?);
        let color = *facts()?.effect_colors.get(&id).ok_or_else(|| {
            format!("potion_contents.custom_effects.id: unresolved effect color {id}")
        })?;
        let amplifier = effect
            .get("amplifier")
            .map(crate::nbt::number)
            .transpose()?
            .unwrap_or(0);
        if !(0..=255).contains(&amplifier) {
            return Err("potion_contents.custom_effects.amplifier: expected0..255".into());
        }
        let weight = i64::from(amplifier + 1);
        for (index, shift) in [16, 8, 0].into_iter().enumerate() {
            channels[index] += weight * i64::from((color >> shift) & 255);
        }
        weights += weight;
    }
    if weights == 0 {
        return Ok(0xff385dc6_u32 as i32);
    }
    Ok(0xff000000_u32 as i32
        | ((channels[0] / weights) as i32) << 16
        | ((channels[1] / weights) as i32) << 8
        | (channels[2] / weights) as i32)
}
