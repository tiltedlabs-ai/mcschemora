use super::{blueprint, litematic, mcstructure, nbt, schem, structure, valid_format};
use crate::{Result, convert, model::*, transform::Transform};
use std::{borrow::Cow, collections::BTreeSet};

/// Preflight results separating blocking errors from explicitly acceptable omissions.
#[derive(Default)]
pub struct ExportReport {
    /// Problems that prevent export even when loss is accepted.
    pub errors: Vec<String>,
    /// Omissions that require allow_loss=true during encoding.
    pub losses: Vec<String>,
}

pub async fn check_export(
    schematic: &Schematic,
    format: &str,
    version: Option<&str>,
    flatten: bool,
) -> Result<ExportReport> {
    valid_format(format)?;
    Ok(
        match convert::schematic(schematic, version)
            .await
            .and_then(|schematic| prepare(&schematic, format, flatten).map(|(_, losses)| losses))
        {
            Ok(losses) => ExportReport {
                errors: vec![],
                losses,
            },
            Err(error) => ExportReport {
                errors: vec![error],
                losses: vec![],
            },
        },
    )
}

fn prepare<'a>(
    schematic: &'a Schematic,
    format: &str,
    flatten: bool,
) -> Result<(Option<Cow<'a, Region>>, Vec<String>)> {
    valid_format(format)?;
    if schematic.regions.is_empty() {
        return Err("Schematic has no regions".into());
    }
    if schematic.edition == "bedrock" && format != "mcstructure" {
        return Err("Bedrock-to-Java state and entity mapping is not implemented".into());
    }
    if format == "blueprint" {
        let output = blueprint::encode(schematic, &blueprint::Options::default())?;
        let mut losses = output.diagnostics;
        losses.push("Blueprints store sprite grids, not full block states, schematic-global coordinates, region bounds, or metadata".into());
        losses.extend(schematic.notices.iter().cloned());
        return Ok((None, losses));
    }
    let single = if format == "litematic" {
        None
    } else {
        Some(single(schematic, flatten)?)
    };
    let mut losses = vec![];
    if schematic.regions.len() > 1 && format != "litematic" {
        losses
            .push("Destination stores one region; region names and boundaries will be lost".into());
    }
    if !schematic.metadata.is_empty() && matches!(format, "nbt" | "snbt" | "mcstructure") {
        losses.push("Destination does not retain schematic metadata".into());
    }
    let regions: Vec<_> = match &single {
        Some(r) => vec![("main", r.as_ref())],
        None => schematic
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
        if format == "mcstructure" {
            let blocks: BTreeSet<_> = r
                .blocks
                .states()
                .chain(std::iter::once(&Block::air()))
                .cloned()
                .collect();
            let original = r.retained.bedrock.as_ref();
            for b in &blocks {
                mcstructure::palette_entry(b, original)?;
            }
            if schematic.edition == "bedrock" && original.is_none() {
                return Err("Missing retained Bedrock data".into());
            }
        }
        if (!r.entities.is_empty() || !r.block_entities.is_empty())
            && format == "mcstructure"
            && schematic.edition == "java"
        {
            losses.push(format!(
                "Entity NBT cannot be converted to {format} automatically"
            ));
        }
    }
    losses.extend(schematic.notices.iter().cloned());
    losses.sort();
    losses.dedup();
    Ok((single, losses))
}

fn single(schematic: &Schematic, flatten: bool) -> Result<Cow<'_, Region>> {
    if schematic.regions.len() == 1 {
        return Ok(Cow::Borrowed(schematic.regions.values().next().unwrap()));
    }
    if !flatten {
        return Err("Multiple regions require flatten=True".into());
    }
    let mut bounds: Vec<Bounds> = vec![];
    let mut ends = vec![];
    let mut volume = 0;
    for source in schematic.regions.values() {
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
    if volume < r.bounds.volume()? || schematic.regions.values().any(|r| r.present.is_some()) {
        r.present = Some(BTreeSet::new());
    }
    for source in schematic.regions.values() {
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

/// Encodes a schematic, rejecting blocking errors and unaccepted losses.
///
/// allow_loss accepts reported omissions. flatten merges regions for single-region formats
/// and reports boundary loss; overlapping region bounds error.
pub async fn encode(
    schematic: &Schematic,
    format: &str,
    version: Option<&str>,
    allow_loss: bool,
    flatten: bool,
) -> Result<Vec<u8>> {
    valid_format(format)?;
    let converted = convert::schematic(schematic, version).await?;
    let schematic = converted.as_ref();
    let (single, losses) = prepare(schematic, format, flatten)?;
    if !allow_loss && !losses.is_empty() {
        return Err(format!(
            "Export would lose information:\n{}",
            losses.join("\n")
        ));
    }
    if format == "blueprint" {
        return Ok(
            blueprint::encode(schematic, &blueprint::Options::default())?
                .text
                .into_bytes(),
        );
    }
    let root = if let Some(r) = single {
        match format {
            "schem" => schem::write_schem(schematic, &r)?,
            "nbt" | "snbt" => structure::write_structure(schematic, &r)?,
            "mcstructure" => mcstructure::write_bedrock(schematic, &r)?,
            _ => unreachable!(),
        }
    } else {
        litematic::write_litematic(schematic)?
    };
    if format == "snbt" {
        return Ok(nbt::to_snbt(&root)?.into_bytes());
    }
    nbt::encode(&root, format == "mcstructure", format != "mcstructure")
}
