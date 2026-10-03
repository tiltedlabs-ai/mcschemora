use super::common::*;
use crate::{
    Result,
    model::*,
    nbt::{Tag as V, *},
};

pub(super) fn read_structure(root: &Compound, schematic: &mut Schematic) -> Result<()> {
    version(
        schematic,
        root.get("DataVersion")
            .map(number)
            .transpose()?
            .unwrap_or(0),
    )?;
    let mut r = Region::new([0; 3]);
    r.bounds = Bounds::new([0; 3], xyz(get(root, "size")?)?)?;
    let pal=if let Some(v)=root.get("palette"){list(v)?}else{
        let variants=list(get(root,"palettes")?)?;
        if variants.len()!=1{return Err("Multiple structure palettes need an explicit variant choice; import one variant first".into());}
        list(&variants[0])?
    }.iter().map(|value| tag_block(value, schematic.data_version)).collect::<Result<Vec<_>>>()?;
    let mut present = std::collections::BTreeSet::new();
    for v in list(get(root, "blocks")?)? {
        let c = compound(v)?;
        let p = xyz(get(c, "pos")?)?;
        if !r.bounds.contains(p) {
            return Err("Structure block outside bounds".into());
        }
        let idx = number(get(c, "state")?)?;
        let b = pal
            .get(usize::try_from(idx).map_err(|_| "Negative palette index")?)
            .ok_or("Missing structure palette entry")?
            .clone();
        if !present.insert(p) {
            return Err("Duplicate structure block coordinate".into());
        }
        if b != Block::air() {
            r.blocks.set(p, &b);
        }
        if let Some(data) = c.get("nbt") {
            r.block_entities.insert(p, compound(data)?.clone());
        }
    }
    r.present = Some(present);
    read_entities(root, "entities", &mut r, schematic, "nbt")?;
    schematic.regions.insert("main".into(), r);
    Ok(())
}

pub(super) fn write_structure(schematic: &Schematic, r: &Region) -> Result<Compound> {
    let (pal, ids) = palette(r);
    let mut blocks = vec![];
    let sparse = r.present.as_ref();
    for p in r.bounds.positions() {
        if sparse.is_some_and(|s| !s.contains(&p)) && !r.blocks.contains_key(&p) {
            continue;
        }
        let q = std::array::from_fn(|i| p[i] - r.bounds.start[i]);
        let mut b = Compound::from([
            ("pos".into(), ints(q)),
            ("state".into(), V::Int(ids[&r.get(p)] as i32)),
        ]);
        if let Some(data) = r.block_entities.get(&p) {
            b.insert("nbt".into(), V::Compound(data.clone()));
        }
        blocks.push(V::Compound(b));
    }
    Ok(Compound::from([
        ("DataVersion".into(), V::Int(schematic.data_version)),
        ("size".into(), ints(r.bounds.size)),
        (
            "palette".into(),
            V::List(
                pal.iter()
                    .map(|block| block_tag(block, schematic.data_version))
                    .collect(),
            ),
        ),
        ("blocks".into(), V::List(blocks)),
        ("entities".into(), entities(r, r.bounds.start, "nbt")),
    ]))
}
