use super::*;
use fastnbt::IntArray;

pub(super) fn pair(
    data: &mut Compound,
    most: &str,
    least: &str,
    target: &str,
    forward: bool,
) -> Result<()> {
    if forward {
        let a = data.remove(most);
        let b = data.remove(least);
        match (a, b) {
            (None, None) => {}
            (Some(V::Long(a)), Some(V::Long(b))) => {
                super::insert(data, target, array(a, b))?;
            }
            _ => return Err(format!("{most}/{least}: expected UUID long pair")),
        }
    } else if let Some(value) = data.remove(target) {
        let (a, b) = longs(&value)?;
        super::insert(data, most, V::Long(a))?;
        super::insert(data, least, V::Long(b))?;
    }
    Ok(())
}

fn array(a: i64, b: i64) -> V {
    V::IntArray(IntArray::new(vec![
        (a >> 32) as i32,
        a as i32,
        (b >> 32) as i32,
        b as i32,
    ]))
}

fn longs(value: &V) -> Result<(i64, i64)> {
    match value {
        V::IntArray(a) if a.len() == 4 => Ok((
            ((a[0] as i64) << 32) | (a[1] as u32 as i64),
            ((a[2] as i64) << 32) | (a[3] as u32 as i64),
        )),
        _ => Err("UUID: expected four integers".into()),
    }
}

pub(super) fn parse(value: &str) -> Result<V> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || bytes.iter().enumerate().any(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                *byte != b'-'
            } else {
                !byte.is_ascii_hexdigit()
            }
        })
    {
        return Err("UUID: expected canonical hexadecimal UUID string".into());
    }
    let hex = value.replace('-', "");
    let a = u64::from_str_radix(&hex[..16], 16).map_err(|e| e.to_string())? as i64;
    let b = u64::from_str_radix(&hex[16..], 16).map_err(|e| e.to_string())? as i64;
    Ok(array(a, b))
}

pub(super) fn format(value: &V) -> Result<String> {
    let (a, b) = longs(value)?;
    let hex = format!("{:016x}{:016x}", a as u64, b as u64);
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

pub(super) fn string(data: &mut Compound, old: &str, new: &str, forward: bool) -> Result<()> {
    if forward {
        if let Some(value) = data.remove(old) {
            let value = parse(crate::nbt::string(&value)?)?;
            super::insert(data, new, value)?;
        }
    } else if let Some(value) = data.remove(new) {
        super::insert(data, old, V::String(format(&value)?))?;
    }
    Ok(())
}

pub(super) fn normalize(value: &mut V) -> Result<()> {
    match value {
        V::List(values) if values.len() == 4 => {
            *value = V::IntArray(IntArray::new(
                values
                    .iter()
                    .map(crate::nbt::number)
                    .collect::<Result<_>>()?,
            ));
        }
        V::IntArray(values) if values.len() == 4 => {}
        _ => return Err("UUID: expected four integers".into()),
    }
    Ok(())
}

pub(super) fn compound(data: &mut Compound, old: &str, new: &str, forward: bool) -> Result<()> {
    if forward {
        if let Some(value) = data.remove(old) {
            let mut owner = crate::nbt::compound(&value)?.clone();
            let a = owner
                .remove("M")
                .ok_or_else(|| format!("{old}.M: missing UUID half"))?;
            let b = owner
                .remove("L")
                .ok_or_else(|| format!("{old}.L: missing UUID half"))?;
            if !owner.is_empty() {
                return Err(format!("{old}: unexpected UUID fields"));
            }
            let (V::Long(a), V::Long(b)) = (a, b) else {
                return Err(format!("{old}: expected UUID long pair"));
            };
            super::insert(data, new, array(a, b))?;
        }
    } else if let Some(value) = data.remove(new) {
        let (a, b) = longs(&value)?;
        super::insert(
            data,
            old,
            V::Compound(Compound::from([
                ("M".into(), V::Long(a)),
                ("L".into(), V::Long(b)),
            ])),
        )?;
    }
    Ok(())
}

pub(super) fn entity(data: &mut Compound, id: &str, context: &Context) -> Result<()> {
    if !context.crosses(2514) {
        return Ok(());
    }
    pair(data, "UUIDMost", "UUIDLeast", "UUID", context.forward())?;
    for key in ["Attributes"] {
        if super::entities::living(id, context)
            && let Some(value) = data.get_mut(key)
        {
            for value in list_mut(value)? {
                let attribute = map_mut(value)?;
                if let Some(value) = attribute.get_mut("Modifiers") {
                    for value in list_mut(value)? {
                        pair(
                            map_mut(value)?,
                            "UUIDMost",
                            "UUIDLeast",
                            "UUID",
                            context.forward(),
                        )?;
                    }
                }
            }
        }
    }
    if super::entities::living(id, context)
        && let Some(value) = data.get_mut("Leash")
    {
        pair(
            map_mut(value)?,
            "UUIDMost",
            "UUIDLeast",
            "UUID",
            context.forward(),
        )?;
    }
    if matches!(
        id,
        "minecraft:horse"
            | "minecraft:donkey"
            | "minecraft:mule"
            | "minecraft:llama"
            | "minecraft:trader_llama"
            | "minecraft:cat"
            | "minecraft:ocelot"
            | "minecraft:wolf"
            | "minecraft:parrot"
    ) {
        string(data, "OwnerUUID", "Owner", context.forward())?;
    }
    if matches!(
        id,
        "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
    ) {
        pair(
            data,
            "OwnerUUIDMost",
            "OwnerUUIDLeast",
            "Owner",
            context.forward(),
        )?;
    }
    if id == "minecraft:area_effect_cloud" {
        pair(
            data,
            "OwnerUUIDMost",
            "OwnerUUIDLeast",
            "Owner",
            context.forward(),
        )?;
    }
    if id == "minecraft:zombie_villager" {
        pair(
            data,
            "ConversionPlayerMost",
            "ConversionPlayerLeast",
            "ConversionPlayer",
            context.forward(),
        )?;
    }
    if id == "minecraft:item" {
        for field in ["Owner", "Thrower"] {
            compound(data, field, field, context.forward())?;
        }
    }
    if id == "minecraft:shulker_bullet" {
        for field in ["Owner", "Target"] {
            compound(data, field, field, context.forward())?;
        }
    }
    if matches!(
        id,
        "minecraft:egg"
            | "minecraft:ender_pearl"
            | "minecraft:snowball"
            | "minecraft:potion"
            | "minecraft:fireball"
            | "minecraft:small_fireball"
            | "minecraft:dragon_fireball"
            | "minecraft:wither_skull"
    ) {
        compound(data, "owner", "Owner", context.forward())?;
    }
    if id == "minecraft:bee" {
        string(data, "HurtBy", "HurtBy", context.forward())?;
    }
    if id == "minecraft:fox" {
        let (old, new) = if context.forward() {
            ("TrustedUUIDs", "Trusted")
        } else {
            ("Trusted", "TrustedUUIDs")
        };
        if let Some(value) = data.remove(old) {
            let mut converted = Vec::new();
            for value in crate::nbt::list(&value)? {
                let mut entry = Compound::from([("uuid".into(), value.clone())]);
                compound(&mut entry, "uuid", "uuid", context.forward())?;
                converted.push(entry.remove("uuid").ok_or("Trusted: missing UUID")?);
            }
            super::insert(data, new, V::List(converted))?;
        }
    }
    if matches!(id, "minecraft:villager" | "minecraft:zombie_villager")
        && let Some(value) = data.get_mut("Gossips")
    {
        for value in list_mut(value)? {
            pair(
                map_mut(value)?,
                "TargetMost",
                "TargetLeast",
                "Target",
                context.forward(),
            )?;
        }
    }
    Ok(())
}
