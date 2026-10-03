use super::*;

pub(super) fn convert(region: &mut Region, context: &Context) -> Result<()> {
    if region.retained.bedrock.is_some() {
        return Err("cannot convert retained Bedrock data".into());
    }
    let volume = region.bounds.volume()?;
    for (key, value) in &mut region.retained.spatial {
        match key.as_str() {
            "Biomes" => biomes(value, volume, context)?,
            "PendingBlockTicks" | "PendingFluidTicks" => {
                let field = if key == "PendingBlockTicks" {
                    "Block"
                } else {
                    "Fluid"
                };
                if context.crosses(2860) {
                    let ticks = list_mut(value)?;
                    if context.forward() {
                        for (index, tick) in ticks.iter_mut().enumerate() {
                            let tick = map_mut(tick)?;
                            if tick.contains_key("SubTick") {
                                return Err(format!(
                                    "{key}[{index}].SubTick: source field collides with the new ordering schema"
                                ));
                            }
                            tick.insert("SubTick".into(), V::Long(index as i64));
                        }
                    } else {
                        let mut ordered = Vec::with_capacity(ticks.len());
                        for tick in ticks.drain(..) {
                            let mut tick = crate::nbt::compound(&tick)?.clone();
                            let order = match tick.remove("SubTick") {
                                None => 0,
                                Some(V::Long(value)) => value,
                                Some(value) => crate::nbt::number(&value)? as i64,
                            };
                            ordered.push((order, V::Compound(tick)));
                        }
                        ordered.sort_by_key(|(order, _)| *order);
                        *ticks = ordered.into_iter().map(|(_, tick)| tick).collect();
                    }
                }
                for (index, tick) in list_mut(value)?.iter_mut().enumerate() {
                    let tick = map_mut(tick)?;
                    let id = text(tick, field)?;
                    let renamed =
                        context.rename(if field == "Block" { "block" } else { "fluid" }, &id)?;
                    if field == "Block" {
                        context
                            .target
                            .resolve(&Block::parse(&renamed)?)
                            .map_err(|e| format!("{key}[{index}]: {e}"))?;
                    } else if !matches!(
                        renamed.as_str(),
                        "minecraft:water"
                            | "minecraft:flowing_water"
                            | "minecraft:lava"
                            | "minecraft:flowing_lava"
                            | "minecraft:empty"
                    ) {
                        return Err(format!("{key}[{index}]: unresolved fluid {renamed}"));
                    }
                    tick.insert(field.into(), V::String(renamed));
                }
            }
            _ => {
                return Err(format!(
                    "version conversion of retained {key} is not implemented"
                ));
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct BiomeRename {
    data_version: i32,
    from: String,
    to: String,
}

pub(super) async fn prepare(schematic: &Schematic, context: &Context) -> Result<()> {
    if schematic
        .regions
        .values()
        .any(|region| region.retained.spatial.contains_key("Biomes"))
    {
        schematic.data.load_biomes(&context.source_registry).await?;
        schematic.data.load_biomes(&context.target).await?;
    }
    Ok(())
}

fn biome(id: &str, context: &Context) -> Result<String> {
    static RULES: OnceLock<Vec<BiomeRename>> = OnceLock::new();
    let rules =
        RULES.get_or_init(|| serde_json::from_str(include_str!("data/biomes.json")).unwrap());
    context.source_registry.biome(id)?;
    let mut result = id.to_string();
    if context.source < context.target.data_version {
        for rule in rules {
            if context.source < rule.data_version
                && rule.data_version <= context.target.data_version
                && result == rule.from
            {
                if rule.data_version == 2838
                    && (rules.iter().filter(|other| other.to == rule.to).count() > 1
                        || context.source_registry.biome(&rule.to).is_ok())
                {
                    context.loss(
                        &format!("Biomes.Palette.{id}"),
                        "biome identity merges with other source biomes",
                    );
                }
                result = rule.to.clone();
            }
        }
    } else if context.target.biome(&result).is_err() {
        let candidates: Vec<_> = rules
            .iter()
            .filter(|rule| {
                context.target.data_version < rule.data_version
                    && rule.data_version <= context.source
                    && rule.to == result
                    && context.target.biome(&rule.from).is_ok()
            })
            .collect();
        match candidates.as_slice() {
            [rule] => result = rule.from.clone(),
            [] => {}
            _ => return Err(format!("Biomes: ambiguous inverse for {id}")),
        }
    }
    context.target.biome(&result)?;
    Ok(result)
}

fn biomes(value: &mut V, volume: usize, context: &Context) -> Result<()> {
    let container = map_mut(value)?;
    let V::Compound(palette) = container
        .get("Palette")
        .ok_or("Biomes.Palette is missing")?
    else {
        return Err("Biomes.Palette must be a compound".into());
    };
    let mut translated = Compound::new();
    let mut indices = std::collections::BTreeMap::new();
    for (id, index) in palette {
        let V::Int(index) = index else {
            return Err("Biome palette indices must be ints".into());
        };
        if *index < 0 || indices.contains_key(index) {
            return Err("Biome palette indices must be unique and nonnegative".into());
        }
        let id = if id.contains(':') {
            id.clone()
        } else {
            format!("minecraft:{id}")
        };
        let target = biome(&id, context)?;
        let entry = translated.entry(target).or_insert(V::Int(*index));
        let V::Int(target_index) = entry else {
            unreachable!()
        };
        indices.insert(*index, *target_index as u32);
    }
    let V::ByteArray(data) = container.get("Data").ok_or("Biomes.Data is missing")? else {
        return Err("Biomes.Data must be a byte array".into());
    };
    let mut output = Vec::with_capacity(data.len());
    let mut cursor = 0;
    let mut count = 0;
    while cursor < data.len() {
        let mut index = 0u32;
        for part in 0..5 {
            let byte = *data.get(cursor).ok_or("Truncated biome varint")? as u8;
            cursor += 1;
            if part == 4 && byte > 7 {
                return Err("Biome index varint exceeds signed int range".into());
            }
            index |= u32::from(byte & 127) << (part * 7);
            if byte & 128 == 0 {
                break;
            }
        }
        let mut target = *indices
            .get(&(index as i32))
            .ok_or_else(|| format!("Missing biome palette index {index}"))?;
        loop {
            let byte = (target & 127) as u8;
            target >>= 7;
            output.push((if target == 0 { byte } else { byte | 128 }) as i8);
            if target == 0 {
                break;
            }
        }
        count += 1;
        if count > volume {
            return Err("Biomes.Data exceeds region volume".into());
        }
    }
    if count != volume {
        return Err("Biomes.Data does not match region volume".into());
    }
    container.insert("Palette".into(), V::Compound(translated));
    container.insert("Data".into(), V::ByteArray(fastnbt::ByteArray::new(output)));
    Ok(())
}
