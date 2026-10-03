use super::*;

pub(super) fn state(source: &Block, context: &Context) -> Result<Block> {
    let mut block = context.source_registry.resolve(source)?;
    block.id = context.rename("block", &block.id)?;
    historical(&mut block, context)?;
    defaults(&mut block, context)?;
    let mut result = context.target.resolve(&block)?;
    if let Some(key) = result
        .properties
        .keys()
        .find(|key| !block.properties.contains_key(*key))
    {
        return Err(format!(
            "{}: property {key} requires an audited default migration",
            block.id
        ));
    }
    let defaults = context.target.resolve(&Block::parse(&result.id)?)?;
    result.properties.retain(|key, value| {
        source.properties.contains_key(key) || defaults.properties.get(key) != Some(value)
    });
    Ok(result)
}

pub(super) fn nbt(data: &mut Compound, context: &Context) -> Result<()> {
    let (source_name, source_properties) = if context.source >= 5006 {
        ("id", "properties")
    } else {
        ("Name", "Properties")
    };
    let (target_name, target_properties) = if context.target.data_version >= 5006 {
        ("id", "properties")
    } else {
        ("Name", "Properties")
    };
    let name = text(data, source_name)?;
    let mut properties = std::collections::BTreeMap::new();
    if let Some(value) = data.get(source_properties) {
        for (key, value) in crate::nbt::compound(value)? {
            properties.insert(key.clone(), crate::nbt::string(value)?.into());
        }
    }
    let block = state(&Block::new(&name, properties)?, context)?;
    if source_name != target_name
        && (data.contains_key(target_name) || data.contains_key(target_properties))
    {
        return Err("block state: source opaque fields collide with renamed target schema".into());
    }
    data.remove(source_name);
    data.remove(source_properties);
    data.insert(target_name.into(), V::String(block.id));
    if !block.properties.is_empty() {
        data.insert(
            target_properties.into(),
            V::Compound(
                block
                    .properties
                    .into_iter()
                    .map(|(key, value)| (key, V::String(value)))
                    .collect(),
            ),
        );
    }
    Ok(())
}

pub(super) fn constraint(source: &Block, context: &Context) -> Result<Block> {
    block_feature(&source.id, context.source)?;
    context.source_registry.resolve(source)?;
    let mut block = source.clone();
    block.id = context.rename("block", &block.id)?;
    if block.id == "minecraft:redstone_wire"
        && context.crosses(2531)
        && ["north", "south", "east", "west"]
            .iter()
            .any(|key| !block.properties.contains_key(*key))
        && !block.properties.is_empty()
    {
        return Err(
            "redstone_wire: partial connection constraints require placement context".into(),
        );
    }
    if block.id == "minecraft:cauldron"
        && context.crosses(2679)
        && context.forward()
        && !block.properties.contains_key("level")
    {
        return Err(
            "cauldron: unconstrained levels require separate empty and water predicates".into(),
        );
    }
    if block.id == "minecraft:water_cauldron"
        && context.crosses(2679)
        && !context.forward()
        && !block.properties.contains_key("level")
    {
        return Err(
            "water_cauldron: unconstrained levels require separate level predicates".into(),
        );
    }
    historical(&mut block, context)?;
    let explicit: std::collections::BTreeSet<_> = block.properties.keys().cloned().collect();
    defaults(&mut block, context)?;
    block.properties.retain(|key, _| explicit.contains(key));
    block_feature(&block.id, context.target.data_version)?;
    context.target.resolve(&block)?;
    Ok(block)
}

pub(super) fn item(
    value: &mut V,
    source_id: &str,
    target_id: &str,
    context: &Context,
) -> Result<()> {
    fn placed(id: &str) -> &str {
        match id {
            "minecraft:redstone" => "minecraft:redstone_wire",
            "minecraft:string" => "minecraft:tripwire",
            "minecraft:wheat_seeds" => "minecraft:wheat",
            "minecraft:beetroot_seeds" => "minecraft:beetroots",
            "minecraft:carrot" => "minecraft:carrots",
            "minecraft:potato" => "minecraft:potatoes",
            "minecraft:melon_seeds" => "minecraft:melon_stem",
            "minecraft:pumpkin_seeds" => "minecraft:pumpkin_stem",
            "minecraft:sweet_berries" => "minecraft:sweet_berry_bush",
            "minecraft:glow_berries" => "minecraft:cave_vines",
            "minecraft:cocoa_beans" => "minecraft:cocoa",
            _ => id,
        }
    }
    let fields = map_mut(value)?;
    if fields.is_empty() || context.source_registry.describe(placed(source_id)).is_err() {
        return Ok(());
    }
    let properties = fields
        .iter()
        .map(|(key, value)| {
            let value = match value {
                V::String(value) => value.clone(),
                _ if context.source < crate::versions::ITEM_COMPONENTS => {
                    crate::nbt::number(value)?.to_string()
                }
                _ => return Err(format!("block_state.{key}: expected string")),
            };
            Ok((key.clone(), value))
        })
        .collect::<Result<_>>()?;
    let block = constraint(&Block::new(placed(source_id), properties)?, context)?;
    if block.id != placed(target_id) {
        return Err(
            "block_state: these overrides require a different placed block identity".into(),
        );
    }
    *fields = block
        .properties
        .into_iter()
        .map(|(key, value)| (key, V::String(value)))
        .collect();
    Ok(())
}

fn historical(block: &mut Block, context: &Context) -> Result<()> {
    if block.id == "minecraft:jigsaw"
        && context.target.describe(&block.id)?["properties"]
            .get("orientation")
            .is_some()
        && let Some(facing) = block.properties.remove("facing")
    {
        let orientation = match facing.as_str() {
            "down" => "down_south",
            "up" => "up_north",
            "north" => "north_up",
            "south" => "south_up",
            "west" => "west_up",
            "east" => "east_up",
            _ => return Err(format!("jigsaw.facing: invalid direction {facing}")),
        };
        block
            .properties
            .insert("orientation".into(), orientation.into());
    }
    if block.id == "minecraft:jigsaw"
        && context.target.describe(&block.id)?["properties"]
            .get("facing")
            .is_some()
        && let Some(orientation) = block.properties.remove("orientation")
    {
        let facing = match orientation.as_str() {
            "down_south" => "down",
            "up_north" => "up",
            "north_up" => "north",
            "south_up" => "south",
            "west_up" => "west",
            "east_up" => "east",
            _ => {
                return Err(format!(
                    "jigsaw.orientation: {orientation} cannot be represented by a facing"
                ));
            }
        };
        block.properties.insert("facing".into(), facing.into());
    }
    if block.id == "minecraft:redstone_wire" && context.crosses(2531) {
        let none = |key: &str| block.properties.get(key).is_some_and(|v| v == "none");
        let ns = none("north") && none("south");
        let ew = none("east") && none("west");
        let mut converted = block.properties.clone();
        if ew {
            for key in ["north", "south"] {
                if none(key) {
                    converted.insert(key.into(), "side".into());
                }
            }
        }
        if ns {
            for key in ["east", "west"] {
                if none(key) {
                    converted.insert(key.into(), "side".into());
                }
            }
        }
        if context.forward() {
            block.properties = converted;
        } else if converted != block.properties {
            return Err(
                "redstone_wire: dot or single-ended connection cannot be represented before1.16"
                    .into(),
            );
        }
    }
    if context.crosses(2503) && block.id.ends_with("_wall") {
        for key in ["north", "south", "east", "west"] {
            if let Some(value) = block.properties.get_mut(key) {
                *value = match (context.forward(), value.as_str()) {
                    (true, "false") => "none",
                    (true, "true") => "low",
                    (false, "none") => "false",
                    (false, "low") => "true",
                    (false, "tall") => {
                        return Err(format!(
                            "{key}=tall: tall wall connections cannot be represented before1.16"
                        ));
                    }
                    _ => return Err(format!("{key}: invalid wall connection {value}")),
                }
                .into();
            }
        }
    }
    if context.crosses(4294) && block.id == "minecraft:creaking_heart" {
        if context.forward() {
            if let Some(value) = block.properties.remove("active") {
                let state = match value.as_str() {
                    "true" => "awake",
                    "false" => "uprooted",
                    _ => return Err("creaking_heart.active: invalid boolean".into()),
                };
                block
                    .properties
                    .insert("creaking_heart_state".into(), state.into());
            }
        } else if let Some(value) = block.properties.remove("creaking_heart_state") {
            let active = match value.as_str() {
                "awake" => "true",
                "uprooted" => "false",
                _ => {
                    return Err(
                        "creaking_heart_state: state cannot be represented before1.21.5".into(),
                    );
                }
            };
            block.properties.insert("active".into(), active.into());
        }
    }
    if context.crosses(2679) {
        if context.forward() && block.id == "minecraft:cauldron" {
            let level = block
                .properties
                .get("level")
                .ok_or("cauldron.level: missing source level")?;
            if level == "0" {
                block.properties.remove("level");
            } else {
                block.id = "minecraft:water_cauldron".into();
            }
        } else if !context.forward() && block.id == "minecraft:water_cauldron" {
            block.id = "minecraft:cauldron".into();
        } else if !context.forward() && block.id == "minecraft:cauldron" {
            block.properties.insert("level".into(), "0".into());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct AddedProperty {
    ids: Vec<String>,
    suffixes: Vec<String>,
    property: String,
    default: String,
}

fn defaults(block: &mut Block, context: &Context) -> Result<()> {
    static RULES: OnceLock<std::result::Result<Vec<AddedProperty>, String>> = OnceLock::new();
    let rules = RULES
        .get_or_init(|| {
            serde_json::from_str(include_str!("data/block_properties.json"))
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let target = context.target.describe(&block.id)?;
    for rule in rules {
        if !rule.ids.contains(&block.id)
            && !rule
                .suffixes
                .iter()
                .any(|suffix| block.id.ends_with(suffix))
        {
            continue;
        }
        let target_has = target["properties"].get(&rule.property).is_some();
        if target_has && !block.properties.contains_key(&rule.property) {
            block
                .properties
                .insert(rule.property.clone(), rule.default.clone());
        } else if !target_has
            && let Some(value) = block.properties.remove(&rule.property)
            && value != rule.default
        {
            return Err(format!(
                "{}: {}={} cannot be represented in Java{}",
                block.id, rule.property, value, context.target.version
            ));
        }
    }
    Ok(())
}
