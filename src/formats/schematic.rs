use super::common::*;
use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};
use fastnbt::ByteArray;

pub(super) fn read_legacy(root: &Compound, doc: &mut Document) -> Result<()> {
    let catalog = doc.data.registry("1.13")?;
    doc.version = catalog.version.clone();
    doc.data_version = catalog.data_version;
    doc.catalog = Some(catalog.clone());
    let origin = ["WEOffsetX", "WEOffsetY", "WEOffsetZ"]
        .map(|k| root.get(k).map(number).transpose())
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let mut r = Region::new(std::array::from_fn(|i| origin[i].unwrap_or(0)));
    r.bounds = Bounds::new([0; 3], dimensions(root)?)?;
    let ids = bytes(get(root, "Blocks")?)?;
    let values = bytes(get(root, "Data")?)?;
    let high = root
        .get("AddBlocks")
        .map(bytes)
        .transpose()?
        .unwrap_or_default();
    if ids.len() != r.bounds.volume()? || values.len() != ids.len() {
        return Err("Invalid legacy block array lengths".into());
    }
    if !high.is_empty() && high.len() != ids.len().div_ceil(2) {
        return Err("Invalid AddBlocks length".into());
    }
    for (i, p) in r.bounds.positions().enumerate() {
        let upper = high
            .get(i / 2)
            .map(|v| if i % 2 == 0 { v & 15 } else { v >> 4 })
            .unwrap_or(0);
        let id = ids[i] as u16 + ((upper as u16) << 8);
        let data = values[i] & 15;
        let state = doc
            .data
            .legacy()?
            .get(&format!("{id}:{data}"))
            .ok_or_else(|| format!("Unmapped legacy block {id}:{data} at {p:?}"))?;
        let raw = Block::parse(state)?;
        let b = catalog.resolve(&raw).unwrap_or(raw);
        if b != Block::air() {
            r.blocks.set(p, &b);
        }
    }
    read_block_entities(root, "TileEntities", &mut r, false)?;
    read_entities(root, "Entities", &mut r, doc, "legacy")?;
    if !r.block_entities.is_empty() || !r.entities.is_empty() {
        doc.notices.push("Legacy entity NBT is retained in its original schema; cross-version upgrading is not implemented".into());
    }
    doc.regions.insert("main".into(), r);
    Ok(())
}

pub(super) fn write_legacy(doc: &Document, r: &Region) -> Result<Compound> {
    let mut ids = vec![];
    let mut data = vec![];
    let mut high = vec![0u8; r.bounds.volume()?.div_ceil(2)];
    for (i, p) in r.bounds.positions().enumerate() {
        let b = r.get(p);
        let (id, value) = doc.registry()?.legacy_pair(&b)?;
        ids.push(id as u8 as i8);
        data.push(value as i8);
        high[i / 2] |= ((id >> 8) as u8) << (if i % 2 == 0 { 0 } else { 4 });
    }
    let offset = std::array::from_fn::<_, 3, _>(|i| r.origin[i] + r.bounds.start[i]);
    let preserve = doc.source_format.as_deref() == Some("schematic");
    let mut root = Compound::from([
        ("Width".into(), V::Short(r.bounds.size[0] as i16)),
        ("Height".into(), V::Short(r.bounds.size[1] as i16)),
        ("Length".into(), V::Short(r.bounds.size[2] as i16)),
        ("Materials".into(), s("Alpha")),
        ("Blocks".into(), V::ByteArray(ByteArray::new(ids))),
        ("Data".into(), V::ByteArray(ByteArray::new(data))),
        (
            "TileEntities".into(),
            if preserve {
                block_entities(r, r.bounds.start, false)
            } else {
                V::List(vec![])
            },
        ),
        (
            "Entities".into(),
            if preserve {
                entities(r, r.bounds.start, "legacy")
            } else {
                V::List(vec![])
            },
        ),
    ]);
    if high.iter().any(|&n| n > 0) {
        root.insert(
            "AddBlocks".into(),
            V::ByteArray(ByteArray::new(high.into_iter().map(|n| n as i8).collect())),
        );
    }
    for (k, v) in ["WEOffsetX", "WEOffsetY", "WEOffsetZ"]
        .into_iter()
        .zip(offset)
    {
        root.insert(k.into(), V::Int(v));
    }
    Ok(root)
}
