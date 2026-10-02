use crate::model::Block;

pub(crate) struct Sprite {
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

pub(crate) fn resolve(block: &Block, display_name: &str) -> Sprite {
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
