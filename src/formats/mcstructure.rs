use super::common::*;
use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) const BEDROCK_SIMPLE: [&str; 13] = [
    "air",
    "stone",
    "cobblestone",
    "glass",
    "bedrock",
    "obsidian",
    "gold_block",
    "iron_block",
    "diamond_block",
    "emerald_block",
    "redstone_block",
    "slime",
    "coal_block",
];

fn positions(size: Pos) -> impl Iterator<Item = Pos> {
    (0..size[0])
        .flat_map(move |x| (0..size[1]).flat_map(move |y| (0..size[2]).map(move |z| [x, y, z])))
}
fn index(p: Pos, size: Pos) -> usize {
    ((p[0] * size[1] + p[1]) * size[2] + p[2]) as usize
}

pub(super) fn read_bedrock(root: &Compound, doc: &mut Document) -> Result<()> {
    if number(get(root, "format_version")?)? != 1 {
        return Err("Unsupported mcstructure format version".into());
    }
    doc.edition = "bedrock".into();
    doc.version = "imported".into();
    doc.data_version = 0;
    let origin = xyz(get(root, "structure_world_origin")?)?;
    let mut r = Region::new(origin);
    r.bounds = Bounds::new([0; 3], xyz(get(root, "size")?)?)?;
    let structure = compound(get(root, "structure")?)?;
    let p = compound(get(compound(get(structure, "palette")?)?, "default")?)?;
    let mut order = vec![];
    let mut palette = BTreeMap::new();
    for v in list(get(p, "block_palette")?)? {
        let c = compound(v)?;
        let props = compound(get(c, "states")?)?
            .iter()
            .map(|(k, v)| {
                let value = match v {
                    V::String(s) => s.clone(),
                    _ => number(v)?.to_string(),
                };
                Ok((k.clone(), value))
            })
            .collect::<Result<_>>()?;
        let b = Block::new(string(get(c, "name")?)?, props)?;
        palette.insert(b.clone(), c.clone());
        order.push(b);
    }
    let layers = list(get(structure, "block_indices")?)?;
    if layers.len() != 2 {
        return Err("mcstructure needs two block index layers".into());
    }
    let primary = list(&layers[0])?
        .iter()
        .map(number)
        .collect::<Result<Vec<_>>>()?;
    let secondary = list(&layers[1])?
        .iter()
        .map(number)
        .collect::<Result<Vec<_>>>()?;
    let n = r.bounds.volume()?;
    if primary.len() != n || secondary.len() != n {
        return Err("mcstructure index count does not match size".into());
    }
    if secondary.iter().any(|&i| i < -1 || i >= order.len() as i32) {
        return Err("Invalid secondary palette index".into());
    }
    r.present = primary.contains(&-1).then(BTreeSet::new);
    for (p, id) in positions(r.bounds.size).zip(primary) {
        if id == -1 {
            continue;
        }
        let b = order
            .get(usize::try_from(id).map_err(|_| "Invalid negative Bedrock index")?)
            .ok_or("Missing Bedrock palette entry")?;
        if let Some(present) = &mut r.present {
            present.insert(p);
        }
        if b != &Block::air() {
            r.blocks.set(p, b);
        }
    }
    let mut position_data = BTreeMap::new();
    if let Some(v) = p.get("block_position_data") {
        for (k, v) in compound(v)? {
            let idx = k
                .parse::<usize>()
                .map_err(|_| "Invalid Bedrock position index")?;
            if idx >= n {
                return Err("Bedrock position data outside volume".into());
            }
            let yz = (r.bounds.size[1] * r.bounds.size[2]) as usize;
            let at = [
                (idx / yz) as i32,
                ((idx % yz) / r.bounds.size[2] as usize) as i32,
                (idx % r.bounds.size[2] as usize) as i32,
            ];
            let mut fields = compound(v)?.clone();
            if let Some(data) = fields.remove("block_entity_data") {
                r.block_entities.insert(at, compound(&data)?.clone());
            }
            if !fields.is_empty() {
                position_data.insert(at, fields);
            }
        }
    }
    r.retained.bedrock = Some(BedrockData {
        palette,
        order,
        secondary,
        size: r.bounds.size,
        position_data,
    });
    read_entities(structure, "entities", &mut r, doc, "bedrock")?;
    for e in &mut r.entities {
        for (i, coordinate) in origin.iter().enumerate() {
            e.position[i] -= *coordinate as f64;
        }
    }
    doc.regions.insert("main".into(), r);
    Ok(())
}

pub(super) fn palette_entry(b: &Block, original: Option<&BedrockData>) -> Result<V> {
    if let Some(raw) = original.and_then(|data| data.palette.get(b)) {
        return Ok(V::Compound(raw.clone()));
    }
    if BEDROCK_SIMPLE.contains(&b.name.trim_start_matches("minecraft:")) && b.properties.is_empty()
    {
        return Ok(c([
            ("name", s(&b.name)),
            ("states", V::Compound(Compound::new())),
            ("version", V::Int(18153472)),
        ]));
    }
    Err(format!(
        "No Bedrock mapping for {}; supply an explicit replacement",
        b.text()
    ))
}

pub(super) fn write_bedrock(doc: &Document, r: &Region) -> Result<Compound> {
    let n = r.bounds.volume()?;
    let original = r.retained.bedrock.as_ref();
    let (pal, mut ids) = palette(r);
    let mut out_pal = pal
        .iter()
        .map(|b| palette_entry(b, original))
        .collect::<Result<Vec<_>>>()?;
    let primary = positions(r.bounds.size)
        .map(|p| {
            let at = std::array::from_fn(|i| p[i] + r.bounds.start[i]);
            V::Int(
                if r.present.as_ref().is_some_and(|cells| !cells.contains(&at))
                    && !r.blocks.contains_key(&at)
                {
                    -1
                } else {
                    ids[&r.get(at)] as i32
                },
            )
        })
        .collect();
    let mut secondary = vec![V::Int(-1); n];
    let mut position_data = BTreeMap::new();
    if let Some(data) = original {
        for (dest, &i) in secondary.iter_mut().zip(&data.secondary) {
            if i == -1 {
                continue;
            }
            let b = data
                .order
                .get(i as usize)
                .ok_or("Invalid secondary index")?;
            let id = if let Some(id) = ids.get(b) {
                *id
            } else {
                let id = out_pal.len();
                out_pal.push(palette_entry(b, original)?);
                ids.insert(b.clone(), id);
                id
            };
            *dest = V::Int(id as i32);
        }
        position_data = data.position_data.clone();
        for (p, data) in &r.block_entities {
            position_data
                .entry(*p)
                .or_default()
                .insert("block_entity_data".into(), V::Compound(data.clone()));
        }
    }
    let origin = std::array::from_fn(|i| r.origin[i] + r.bounds.start[i]);
    let entity_list = if doc.edition == "bedrock" {
        entities(r, origin.map(|v| -v), "mcstructure")
    } else {
        V::List(vec![])
    };
    Ok(Compound::from([
        ("format_version".into(), V::Int(1)),
        ("size".into(), ints(r.bounds.size)),
        ("structure_world_origin".into(), ints(origin)),
        (
            "structure".into(),
            c([
                (
                    "block_indices",
                    V::List(vec![V::List(primary), V::List(secondary)]),
                ),
                ("entities", entity_list),
                (
                    "palette",
                    c([(
                        "default",
                        c([
                            ("block_palette", V::List(out_pal)),
                            (
                                "block_position_data",
                                V::Compound(
                                    position_data
                                        .into_iter()
                                        .map(|(p, fields)| {
                                            (
                                                index(p, r.bounds.size).to_string(),
                                                V::Compound(fields),
                                            )
                                        })
                                        .collect(),
                                ),
                            ),
                        ]),
                    )]),
                ),
            ]),
        ),
    ]))
}
