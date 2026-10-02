use crate::model::Block;

pub(super) struct Sprite {
    pub name: String,
    pub omitted: Vec<String>,
}

fn direction(value: &str, reverse: bool) -> &str {
    match (value, reverse) {
        ("north", false) | ("south", true) => "n",
        ("south", false) | ("north", true) => "s",
        ("east", false) | ("west", true) => "e",
        ("west", false) | ("east", true) => "w",
        ("up", false) | ("down", true) => "u",
        ("down", false) | ("up", true) => "d",
        _ => "",
    }
}

pub(super) fn resolve(block: &Block, display_name: &str) -> Sprite {
    let prop = |key: &str| block.properties.get(key).map_or("", String::as_str);
    let powered = if prop("powered") == "true" { "!" } else { "" };
    let facing = direction(prop("facing"), false);
    let mut covered: Vec<&str> = Vec::new();
    let id = match block.name.as_str() {
        "minecraft:repeater" => {
            covered.extend(["facing", "delay", "locked", "powered"]);
            let prefix = if prop("locked") == "true" { "lr" } else { "rr" };
            Some(format!(
                "{prefix}-{}{}{powered}",
                direction(prop("facing"), true),
                prop("delay")
            ))
        }
        "minecraft:comparator" => {
            covered.extend(["facing", "mode", "powered"]);
            let prefix = if prop("mode") == "subtract" {
                "rs"
            } else {
                "rc"
            };
            Some(format!(
                "{prefix}-{}{powered}",
                direction(prop("facing"), true)
            ))
        }
        "minecraft:redstone_wire" => {
            let mut sides = String::new();
            for key in ["north", "south", "east", "west"] {
                if prop(key) != "none" {
                    sides.push_str(direction(key, false));
                }
                if prop(key) != "up" {
                    covered.push(key);
                }
            }
            if sides.len() == 1 {
                covered.clear();
                sides = if sides == "n" || sides == "s" {
                    "ns"
                } else {
                    "ew"
                }
                .into();
            }
            let lit = prop("power") != "0";
            if !lit {
                covered.push("power");
            }
            let suffix = if lit { "!" } else { "" };
            Some(if sides.is_empty() && !lit {
                "rd".into()
            } else {
                format!("rd-{sides}{suffix}")
            })
        }
        "minecraft:piston" | "minecraft:sticky_piston" => {
            covered.extend(["facing", "extended"]);
            let prefix = if block.name == "minecraft:piston" {
                "pi"
            } else {
                "sp"
            };
            let extended = if prop("extended") == "true" { "!" } else { "" };
            Some(format!("{prefix}-{facing}{extended}"))
        }
        "minecraft:piston_head" => {
            covered.extend(["facing", "type"]);
            let prefix = if prop("type") == "sticky" { "se" } else { "pe" };
            Some(format!("{prefix}-{facing}"))
        }
        "minecraft:observer" | "minecraft:dispenser" | "minecraft:dropper" | "minecraft:hopper" => {
            covered.push("facing");
            let prefix = match block.name.as_str() {
                "minecraft:observer" => "obs",
                "minecraft:dispenser" => "Di",
                "minecraft:dropper" => "Dr",
                _ => "ho",
            };
            let facing = direction(prop("facing"), block.name == "minecraft:observer");
            Some(format!("{prefix}-{facing}"))
        }
        "minecraft:redstone_torch" | "minecraft:redstone_wall_torch" => {
            covered.extend(["facing", "lit"]);
            let lit = if prop("lit") == "true" { "!" } else { "" };
            Some(if facing.is_empty() && lit.is_empty() {
                "rt".into()
            } else {
                format!("rt-{facing}{lit}")
            })
        }
        "minecraft:redstone_lamp" => {
            covered.push("lit");
            Some(if prop("lit") == "true" { "RL-!" } else { "RL" }.into())
        }
        _ => None,
    };
    let name = id.map_or_else(
        || {
            format!(
                "BlockSprite:{}",
                display_name.to_lowercase().replace([' ', '+'], "-")
            )
        },
        |id| format!("SchematicSprite:{id}"),
    );
    Sprite {
        name,
        omitted: block
            .properties
            .keys()
            .filter(|key| !covered.contains(&key.as_str()))
            .cloned()
            .collect(),
    }
}

pub(super) struct Imported {
    pub block: Block,
    pub defaulted: Vec<String>,
}

pub(super) fn reverse(
    catalog: &crate::registry::Registry,
    requested: &std::collections::BTreeSet<String>,
) -> crate::Result<std::collections::BTreeMap<String, Option<Imported>>> {
    let mut result = std::collections::BTreeMap::new();
    for id in catalog.block_names() {
        let display_name = catalog.block_display_name(id)?;
        let default = catalog.resolve(&Block::parse(id)?)?;
        let ordinary = format!(
            "BlockSprite:{}",
            display_name.to_lowercase().replace([' ', '+'], "-")
        );
        if requested.contains(&ordinary) {
            let candidate = Imported {
                defaulted: default
                    .properties
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect(),
                block: default.clone(),
            };
            result
                .entry(ordinary)
                .and_modify(|entry| *entry = None)
                .or_insert(Some(candidate));
        }
        if resolve(&default, display_name)
            .name
            .starts_with("BlockSprite:")
        {
            continue;
        }
        for state in catalog.block_states(id)? {
            let sprite = resolve(&state, display_name);
            if !requested.contains(&sprite.name) {
                continue;
            }
            let mut defaulted = Vec::new();
            if id == "minecraft:redstone_wire" {
                let connections = ["north", "south", "east", "west"];
                if connections.iter().any(|key| state.properties[*key] == "up")
                    || connections
                        .iter()
                        .filter(|key| state.properties[**key] == "side")
                        .count()
                        == 1
                    || !matches!(state.properties["power"].as_str(), "0" | "15")
                {
                    continue;
                }
                if state.properties["power"] == "15" {
                    defaulted.push("power=15".into());
                }
                for key in connections {
                    if state.properties[key] == "side" {
                        defaulted.push(format!("{key}=side (connection elevation unspecified)"));
                    }
                }
            } else {
                if sprite
                    .omitted
                    .iter()
                    .any(|key| state.properties[key] != default.properties[key])
                {
                    continue;
                }
                defaulted.extend(
                    sprite
                        .omitted
                        .iter()
                        .map(|key| format!("{key}={}", state.properties[key])),
                );
            }
            result
                .entry(sprite.name)
                .and_modify(|entry| *entry = None)
                .or_insert(Some(Imported {
                    block: state,
                    defaulted,
                }));
        }
    }
    Ok(result)
}

pub(super) fn normalize(value: &str, sheet: &str) -> String {
    let value = value.trim().trim_start_matches('+');
    let value = value.split_once('?').map_or(value, |(sprite, _)| sprite);
    let reference = if value.contains(':') {
        value.into()
    } else {
        format!("{sheet}:{value}")
    };
    let Some(id) = reference.strip_prefix("SchematicSprite:") else {
        return reference;
    };
    let canonical = match id {
        "redstone dust" => "rd",
        "redstone torch" | "rt-d" => "rt",
        "rt-d!" => "rt-!",
        "redstone repeater" => "rr-e1",
        "locked repeater" => "lr-e1",
        "redstone comparator" => "rc-e",
        "redstone subtractor" => "rs-e",
        "piston" => "pi-e",
        "sticky piston" => "sp-e",
        "piston extension" => "pe-e",
        "hopper" | "ho" => "ho-d",
        "dropper" => "Dr-e",
        "redstone lamp" => "RL",
        _ => return reference,
    };
    format!("SchematicSprite:{canonical}")
}
