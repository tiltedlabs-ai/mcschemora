use super::entities::COLORS;
use super::*;

const AXOLOTLS: [&str; 5] = ["lucy", "wild", "gold", "cyan", "blue"];
const PATTERNS: [&str; 12] = [
    "kob",
    "sunstreak",
    "snooper",
    "dasher",
    "brinely",
    "spotty",
    "flopper",
    "stripey",
    "glitter",
    "blockfish",
    "betty",
    "clayfish",
];
const FISH: [&str; 3] = [
    "minecraft:tropical_fish/pattern",
    "minecraft:tropical_fish/base_color",
    "minecraft:tropical_fish/pattern_color",
];

pub(super) fn convert(data: &mut Compound, context: &Context, id: &str) -> Result<()> {
    if !context.crosses(crate::versions::NBT_TEXT_COMPONENTS) {
        return Ok(());
    }
    if !context.forward()
        && let Some(owner) = id.strip_suffix("_spawn_egg")
    {
        spawn_egg(data, owner)?;
    }
    let field = if id == "minecraft:painting" {
        "minecraft:entity_data"
    } else if matches!(
        id,
        "minecraft:axolotl_bucket" | "minecraft:salmon_bucket" | "minecraft:tropical_fish_bucket"
    ) {
        "minecraft:bucket_entity_data"
    } else {
        return Ok(());
    };
    if context.forward() {
        let Some(value) = data.get_mut(field) else {
            return Ok(());
        };
        let payload = map_mut(value)?;
        let mut components = Compound::new();
        match id {
            "minecraft:painting" => {
                if text(payload, "id")? != "minecraft:painting" {
                    return Err("painting.entity_data: entity identity is not a painting".into());
                }
                if let Some(value) = payload.remove("variant") {
                    components.insert("minecraft:painting/variant".into(), value);
                }
            }
            "minecraft:axolotl_bucket" => {
                if let Some(value) = payload.remove("Variant") {
                    components.insert(
                        "minecraft:axolotl/variant".into(),
                        V::String(named(
                            &AXOLOTLS,
                            crate::nbt::number(&value)?,
                            "axolotl variant",
                        )?),
                    );
                }
            }
            "minecraft:salmon_bucket" => {
                if let Some(value) = payload.remove("type") {
                    salmon(&value)?;
                    components.insert("minecraft:salmon/size".into(), value);
                }
            }
            "minecraft:tropical_fish_bucket" => {
                if let Some(value) = payload.remove("BucketVariantTag") {
                    let packed = crate::nbt::number(&value)? as u32;
                    let shape = packed & 255;
                    let pattern = (packed >> 8) & 255;
                    if shape > 1 || pattern > 5 {
                        return Err("BucketVariantTag: invalid tropical fish pattern".into());
                    }
                    for (key, value) in [
                        (
                            FISH[0],
                            named(&PATTERNS, (shape * 6 + pattern) as i32, "fish pattern")?,
                        ),
                        (
                            FISH[1],
                            named(&COLORS, ((packed >> 16) & 255) as i32, "fish base color")?,
                        ),
                        (
                            FISH[2],
                            named(&COLORS, ((packed >> 24) & 255) as i32, "fish pattern color")?,
                        ),
                    ] {
                        components.insert(key.into(), V::String(value));
                    }
                }
            }
            _ => unreachable!(),
        }
        if id == "minecraft:painting"
            && payload.len() == 1
            && payload.contains_key("id")
            && !components.is_empty()
        {
            data.remove(field);
        }
        for (key, value) in components {
            super::insert(data, &key, value)?;
        }
    } else {
        let mut converted = Compound::new();
        match id {
            "minecraft:painting" => {
                if let Some(value) = data.remove("minecraft:painting/variant") {
                    crate::nbt::string(&value)?;
                    converted.insert("variant".into(), value);
                }
            }
            "minecraft:axolotl_bucket" => {
                if let Some(value) = data.remove("minecraft:axolotl/variant") {
                    converted.insert(
                        "Variant".into(),
                        V::Int(index(&AXOLOTLS, &value, "axolotl variant")?),
                    );
                }
            }
            "minecraft:salmon_bucket" => {
                if let Some(value) = data.remove("minecraft:salmon/size") {
                    salmon(&value)?;
                    converted.insert("type".into(), value);
                }
            }
            "minecraft:tropical_fish_bucket" => {
                if let Some(value) = packed_fish(data)? {
                    converted.insert("BucketVariantTag".into(), value);
                }
            }
            _ => unreachable!(),
        }
        if !converted.is_empty() {
            if data.contains_key(&format!("!{field}")) {
                return Err(format!(
                    "{field}: required variant payload conflicts with component removal"
                ));
            }
            let value = data
                .entry(field.into())
                .or_insert_with(|| V::Compound(Compound::new()));
            let payload = map_mut(value)?;
            if id == "minecraft:painting" {
                if let Some(value) = payload.get("id") {
                    if crate::nbt::string(value)? != "minecraft:painting" {
                        return Err("painting.entity_data: conflicting entity identity".into());
                    }
                } else {
                    payload.insert("id".into(), V::String("minecraft:painting".into()));
                }
            }
            for (key, value) in converted {
                super::insert(payload, &key, value)?;
            }
        }
    }
    Ok(())
}

fn named(names: &[&str], value: i32, field: &str) -> Result<String> {
    usize::try_from(value)
        .ok()
        .and_then(|index| names.get(index))
        .map(|name| (*name).to_string())
        .ok_or_else(|| format!("{field}: unknown value {value}"))
}

fn index(names: &[&str], value: &V, field: &str) -> Result<i32> {
    let value = crate::nbt::string(value)?;
    names
        .iter()
        .position(|name| *name == value)
        .map(|index| index as i32)
        .ok_or_else(|| format!("{field}: unknown value {value}"))
}

fn salmon(value: &V) -> Result<()> {
    if !matches!(crate::nbt::string(value)?, "small" | "medium" | "large") {
        return Err("salmon size: expected small, medium or large".into());
    }
    Ok(())
}

fn spawn_egg(data: &mut Compound, owner: &str) -> Result<()> {
    let kind = owner.strip_prefix("minecraft:").unwrap_or(owner);
    let key = match kind {
        "cat" | "wolf" | "frog" | "rabbit" | "parrot" | "llama" | "trader_llama" | "fox"
        | "mooshroom" | "axolotl" | "pig" | "cow" | "chicken" => "variant",
        "sheep" | "shulker" => "color",
        "salmon" => "size",
        "tropical_fish" => "pattern",
        _ => return Ok(()),
    };
    let component_owner = if kind == "trader_llama" {
        "llama"
    } else {
        kind
    };
    let component = format!("minecraft:{component_owner}/{key}");
    let mut fields = Compound::new();
    if kind != "tropical_fish"
        && let Some(value) = data.remove(&component)
    {
        let (field, value) = match kind {
            "cat" | "wolf" | "frog" | "pig" | "cow" | "chicken" => {
                let id = crate::catalog::namespace(crate::nbt::string(&value)?);
                let known = if kind == "cat" {
                    &[
                        "tabby",
                        "black",
                        "red",
                        "siamese",
                        "british_shorthair",
                        "calico",
                        "persian",
                        "ragdoll",
                        "white",
                        "jellie",
                        "all_black",
                    ][..]
                } else if kind == "wolf" {
                    &[
                        "striped", "chestnut", "black", "rusty", "pale", "woods", "snowy", "ashen",
                        "spotted",
                    ][..]
                } else {
                    &["temperate", "warm", "cold"][..]
                };
                if !known.iter().any(|name| id == format!("minecraft:{name}")) {
                    return Err(format!(
                        "{component}: external variant requires registry context"
                    ));
                }
                ("variant", V::String(id))
            }
            "axolotl" => (
                "Variant",
                V::Int(index(&AXOLOTLS, &value, "axolotl variant")?),
            ),
            "salmon" => {
                salmon(&value)?;
                ("type", value)
            }
            "rabbit" => {
                let value = if crate::nbt::string(&value)? == "evil" {
                    99
                } else {
                    index(
                        &["brown", "white", "black", "white_splotched", "gold", "salt"],
                        &value,
                        "rabbit variant",
                    )?
                };
                ("RabbitType", V::Int(value))
            }
            "parrot" => (
                "Variant",
                V::Int(index(
                    &["red_blue", "blue", "green", "yellow_blue", "gray"],
                    &value,
                    "parrot variant",
                )?),
            ),
            "llama" | "trader_llama" => (
                "Variant",
                V::Int(index(
                    &["creamy", "white", "brown", "gray"],
                    &value,
                    "llama variant",
                )?),
            ),
            "sheep" | "shulker" => (
                "Color",
                V::Byte(index(&COLORS, &value, "entity color")? as i8),
            ),
            "fox" | "mooshroom" => {
                let names = if kind == "fox" {
                    &["red", "snow"][..]
                } else {
                    &["red", "brown"][..]
                };
                index(names, &value, "entity variant")?;
                ("Type", value)
            }
            _ => unreachable!(),
        };
        fields.insert(field.into(), value);
    }
    if matches!(kind, "cat" | "wolf")
        && let Some(value) = data.remove(&format!("minecraft:{kind}/collar"))
    {
        fields.insert(
            "CollarColor".into(),
            V::Byte(index(&COLORS, &value, "animal collar")? as i8),
        );
    }
    if kind == "wolf"
        && let Some(value) = data.remove("minecraft:wolf/sound_variant")
    {
        let id = crate::catalog::namespace(crate::nbt::string(&value)?);
        if id != "minecraft:classic" {
            return Err(
                "wolf/sound_variant: only classic wolf sounds have an older equivalent".into(),
            );
        }
        fields.insert("sound_variant".into(), V::String(id));
    }
    if kind == "tropical_fish"
        && let Some(value) = packed_fish(data)?
    {
        fields.insert("Variant".into(), value);
    }
    if fields.is_empty() {
        return Ok(());
    }
    if data.contains_key("!minecraft:entity_data") {
        return Err("variant: entity data conflicts with its explicit removal".into());
    }
    let payload = data
        .entry("minecraft:entity_data".into())
        .or_insert_with(|| V::Compound(Compound::new()));
    let payload = map_mut(payload)?;
    super::insert(payload, "id", V::String(owner.into()))?;
    for (key, value) in fields {
        super::insert(payload, &key, value)?;
    }
    Ok(())
}

fn packed_fish(data: &mut Compound) -> Result<Option<V>> {
    let count = FISH.iter().filter(|key| data.contains_key(**key)).count();
    if count == 0 {
        return Ok(None);
    }
    if count != 3 {
        return Err("tropical fish variant: partial color/pattern components require source-effective spawning defaults".into());
    }
    let pattern = index(&PATTERNS, &data.remove(FISH[0]).unwrap(), "fish pattern")? as u32;
    let base = index(&COLORS, &data.remove(FISH[1]).unwrap(), "fish base color")? as u32;
    let color = index(
        &COLORS,
        &data.remove(FISH[2]).unwrap(),
        "fish pattern color",
    )? as u32;
    let packed = (pattern / 6) | ((pattern % 6) << 8) | (base << 16) | (color << 24);
    Ok(Some(V::Int(packed as i32)))
}
