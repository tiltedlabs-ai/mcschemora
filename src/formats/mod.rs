//! Structure file codecs. Lossy conversions require explicit acknowledgement.
use crate::{Result, model::*, nbt, registry::MinecraftData, transform::Transform};
use std::{borrow::Cow, collections::BTreeSet};

pub mod blueprint;
mod common;
mod litematic;
mod mcstructure;
mod schem;
mod schematic;
mod structure;

pub const FORMATS: [&str; 7] = [
    "blueprint",
    "schem",
    "litematic",
    "nbt",
    "snbt",
    "schematic",
    "mcstructure",
];

fn valid_format(f: &str) -> Result<()> {
    if FORMATS.contains(&f) {
        Ok(())
    } else {
        Err(format!(
            "Unknown format {f}; expected {}",
            FORMATS.join(", ")
        ))
    }
}

pub fn decode(
    data: &[u8],
    format: &str,
    source: std::sync::Arc<MinecraftData>,
    options: &blueprint::import::Options,
) -> Result<Document> {
    valid_format(format)?;
    if data.len() > nbt::MAX_BYTES {
        return Err("Input exceeds 256 MiB".into());
    }
    if format == "blueprint" {
        return blueprint::import::decode(data, source, options);
    }
    if options.version.is_some() || options.origin.is_some() || !options.palette.is_empty() {
        return Err(
            "version, origin, and palette import options are only supported for blueprint".into(),
        );
    }
    let root = if format == "snbt" {
        nbt::from_snbt(std::str::from_utf8(data).map_err(|e| e.to_string())?)?
    } else {
        nbt::decode(data, format == "mcstructure")?
    };
    let mut doc = Document::imported(source);
    doc.source_format = Some(format.into());
    match format {
        "schem" => schem::read_schem(&root, &mut doc)?,
        "litematic" => litematic::read_litematic(&root, &mut doc)?,
        "nbt" | "snbt" => structure::read_structure(&root, &mut doc)?,
        "schematic" => schematic::read_legacy(&root, &mut doc)?,
        "mcstructure" => mcstructure::read_bedrock(&root, &mut doc)?,
        _ => unreachable!(),
    }
    if doc.regions.is_empty() {
        return Err("File has no regions".into());
    }
    if doc.edition == "java" {
        if doc.data_version > 0 && doc.data_version < crate::registry::MIN_DATA_VERSION {
            return Err("Files older than Java 1.13 are unsupported".into());
        }
        if doc.data.versions()?.contains(&doc.version) {
            doc.catalog = Some(doc.data.registry(&doc.version)?);
        }
    }
    Ok(doc)
}

#[derive(Default)]
pub struct ExportReport {
    pub errors: Vec<String>,
    pub losses: Vec<String>,
}

/// Inspect the same preparation and checks used by encode.
pub fn check_export(doc: &Document, format: &str, flatten: bool) -> Result<ExportReport> {
    valid_format(format)?;
    Ok(match prepare(doc, format, flatten) {
        Ok((_, losses)) => ExportReport {
            errors: vec![],
            losses,
        },
        Err(error) => ExportReport {
            errors: vec![error],
            losses: vec![],
        },
    })
}

fn prepare<'a>(
    doc: &'a Document,
    format: &str,
    flatten: bool,
) -> Result<(Option<Cow<'a, Region>>, Vec<String>)> {
    valid_format(format)?;
    if doc.regions.is_empty() {
        return Err("Document has no regions".into());
    }
    if doc.edition == "bedrock" && format != "mcstructure" {
        return Err("Bedrock-to-Java state and entity mapping is not implemented".into());
    }
    if format == "blueprint" {
        let output = blueprint::encode(doc, &blueprint::Options::default())?;
        let mut losses = output.diagnostics;
        losses.push("Blueprints store sprite grids, not full block states, world coordinates, region bounds, or metadata".into());
        losses.extend(doc.notices.iter().cloned());
        return Ok((None, losses));
    }
    let single = if format == "litematic" {
        None
    } else {
        Some(single(doc, flatten)?)
    };
    let mut losses = vec![];
    if doc.regions.len() > 1 && format != "litematic" {
        losses
            .push("Destination stores one region; region names and boundaries will be lost".into());
    }
    if !doc.metadata.is_empty() && matches!(format, "nbt" | "snbt" | "schematic" | "mcstructure") {
        losses.push("Destination does not retain document metadata".into());
    }
    let regions: Vec<_> = match &single {
        Some(r) => vec![("main", r.as_ref())],
        None => doc
            .regions
            .iter()
            .map(|(name, r)| (name.as_str(), r))
            .collect(),
    };
    for (name, r) in regions {
        Bounds::new(r.bounds.start, r.bounds.size)?;
        let volume = r.bounds.volume()?;
        let limit = match format {
            "schem" => 65535,
            "schematic" => 32767,
            _ => i32::MAX,
        };
        if volume == 0 || r.bounds.size.iter().any(|&v| v > limit) {
            return Err(format!("{name}: dimensions must be 1 through {limit}"));
        }
        for i in 0..3 {
            r.origin[i]
                .checked_add(r.bounds.start[i])
                .and_then(|v| v.checked_add(r.bounds.size[i]))
                .ok_or("Export coordinate overflow")?;
        }
        if r.present.as_ref().is_some_and(|cells| cells.len() < volume)
            && !matches!(format, "nbt" | "snbt" | "mcstructure")
        {
            losses.push(format!(
                "{name}: omitted structure cells become explicit air in {format}"
            ));
        }
        for k in r.retained.spatial.keys() {
            if !matches!(
                (format, k.as_str()),
                ("schem", "Biomes") | ("litematic", "PendingBlockTicks" | "PendingFluidTicks")
            ) {
                losses.push(format!("{name}: {k} cannot be represented in {format}"));
            }
        }
        if let Some(data) = &r.retained.bedrock
            && (data.size != r.bounds.size
                || r.bounds.start != [0; 3]
                || data.secondary.len() != volume)
        {
            return Err("Resizing imported Bedrock secondary layers is not implemented".into());
        }
        if matches!(format, "nbt" | "snbt") && (r.origin != [0; 3] || r.bounds.start != [0; 3]) {
            losses.push(format!(
                "{name}: Java structure files do not retain the placement origin"
            ));
        }
        let blocks: BTreeSet<_> = r
            .blocks
            .states()
            .chain(std::iter::once(&Block::air()))
            .cloned()
            .collect();
        if format == "schematic" {
            for b in &blocks {
                doc.registry()?.legacy_pair(b)?;
            }
        }
        if format == "mcstructure" {
            let original = r.retained.bedrock.as_ref();
            for b in &blocks {
                mcstructure::palette_entry(b, original)?;
            }
            if doc.edition == "bedrock" && original.is_none() {
                return Err("Missing retained Bedrock data".into());
            }
        }
        if (!r.entities.is_empty() || !r.block_entities.is_empty())
            && ((format == "schematic" && doc.source_format.as_deref() != Some("schematic"))
                || (format == "mcstructure" && doc.edition == "java"))
        {
            losses.push(format!(
                "Entity NBT cannot be converted to {format} automatically"
            ));
        }
    }
    losses.extend(doc.notices.iter().cloned());
    losses.sort();
    losses.dedup();
    Ok((single, losses))
}

fn single(doc: &Document, flatten: bool) -> Result<Cow<'_, Region>> {
    if doc.regions.len() == 1 {
        return Ok(Cow::Borrowed(doc.regions.values().next().unwrap()));
    }
    if !flatten {
        return Err("Multiple regions require flatten=True".into());
    }
    let mut bounds: Vec<Bounds> = vec![];
    let mut ends = vec![];
    let mut volume = 0;
    for source in doc.regions.values() {
        if !source.retained.is_empty() {
            return Err("Flattening regions with retained spatial data is not supported".into());
        }
        if source.bounds.volume()? == 0 {
            return Err("Cannot export an empty region".into());
        }
        let start = Transform::move_by(source.origin).cell(source.bounds.start)?;
        let b = Bounds::new(start, source.bounds.size)?;
        if bounds.iter().any(|a| {
            (0..3)
                .all(|i| a.start[i] < b.start[i] + b.size[i] && b.start[i] < a.start[i] + a.size[i])
        }) {
            return Err("Overlapping region bounds; resolve overlap before export".into());
        }
        ends.push(start);
        ends.push(std::array::from_fn(|i| start[i] + b.size[i] - 1));
        volume += b.volume()?;
        bounds.push(b);
    }
    let mut r = Region::new([0; 3]);
    r.bounds = Bounds::around(ends.into_iter())?;
    if volume < r.bounds.volume()? || doc.regions.values().any(|r| r.present.is_some()) {
        r.present = Some(BTreeSet::new());
    }
    for source in doc.regions.values() {
        let t = Transform::move_by(source.origin);
        for (p, b) in source.blocks.iter() {
            r.blocks.set(t.cell(*p)?, b);
        }
        for (p, data) in &source.block_entities {
            r.block_entities.insert(t.cell(*p)?, data.clone());
        }
        if let Some(present) = &mut r.present {
            match &source.present {
                Some(cells) => {
                    for p in cells {
                        present.insert(t.cell(*p)?);
                    }
                }
                None => {
                    for p in source.bounds.positions() {
                        present.insert(t.cell(p)?);
                    }
                }
            }
        }
        for e in &source.entities {
            let mut e = e.clone();
            e.position = t.point(e.position)?;
            r.entities.push(e);
        }
    }
    Ok(Cow::Owned(r))
}

pub fn encode(doc: &Document, format: &str, allow_loss: bool, flatten: bool) -> Result<Vec<u8>> {
    let (single, losses) = prepare(doc, format, flatten)?;
    if !allow_loss && !losses.is_empty() {
        return Err(format!(
            "Export would lose information:\n{}",
            losses.join("\n")
        ));
    }
    if format == "blueprint" {
        return Ok(blueprint::encode(doc, &blueprint::Options::default())?
            .text
            .into_bytes());
    }
    let root = if let Some(r) = single {
        match format {
            "schem" => schem::write_schem(doc, &r)?,
            "nbt" | "snbt" => structure::write_structure(doc, &r)?,
            "schematic" => schematic::write_legacy(doc, &r)?,
            "mcstructure" => mcstructure::write_bedrock(doc, &r)?,
            _ => unreachable!(),
        }
    } else {
        litematic::write_litematic(doc)?
    };
    if format == "snbt" {
        return Ok(nbt::to_snbt(&root)?.into_bytes());
    }
    nbt::encode(&root, format == "mcstructure", format != "mcstructure")
}
