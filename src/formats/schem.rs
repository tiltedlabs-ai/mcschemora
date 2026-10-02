use super::common::*;
use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};
use fastnbt::ByteArray;
use std::collections::HashMap;

pub(super) fn read_schem(root: &Compound, doc: &mut Schematic) -> Result<()> {
    let root = if let Some(v) = root.get("Schematic") {
        compound(v)?
    } else {
        root
    };
    let format = number(get(root, "Version")?)?;
    if !(1..=3).contains(&format) {
        return Err(format!("Unsupported Sponge version {format}"));
    }
    version(
        doc,
        root.get("DataVersion")
            .map(number)
            .transpose()?
            .unwrap_or(0),
    )?;
    doc.metadata = root
        .get("Metadata")
        .map(compound)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    let origin = root.get("Offset").map(xyz).transpose()?.unwrap_or([0; 3]);
    let mut r = Region::new(origin);
    r.bounds = Bounds::new([0; 3], dimensions(root)?)?;
    let block_data = if format == 3 {
        compound(get(root, "Blocks")?)?
    } else {
        root
    };
    let mut pal = HashMap::new();
    for (k, v) in compound(get(block_data, "Palette")?)? {
        let id = number(v)?;
        if id < 0 || pal.insert(id as usize, Block::parse(k)?).is_some() {
            return Err("Invalid duplicate palette index".into());
        }
    }
    let raw = bytes(get(
        block_data,
        if format == 3 { "Data" } else { "BlockData" },
    )?)?;
    let mut at = 0;
    for p in r.bounds.positions() {
        let mut n = 0u32;
        let mut shift = 0;
        loop {
            let b = *raw.get(at).ok_or("Truncated Sponge block data")?;
            at += 1;
            if shift == 28 && b > 15 {
                return Err("Invalid Sponge varint".into());
            }
            n |= ((b & 127) as u32) << shift;
            if b & 128 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err("Sponge varint overflow".into());
            }
        }
        let b = pal
            .get(&(n as usize))
            .ok_or_else(|| format!("Missing palette index {n}"))?
            .clone();
        if b != Block::air() {
            r.blocks.set(p, &b);
        }
    }
    if at != raw.len() {
        return Err("Unexpected extra Sponge block data".into());
    }
    read_block_entities(
        block_data,
        if format == 1 {
            "TileEntities"
        } else {
            "BlockEntities"
        },
        &mut r,
        true,
    )?;
    read_entities(root, "Entities", &mut r, doc, "schem")?;
    let mut biomes = Compound::new();
    for k in ["Biomes", "BiomeData", "BiomePalette", "BiomePaletteMax"] {
        if let Some(v) = root.get(k) {
            biomes.insert(k.into(), v.clone());
        }
    }
    if format < 3 && !biomes.is_empty() {
        doc.notices.push("Older Sponge biome encoding is retained but cannot be converted to v3 without an explicit mapping".into());
    }
    r.retained.spatial = biomes;
    doc.regions.insert("main".into(), r);
    Ok(())
}

pub(super) fn write_schem(doc: &Schematic, r: &Region) -> Result<Compound> {
    let (pal, ids) = palette(r);
    let mut data = vec![];
    for p in r.bounds.positions() {
        let mut n = ids[&r.get(p)] as u32;
        loop {
            let v = (n & 127) as u8;
            n >>= 7;
            data.push((if n > 0 { v | 128 } else { v }) as i8);
            if n == 0 {
                break;
            }
        }
    }
    let offset = std::array::from_fn(|i| r.origin[i] + r.bounds.start[i]);
    let blocks = c([
        (
            "Palette",
            V::Compound(
                pal.iter()
                    .enumerate()
                    .map(|(i, b)| (b.text(), V::Int(i as i32)))
                    .collect(),
            ),
        ),
        ("Data", V::ByteArray(ByteArray::new(data))),
        ("BlockEntities", block_entities(r, r.bounds.start, true)),
    ]);
    let mut root = Compound::from([
        ("Version".into(), V::Int(3)),
        ("DataVersion".into(), V::Int(doc.data_version)),
        ("Width".into(), V::Short(r.bounds.size[0] as i16)),
        ("Height".into(), V::Short(r.bounds.size[1] as i16)),
        ("Length".into(), V::Short(r.bounds.size[2] as i16)),
        ("Offset".into(), int_array(offset)),
        ("Blocks".into(), blocks),
        ("Entities".into(), entities(r, r.bounds.start, "schem")),
        ("Metadata".into(), V::Compound(doc.metadata.clone())),
    ]);
    if let Some(b) = r.retained.spatial.get("Biomes") {
        root.insert("Biomes".into(), b.clone());
    }
    Ok(Compound::from([("Schematic".into(), V::Compound(root))]))
}
