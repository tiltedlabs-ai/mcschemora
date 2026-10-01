use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};
use std::collections::BTreeMap;

pub(super) fn block_tag(b: &Block) -> V {
    let mut c = Compound::from([("Name".into(), s(&b.name))]);
    if !b.properties.is_empty() {
        c.insert(
            "Properties".into(),
            V::Compound(
                b.properties
                    .iter()
                    .map(|(k, v)| (k.clone(), s(v)))
                    .collect(),
            ),
        );
    }
    V::Compound(c)
}

pub(super) fn tag_block(v: &V) -> Result<Block> {
    let c = compound(v)?;
    let mut props = BTreeMap::new();
    if let Some(v) = c.get("Properties") {
        for (k, v) in compound(v)? {
            props.insert(k.clone(), string(v)?.into());
        }
    }
    Block::new(string(get(c, "Name")?)?, props)
}

pub(super) fn palette(r: &Region) -> (Vec<Block>, BTreeMap<Block, usize>) {
    let mut blocks = vec![Block::air()];
    let mut ids = BTreeMap::from([(Block::air(), 0)]);
    for b in r.blocks.states() {
        if !ids.contains_key(b) {
            let n = blocks.len();
            ids.insert(b.clone(), n);
            blocks.push(b.clone());
        }
    }
    (blocks, ids)
}

pub(super) fn bytes(v: &V) -> Result<Vec<u8>> {
    if let V::ByteArray(a) = v {
        Ok(a.iter().map(|&v| v as u8).collect())
    } else {
        Err("Expected NBT byte array".into())
    }
}

pub(super) fn block_entities(r: &Region, start: Pos, wrapped: bool) -> V {
    V::List(
        r.block_entities
            .iter()
            .map(|(&p, data)| {
                let p = std::array::from_fn(|i| p[i] - start[i]);
                let mut data = data.clone();
                if wrapped {
                    let id = data.remove("id").unwrap_or_else(|| s("minecraft:unknown"));
                    data.remove("x");
                    data.remove("y");
                    data.remove("z");
                    c([
                        ("Pos", int_array(p)),
                        ("Id", id),
                        ("Data", V::Compound(data)),
                    ])
                } else {
                    for (k, v) in ["x", "y", "z"].into_iter().zip(p) {
                        data.insert(k.into(), V::Int(v));
                    }
                    V::Compound(data)
                }
            })
            .collect(),
    )
}

pub(super) fn entities(r: &Region, start: Pos, kind: &str) -> V {
    V::List(
        r.entities
            .iter()
            .map(|e| {
                let p = std::array::from_fn(|i| e.position[i] - start[i] as f64);
                let mut data = e.data.clone();
                if kind == "schem" {
                    let id = data.remove("id").unwrap_or_else(|| s("minecraft:pig"));
                    data.remove("Pos");
                    c([
                        ("Pos", double_list(p)),
                        ("Id", id),
                        ("Data", V::Compound(data)),
                    ])
                } else if kind == "nbt" {
                    data.insert("Pos".into(), double_list(p));
                    c([
                        ("pos", double_list(p)),
                        ("blockPos", ints(p.map(|n| n.floor() as i32))),
                        ("nbt", V::Compound(data)),
                    ])
                } else {
                    let position = if kind == "mcstructure" && e.position_float {
                        V::List(p.into_iter().map(|n| V::Float(n as f32)).collect())
                    } else {
                        double_list(p)
                    };
                    data.insert("Pos".into(), position);
                    V::Compound(data)
                }
            })
            .collect(),
    )
}

pub(super) fn read_entities(
    root: &Compound,
    key: &str,
    r: &mut Region,
    doc: &mut Document,
    kind: &str,
) -> Result<()> {
    if let Some(value) = root.get(key) {
        for v in list(value)? {
            let e = compound(v)?;
            let (p, mut data) = if kind == "schem" {
                let mut d = if let Some(v) = e.get("Data") {
                    compound(v)?.clone()
                } else {
                    e.clone()
                };
                d.insert("id".into(), get(e, "Id")?.clone());
                (doubles(get(e, "Pos")?)?, d)
            } else if kind == "nbt" {
                (doubles(get(e, "pos")?)?, compound(get(e, "nbt")?)?.clone())
            } else {
                (doubles(get(e, "Pos")?)?, e.clone())
            };
            data.remove("Pos");
            r.entities.push(Entity {
                reference: doc.next_entity,
                position_float: matches!(e.get("Pos"), Some(V::List(v)) if matches!(v.first(), Some(V::Float(_)))),
                position: p,
                data,
            });
            doc.next_entity += 1;
        }
    }
    Ok(())
}

pub(super) fn read_block_entities(
    root: &Compound,
    key: &str,
    r: &mut Region,
    wrapped: bool,
) -> Result<()> {
    if let Some(v) = root.get(key) {
        for v in list(v)? {
            let c = compound(v)?;
            let (p, mut data) = if wrapped {
                let mut data = if let Some(v) = c.get("Data") {
                    compound(v)?.clone()
                } else {
                    c.clone()
                };
                data.insert("id".into(), get(c, "Id")?.clone());
                (xyz(get(c, "Pos")?)?, data)
            } else {
                (xyz(v)?, c.clone())
            };
            for k in ["Pos", "Id", "x", "y", "z"] {
                data.remove(k);
            }
            if !r.bounds.contains(p) {
                return Err(format!("Block entity at {p:?} is outside bounds"));
            }
            r.block_entities.insert(p, data);
        }
    }
    Ok(())
}

pub(super) fn version(doc: &mut Document, n: i32) -> Result<()> {
    doc.data_version = n;
    doc.version = doc
        .data
        .version_for_data_version(n)?
        .unwrap_or_else(|| format!("data:{n}"));
    Ok(())
}

pub(super) fn dimensions(c: &Compound) -> Result<Pos> {
    let mut p = [0; 3];
    for (i, key) in ["Width", "Height", "Length"].iter().enumerate() {
        let v = get(c, key)?;
        p[i] = if let V::Short(n) = v {
            *n as u16 as i32
        } else {
            number(v)?
        };
    }
    Ok(p)
}
