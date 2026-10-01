use schemora::{
    Result,
    model::{Block, Document},
};
use std::collections::{BTreeMap, HashMap};

pub fn build(document: &mut Document) -> Result<()> {
    let block = |name: &str, properties: &[(&str, &str)]| {
        Block::new(
            name,
            properties
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    };
    let stone = block("stone_bricks", &[])?;
    let glass = block("glass", &[])?;
    let floor = block("smooth_stone", &[])?;
    let trapdoor = block("oak_trapdoor", &[("open", "true"), ("facing", "north")])?;
    let hopper = block("hopper", &[("facing", "north")])?;
    let chest = block("chest", &[("facing", "north")])?;
    let wire = block("redstone_wire", &[("north", "side"), ("south", "side")])?;
    let dispenser = block("dispenser", &[("facing", "down")])?;
    let repeater = block("repeater", &[("facing", "south"), ("delay", "4")])?;
    let spawner = block("spawner", &[])?;
    let railing = block("oak_fence", &[("east", "true"), ("west", "true")])?;
    let mut cells = BTreeMap::new();
    let mut mobs = Vec::new();
    for x in -42..=42 {
        for z in -42..=42 {
            cells.insert([x, 0, z], stone.clone());
        }
    }
    for cx in [-24, 24] {
        for cz in [-24, 24] {
            for x in -1..=1 {
                for z in -1..=1 {
                    cells.insert([cx + x, 1, cz + z], hopper.clone());
                }
                cells.insert([cx + x, 1, cz - 2], chest.clone());
            }
            for y in 2..12 {
                for x in -2i32..=2 {
                    for z in -2i32..=2 {
                        if x.abs() == 2 || z.abs() == 2 {
                            cells.insert([cx + x, y, cz + z], glass.clone());
                        }
                    }
                }
            }
            for level in 0..20 {
                let y = 12 + level * 4;
                for x in -15i32..=15 {
                    for z in -15i32..=15 {
                        if x.abs() > 1 || z.abs() > 1 {
                            cells.insert([cx + x, y, cz + z], floor.clone());
                        }
                        if x.abs() == 15 || z.abs() == 15 {
                            for dy in 1..=3 {
                                cells.insert([cx + x, y + dy, cz + z], stone.clone());
                            }
                        }
                    }
                }
                for x in -1..=1 {
                    cells.insert([cx + x, y + 1, cz - 2], trapdoor.clone());
                }
                cells.insert([cx, y + 3, cz + 12], dispenser.clone());
                cells.insert([cx + 6, y + 1, cz + 6], spawner.clone());
                for (i, name) in ["zombie", "skeleton", "creeper", "spider"]
                    .iter()
                    .enumerate()
                {
                    mobs.push((
                        *name,
                        [
                            f64::from(cx) + if i % 2 == 0 { -7.5 } else { 7.5 },
                            f64::from(y) + 1.,
                            f64::from(cz) + if i < 2 { -7.5 } else { 7.5 },
                        ],
                    ));
                }
            }
            for x in -15..=15 {
                for z in -15..=15 {
                    cells.insert([cx + x, 92, cz + z], stone.clone());
                }
                cells.insert([cx + x, 93, cz - 15], railing.clone());
            }
            for z in -12..=12 {
                cells.insert(
                    [cx, 93, cz + z],
                    if z % 6 == 0 {
                        repeater.clone()
                    } else {
                        wire.clone()
                    },
                );
            }
        }
    }
    document.set_blocks("main", cells)?;
    for (name, at) in mobs {
        document.add_entity(
            "main",
            at,
            HashMap::from([(
                "id".into(),
                fastnbt::Value::String(format!("minecraft:{name}")),
            )]),
        )?;
    }
    Ok(())
}
