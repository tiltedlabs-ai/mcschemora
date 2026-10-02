use crate::{model::Block, sprite_ids::resolve};

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
