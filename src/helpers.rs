//! Placement recipes and inventory, sign, and living-mob NBT.

use crate::versions::{COLORED_SIGNS, DUAL_SIDED_SIGNS, ITEM_COMPONENTS, NBT_TEXT_COMPONENTS};
use crate::{Result, catalog, model::*, nbt};
use fastnbt::Value;
use std::collections::BTreeMap;

/// Inventory item identifier, count, and optional typed SNBT components.
pub type Item = (String, i32, Option<String>);
#[derive(Clone)]
pub enum Placement {
    /// Two bed halves anchored at the foot block.
    Bed {
        /// Minecraft bed color.
        color: String,
        /// Horizontal direction from the foot to the head.
        head_toward: String,
    },
    /// Two door halves anchored at the lower block.
    Door {
        /// Minecraft door material, such as oak or iron.
        material: String,
        /// Horizontal facing direction.
        facing: String,
        /// Hinge side, left or right.
        hinge: String,
        /// Whether the door is open.
        open: bool,
        /// Whether the door is powered.
        powered: bool,
    },
    /// A single chest and inventory, with slots 0 through 26.
    Chest {
        /// Horizontal facing direction.
        facing: String,
        /// Slots mapped to item identifiers, counts, and optional components.
        items: BTreeMap<i8, Item>,
    },
    /// A standing sign with up to four lines of front text.
    Sign {
        /// Minecraft sign material, such as oak.
        material: String,
        /// Minecraft rotation value, 0 through 15.
        rotation: i32,
        /// Minecraft text color.
        color: String,
        /// Up to four plain-text lines; omitted lines are blank.
        lines: Vec<String>,
    },
}

fn make(catalog: &catalog::Registry, id: &str, props: Vec<(&str, String)>) -> Result<Block> {
    catalog.resolve(&Block::new(
        id,
        props.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )?)
}
/// Resolves and validates a placement into local cells and version-appropriate NBT.
///
/// No schematic is changed. at is the bed foot, lower door half, or single-block anchor.
pub fn resolve_placement(
    catalog: &catalog::Registry,
    placement: &Placement,
    at: Position,
) -> Result<Vec<(Position, Block, Option<Compound>)>> {
    let mut cells = vec![];
    match placement {
        Placement::Bed {
            color,
            head_toward: head,
        } => {
            let delta = direction(head)?;
            if delta[1] != 0 {
                return Err("A bed must face horizontally".into());
            }
            let mut end = at;
            for i in 0..3 {
                end[i] = at[i].checked_add(delta[i]).ok_or("Bed position overflow")?;
            }
            for (p, part) in [(at, "foot"), (end, "head")] {
                cells.push((
                    p,
                    make(
                        catalog,
                        &format!("{color}_bed"),
                        vec![("facing", head.clone()), ("part", part.into())],
                    )?,
                    None,
                ));
            }
        }
        Placement::Door {
            material,
            facing,
            hinge,
            open,
            powered,
        } => {
            let mut upper = at;
            upper[1] = at[1].checked_add(1).ok_or("Door position overflow")?;
            for (p, half) in [(at, "lower"), (upper, "upper")] {
                cells.push((
                    p,
                    make(
                        catalog,
                        &format!("{material}_door"),
                        vec![
                            ("facing", facing.clone()),
                            ("half", half.into()),
                            ("hinge", hinge.clone()),
                            ("open", open.to_string()),
                            ("powered", powered.to_string()),
                        ],
                    )?,
                    None,
                ));
            }
        }
        Placement::Chest {
            facing,
            items: inventory,
        } => {
            let b = make(catalog, "chest", vec![("facing", facing.clone())])?;
            let mut items = vec![];
            for (&slot, (id, count, components)) in inventory {
                if !(0..27).contains(&slot) {
                    return Err("Chest slot must be 0 through 26".into());
                }
                catalog.item(id)?;
                if !(1..=99).contains(count) {
                    return Err("Item count must be 1 through 99".into());
                }
                let mut n = Compound::from([
                    ("Slot".into(), Value::Byte(slot)),
                    ("id".into(), nbt::s(catalog::namespace(id))),
                    if catalog.data_version >= ITEM_COMPONENTS {
                        ("count".into(), Value::Int(*count))
                    } else {
                        ("Count".into(), Value::Byte(*count as i8))
                    },
                ]);
                if let Some(s) = components.as_deref() {
                    if catalog.data_version < ITEM_COMPONENTS {
                        return Err("Item components require Java 1.20.5+; use raw block-entity NBT for older item tags".into());
                    }
                    n.insert("components".into(), Value::Compound(nbt::from_snbt(s)?));
                }
                items.push(Value::Compound(n));
            }
            cells.push((
                at,
                b,
                Some(Compound::from([
                    ("id".into(), nbt::s("minecraft:chest")),
                    ("Items".into(), Value::List(items)),
                ])),
            ));
        }
        Placement::Sign {
            material,
            rotation,
            color,
            lines,
        } => {
            if !(0..16).contains(rotation) {
                return Err("Sign rotation must be 0 through 15".into());
            }
            if ![
                "white",
                "orange",
                "magenta",
                "light_blue",
                "yellow",
                "lime",
                "pink",
                "gray",
                "light_gray",
                "cyan",
                "purple",
                "blue",
                "brown",
                "green",
                "red",
                "black",
            ]
            .contains(&color.as_str())
            {
                return Err("Unknown sign text color".into());
            }
            let id = if catalog.data_version < COLORED_SIGNS {
                if material != "oak" {
                    return Err("This Minecraft version only has oak signs".into());
                }
                "sign".into()
            } else {
                format!("{material}_sign")
            };
            let b = make(catalog, &id, vec![("rotation", rotation.to_string())])?;
            if lines.len() > 4 {
                return Err("A sign has at most four lines".into());
            }
            let mut messages = vec![];
            for i in 0..4 {
                let text = lines.get(i).map(String::as_str).unwrap_or("");
                messages.push(if catalog.data_version >= NBT_TEXT_COMPONENTS {
                    nbt::c([("text", nbt::s(text))])
                } else {
                    nbt::s(serde_json::json!({"text":text}).to_string())
                });
            }
            let mut data = Compound::from([("id".into(), nbt::s("minecraft:sign"))]);
            if catalog.data_version >= DUAL_SIDED_SIGNS {
                data.insert(
                    "front_text".into(),
                    nbt::c([
                        ("messages", Value::List(messages)),
                        ("color", nbt::s(color)),
                        ("has_glowing_text", Value::Byte(0)),
                    ]),
                );
            } else {
                if catalog.data_version < COLORED_SIGNS && color != "black" {
                    return Err("Colored sign text requires Java 1.14+".into());
                }
                for (i, message) in messages.into_iter().enumerate() {
                    data.insert(format!("Text{}", i + 1), message);
                }
                if catalog.data_version >= COLORED_SIGNS {
                    data.insert("Color".into(), nbt::s(color));
                }
            }
            cells.push((at, b, Some(data)));
        }
    }
    Ok(cells)
}
/// Whether a block-entity identifier matches the owning block type.
pub fn compatible(id: &str, b: &Block) -> bool {
    crate::validate::block_entity_id(b) == Some(id.strip_prefix("minecraft:").unwrap_or(id))
}

/// Creates living-mob NBT after catalog validation.
///
/// Supplied id and persistence replace those in data; missing Rotation defaults to zero.
pub fn mob(
    catalog: &catalog::Registry,
    id: &str,
    persistent: bool,
    data: Option<&str>,
) -> Result<Compound> {
    let id = catalog.mob_id(id)?;
    let mut c = if let Some(s) = data {
        nbt::from_snbt(s)?
    } else {
        Compound::new()
    };
    c.insert("id".into(), nbt::s(id));
    c.insert("PersistenceRequired".into(), Value::Byte(persistent as i8));
    c.entry("Rotation".into())
        .or_insert(Value::List(vec![Value::Float(0.), Value::Float(0.)]));
    Ok(c)
}
