use super::common::*;
use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};
use fastnbt::LongArray;

pub(super) fn read_litematic(root: &Compound, doc: &mut Document) -> Result<()> {
    let v = number(get(root, "Version")?)?;
    if !(4..=6).contains(&v) {
        return Err(format!(
            "Supported Litematica versions are 4 through 6, got {v}"
        ));
    }
    version(doc, number(get(root, "MinecraftDataVersion")?)?);
    doc.metadata = root
        .get("Metadata")
        .map(compound)
        .transpose()?
        .cloned()
        .unwrap_or_default();
    for (name, v) in compound(get(root, "Regions")?)? {
        let c = compound(v)?;
        let position = xyz(get(c, "Position")?)?;
        let size = xyz(get(c, "Size")?)?;
        let mut abs = [0; 3];
        let mut start = [0; 3];
        for i in 0..3 {
            abs[i] = size[i].checked_abs().ok_or("Region size overflow")?;
            if size[i] < 0 {
                start[i] = size[i] + 1;
            }
        }
        let mut r = Region::new(position);
        r.bounds = Bounds::new(start, abs)?;
        let pal = list(get(c, "BlockStatePalette")?)?
            .iter()
            .map(tag_block)
            .collect::<Result<Vec<_>>>()?;
        if pal.is_empty() {
            return Err("Empty Litematica palette".into());
        }
        let bits = (usize::BITS - (pal.len() - 1).leading_zeros()).max(2) as usize;
        let longs = if let V::LongArray(a) = get(c, "BlockStates")? {
            a
        } else {
            return Err("Expected Litematica long array".into());
        };
        let needed = (r.bounds.volume()? * bits).div_ceil(64);
        if longs.len() < needed {
            return Err("Truncated Litematica block data".into());
        }
        for (index, p) in r.bounds.positions().enumerate() {
            let bit = index * bits;
            let i = bit / 64;
            let shift = bit % 64;
            let mut value = (longs[i] as u64) >> shift;
            if shift + bits > 64 {
                value |= (longs[i + 1] as u64) << (64 - shift);
            }
            let id = (value & ((1u64 << bits) - 1)) as usize;
            let b = pal
                .get(id)
                .ok_or("Invalid Litematica palette index")?
                .clone();
            if b != Block::air() {
                r.blocks.insert(p, b);
            }
        }
        read_block_entities(c, "TileEntities", &mut r, false)?;
        read_entities(c, "Entities", &mut r, doc, "litematic")?;
        let mut ticks = Compound::new();
        for k in ["PendingBlockTicks", "PendingFluidTicks"] {
            if let Some(v) = c.get(k)
                && !list(v)?.is_empty()
            {
                ticks.insert(k.into(), v.clone());
            }
        }
        r.retained.spatial = ticks;
        doc.regions.insert(name.clone(), r);
    }
    Ok(())
}

pub(super) fn write_litematic(doc: &Document) -> Result<Compound> {
    let mut regions = Compound::new();
    let mut volume = 0i64;
    let mut count = 0i64;
    let mut ends = vec![];
    for (name, r) in &doc.regions {
        let n = r.bounds.volume()?;
        let (pal, ids) = palette(r);
        let bits = (usize::BITS - (pal.len() - 1).leading_zeros()).max(2) as usize;
        let mut longs = vec![0u64; (n * bits).div_ceil(64)];
        for (index, p) in r.bounds.positions().enumerate() {
            let val = ids[&r.get(p)] as u64;
            let bit = index * bits;
            let a = bit / 64;
            let s = bit % 64;
            longs[a] |= val << s;
            if s + bits > 64 {
                longs[a + 1] |= val >> (64 - s);
            }
        }
        let position = std::array::from_fn(|i| r.origin[i] + r.bounds.start[i]);
        ends.push(position);
        ends.push(std::array::from_fn(|i| position[i] + r.bounds.size[i] - 1));
        let mut reg = Compound::from([
            ("Position".into(), pos_compound(position)),
            ("Size".into(), pos_compound(r.bounds.size)),
            (
                "BlockStatePalette".into(),
                V::List(pal.iter().map(block_tag).collect()),
            ),
            (
                "BlockStates".into(),
                V::LongArray(LongArray::new(
                    longs.into_iter().map(|n| n as i64).collect(),
                )),
            ),
            (
                "TileEntities".into(),
                block_entities(r, r.bounds.start, false),
            ),
            ("Entities".into(), entities(r, r.bounds.start, "litematic")),
        ]);
        for k in ["PendingBlockTicks", "PendingFluidTicks"] {
            let value = r.retained.spatial.get(k);
            reg.insert(k.into(), rebase_ticks(value, r.bounds.start)?);
        }
        volume += n as i64;
        count += r.blocks.values().filter(|b| !b.is_air()).count() as i64;
        regions.insert(name.clone(), V::Compound(reg));
    }
    let mut metadata = doc.metadata.clone();
    for (k, v) in [
        ("Name", s("Schemora build")),
        ("Author", s("")),
        ("Description", s("")),
        ("TimeCreated", V::Long(0)),
        ("TimeModified", V::Long(0)),
    ] {
        metadata.entry(k.into()).or_insert(v);
    }
    metadata.insert("RegionCount".into(), V::Int(doc.regions.len() as i32));
    metadata.insert(
        "TotalVolume".into(),
        V::Int(i32::try_from(volume).map_err(|_| "Litematica volume too large")?),
    );
    metadata.insert(
        "TotalBlocks".into(),
        V::Int(i32::try_from(count).map_err(|_| "Litematica block count too large")?),
    );
    metadata.insert(
        "EnclosingSize".into(),
        pos_compound(Bounds::around(ends.into_iter())?.size),
    );
    Ok(Compound::from([
        ("Version".into(), V::Int(6)),
        ("SubVersion".into(), V::Int(1)),
        ("MinecraftDataVersion".into(), V::Int(doc.data_version)),
        ("Metadata".into(), V::Compound(metadata)),
        ("Regions".into(), V::Compound(regions)),
    ]))
}

fn rebase_ticks(value: Option<&V>, start: Pos) -> Result<V> {
    let Some(value) = value else {
        return Ok(V::List(vec![]));
    };
    let mut ticks = list(value)?.clone();
    for value in &mut ticks {
        let V::Compound(tick) = value else {
            return Err("Tick must be a compound".into());
        };
        for (i, key) in ["x", "y", "z"].iter().enumerate() {
            let n = number(get(tick, key)?)?
                .checked_sub(start[i])
                .ok_or("Tick coordinate overflow")?;
            tick.insert((*key).into(), V::Int(n));
        }
    }
    Ok(V::List(ticks))
}
