use super::*;
use std::collections::BTreeMap;

fn known(id: &str, version: i32) -> Result<()> {
    type Lifetimes = BTreeMap<String, Vec<(i32, Option<i32>)>>;
    static IDS: OnceLock<Lifetimes> = OnceLock::new();
    let ids =
        IDS.get_or_init(|| serde_json::from_str(include_str!("data/particles.json")).unwrap());
    if ids.get(id).is_some_and(|ranges| {
        ranges
            .iter()
            .any(|(start, end)| *start <= version && end.is_none_or(|end| version < end))
    }) {
        Ok(())
    } else {
        Err(format!(
            "particle {id} does not exist in data version {version}"
        ))
    }
}

fn float(value: &V) -> Result<f32> {
    let value = match value {
        V::Float(n) => *n,
        V::Double(n) => *n as f32,
        V::Int(n) => *n as f32,
        _ => return Err("particle parameter must be numeric".into()),
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err("particle parameter must be finite".into())
    }
}

fn numbers(input: &str, count: usize) -> Result<Vec<f32>> {
    let values = input
        .split_whitespace()
        .map(|part| {
            let value = part
                .parse::<f32>()
                .map_err(|_| "invalid particle numeric parameter")?;
            float(&V::Float(value))
        })
        .collect::<Result<Vec<_>>>()?;
    if values.len() != count {
        return Err(format!("particle requires {count} numeric parameters"));
    }
    Ok(values)
}

fn block_nbt(block: Block, version: i32) -> Compound {
    let (name, properties) = if version >= 5006 {
        ("id", "properties")
    } else {
        ("Name", "Properties")
    };
    let mut data = Compound::from([(name.into(), V::String(block.id))]);
    if !block.properties.is_empty() {
        data.insert(
            properties.into(),
            V::Compound(
                block
                    .properties
                    .into_iter()
                    .map(|(key, value)| (key, V::String(value)))
                    .collect(),
            ),
        );
    }
    data
}

fn decode(input: &str) -> Result<Compound> {
    if input.len() > 1024 * 1024 {
        return Err("particle exceeds 1 MiB".into());
    }
    let input = input.trim();
    let (id, options) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
    let id = crate::catalog::namespace(id);
    let options = options.trim();
    let mut data = Compound::from([("type".into(), V::String(id.clone()))]);
    match id.as_str() {
        "minecraft:block"
        | "minecraft:block_marker"
        | "minecraft:falling_dust"
        | "minecraft:dust_pillar"
        | "minecraft:block_crumble" => {
            data.insert(
                "block_state".into(),
                V::Compound(block_nbt(Block::parse(options)?, 0)),
            );
        }
        "minecraft:item" => {
            let end = options.find('{').unwrap_or(options.len());
            let mut item = Compound::from([
                (
                    "id".into(),
                    V::String(crate::catalog::namespace(options[..end].trim())),
                ),
                ("Count".into(), V::Byte(1)),
            ]);
            if end < options.len() {
                item.insert(
                    "tag".into(),
                    V::Compound(crate::nbt::from_snbt(&options[end..])?),
                );
            }
            data.insert("item".into(), V::Compound(item));
        }
        "minecraft:dust" | "minecraft:dust_color_transition" => {
            let transition = id == "minecraft:dust_color_transition";
            let values = numbers(options, if transition { 7 } else { 4 })?;
            data.insert(
                if transition { "from_color" } else { "color" }.into(),
                V::List(values[..3].iter().copied().map(V::Float).collect()),
            );
            data.insert("scale".into(), V::Float(values[3].clamp(0.01, 4.0)));
            if transition {
                data.insert(
                    "to_color".into(),
                    V::List(values[4..].iter().copied().map(V::Float).collect()),
                );
            }
        }
        "minecraft:sculk_charge" => {
            data.insert("roll".into(), V::Float(numbers(options, 1)?[0]));
        }
        "minecraft:shriek" => {
            let delay = options
                .parse::<i32>()
                .map_err(|_| "shriek delay must be an integer")?;
            if delay < 0 {
                return Err("shriek delay must be nonnegative".into());
            }
            data.insert("delay".into(), V::Int(delay));
        }
        "minecraft:vibration" => {
            let tokens: Vec<_> = options.split_whitespace().collect();
            if tokens.len() != 4 {
                return Err("vibration requires three coordinates and arrival ticks".into());
            }
            let pos = tokens[..3]
                .iter()
                .map(|token| {
                    let value = token
                        .parse::<f64>()
                        .map_err(|_| "invalid vibration coordinate")?
                        .floor();
                    if !value.is_finite()
                        || value < f64::from(i32::MIN)
                        || value > f64::from(i32::MAX)
                    {
                        return Err("vibration coordinate exceeds block position range".into());
                    }
                    Ok(value as i32)
                })
                .collect::<Result<Vec<_>>>()?;
            let ticks = tokens[3]
                .parse::<i32>()
                .map_err(|_| "vibration arrival ticks must be an integer")?;
            if ticks < 0 {
                return Err("vibration arrival ticks must be nonnegative".into());
            }
            data.insert("arrival_in_ticks".into(), V::Int(ticks));
            data.insert(
                "destination".into(),
                V::Compound(Compound::from([
                    ("type".into(), V::String("minecraft:block".into())),
                    ("pos".into(), V::IntArray(fastnbt::IntArray::new(pos))),
                ])),
            );
        }
        _ if !options.is_empty() => {
            return Err(format!("unsupported particle parameters for {id}"));
        }
        _ => {}
    }
    Ok(data)
}

fn vector(data: &mut Compound, field: &str) -> Result<String> {
    let value = data
        .remove(field)
        .ok_or_else(|| format!("particle.{field} is missing"))?;
    let values = crate::nbt::list(&value)?;
    if values.len() != 3 {
        return Err(format!("particle.{field} must have three components"));
    }
    Ok(values
        .iter()
        .map(|v| Ok(float(v)?.to_string()))
        .collect::<Result<Vec<_>>>()?
        .join(" "))
}

fn scalar(data: &mut Compound, field: &str) -> Result<String> {
    let value = data
        .remove(field)
        .ok_or_else(|| format!("particle.{field} is missing"))?;
    Ok(float(&value)?.to_string())
}

fn encode(mut data: Compound) -> Result<String> {
    let id = text(&data, "type")?;
    data.remove("type");
    let options = match id.as_str() {
        "minecraft:block" | "minecraft:block_marker" | "minecraft:falling_dust" => {
            let value = data
                .remove("block_state")
                .ok_or("particle.block_state is missing")?;
            let block = crate::nbt::compound(&value)?;
            if block
                .keys()
                .any(|k| !matches!(k.as_str(), "Name" | "Properties"))
            {
                return Err("particle block state has unsupported fields".into());
            }
            let mut properties = BTreeMap::new();
            if let Some(value) = block.get("Properties") {
                for (key, value) in crate::nbt::compound(value)? {
                    properties.insert(key.clone(), crate::nbt::string(value)?.into());
                }
            }
            Block::new(&text(block, "Name")?, properties)?.text()
        }
        "minecraft:item" => {
            let value = data.remove("item").ok_or("particle.item is missing")?;
            let mut item = crate::nbt::compound(&value)?.clone();
            let id = text(&item, "id")?;
            item.remove("id");
            if let Some(count) = item.remove("Count")
                && crate::nbt::number(&count)? != 1
            {
                return Err("particle item count must be one".into());
            }
            let tag = item.remove("tag");
            if !item.is_empty() {
                return Err("particle item has unsupported legacy fields".into());
            }
            match tag {
                Some(value) => format!(
                    "{id}{}",
                    crate::nbt::to_snbt(crate::nbt::compound(&value)?)?
                ),
                None => id,
            }
        }
        "minecraft:dust" => format!(
            "{} {}",
            vector(&mut data, "color")?,
            scalar(&mut data, "scale")?
        ),
        "minecraft:dust_color_transition" => format!(
            "{} {} {}",
            vector(&mut data, "from_color")?,
            scalar(&mut data, "scale")?,
            vector(&mut data, "to_color")?
        ),
        "minecraft:sculk_charge" => scalar(&mut data, "roll")?,
        "minecraft:shriek" => {
            let value = data.remove("delay").ok_or("particle.delay is missing")?;
            let n = crate::nbt::number(&value)?;
            if n < 0 {
                return Err("particle.delay must be nonnegative".into());
            }
            n.to_string()
        }
        "minecraft:vibration" => {
            let value = data
                .remove("destination")
                .ok_or("particle.destination is missing")?;
            let destination = crate::nbt::compound(&value)?;
            if destination.len() != 2
                || crate::catalog::namespace(&text(destination, "type")?) != "minecraft:block"
            {
                return Err("legacy vibration particles require a block destination".into());
            }
            let position = crate::nbt::xyz(crate::nbt::get(destination, "pos")?)?;
            if position.len() != 3 {
                return Err("vibration destination requires three coordinates".into());
            }
            let value = data
                .remove("arrival_in_ticks")
                .ok_or("particle.arrival_in_ticks is missing")?;
            let ticks = crate::nbt::number(&value)?;
            if ticks < 0 {
                return Err("vibration arrival ticks must be nonnegative".into());
            }
            format!("{} {} {} {ticks}", position[0], position[1], position[2])
        }
        _ => String::new(),
    };
    if !data.is_empty() {
        return Err(format!(
            "{id}: particle fields cannot be represented in the legacy string"
        ));
    }
    Ok(if options.is_empty() {
        id
    } else {
        format!("{id} {options}")
    })
}

pub(super) fn convert(value: &mut V, context: &Context, level: usize) -> Result<()> {
    depth(level)?;
    let mut data = if context.source < crate::versions::ITEM_COMPONENTS {
        decode(crate::nbt::string(value)?)?
    } else {
        crate::nbt::compound(value)?.clone()
    };
    let id = crate::catalog::namespace(&text(&data, "type")?);
    known(&id, context.source)?;
    known(&id, context.target.data_version)?;
    data.insert("type".into(), V::String(id.clone()));
    if id == "minecraft:entity_effect" && context.components() {
        data.insert("color".into(), V::Int(0xff000000u32 as i32));
    }
    if matches!(
        id.as_str(),
        "minecraft:dust" | "minecraft:dust_color_transition"
    ) {
        let scale = float(crate::nbt::get(&data, "scale")?)?;
        if !(0.01..=4.0).contains(&scale) {
            return Err("particle.scale must be between 0.01 and 4".into());
        }
    }
    if matches!(
        id.as_str(),
        "minecraft:block"
            | "minecraft:block_marker"
            | "minecraft:falling_dust"
            | "minecraft:dust_pillar"
            | "minecraft:block_crumble"
    ) {
        let state = data
            .get_mut("block_state")
            .ok_or("particle.block_state is missing")?;
        if let V::String(id) = state {
            *state = V::Compound(block_nbt(Block::parse(id)?, context.source));
        }
        blocks::nbt(map_mut(state)?, context)?;
    }
    if id == "minecraft:item" {
        let item = data.get_mut("item").ok_or("particle.item is missing")?;
        if let V::String(id) = item {
            *item = V::Compound(Compound::from([
                ("id".into(), V::String(id.clone())),
                ("count".into(), V::Int(1)),
            ]));
        }
        items::convert(map_mut(item)?, context, level + 1)?;
    }
    *value = if context.target.data_version < crate::versions::ITEM_COMPONENTS {
        V::String(encode(data)?)
    } else {
        V::Compound(data)
    };
    Ok(())
}
