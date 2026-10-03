//! Structure file codecs. Items left as NBT and decoded as-is
use crate::{Result, catalog::MinecraftData, model::*, nbt};
use serde::Deserialize;
use std::collections::BTreeMap;

pub mod blueprint;
mod common;
mod export;
pub use export::{ExportReport, check_export, encode};
mod litematic;
mod mcstructure;
mod schem;
mod structure;

/// Supported codec names; nbt and snbt refer to Java structure files.
pub const FORMATS: [&str; 6] = [
    "blueprint",
    "schem",
    "litematic",
    "nbt",
    "snbt",
    "mcstructure",
];

/// Blueprint-only import settings; other codecs require empty defaults.
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImportOptions {
    /// Explicit Java version required for blueprints; latest is not accepted.
    pub version: Option<String>,
    /// Blueprint region origin in schematic-global coordinates; None defaults to zero.
    pub origin: Option<Position>,
    /// Blueprint symbols mapped to full block-state strings.
    pub palette: BTreeMap<String, String>,
}

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

/// Decodes bytes and loads an authoring catalog when one exists for the imported version.
pub async fn decode(
    data: &[u8],
    format: &str,
    source: std::sync::Arc<MinecraftData>,
    options: &ImportOptions,
) -> Result<Schematic> {
    valid_format(format)?;
    if data.len() > nbt::MAX_BYTES {
        return Err("Input exceeds 256 MiB".into());
    }
    if format == "blueprint" {
        let version = options
            .version
            .as_deref()
            .filter(|v| !v.is_empty() && *v != "latest")
            .ok_or(
                "Blueprint import requires an explicit Java version, for example version='1.21.1'",
            )?;
        source.load(version).await?;
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
    if format != "mcstructure" {
        source.initialize().await?;
    }
    let mut schematic = Schematic::imported(source);
    match format {
        "schem" => schem::read_schem(&root, &mut schematic)?,
        "litematic" => litematic::read_litematic(&root, &mut schematic)?,
        "nbt" | "snbt" => structure::read_structure(&root, &mut schematic)?,
        "mcstructure" => mcstructure::read_bedrock(&root, &mut schematic)?,
        _ => unreachable!(),
    }
    if schematic.regions.is_empty() {
        return Err("File has no regions".into());
    }
    if schematic.edition == "java" {
        if schematic.data_version > 0
            && schematic.data_version < crate::versions::MIN_JAVA_DATA_VERSION
        {
            return Err("Files older than Java 1.13 are unsupported".into());
        }
        if schematic.data.versions()?.contains(&schematic.version) {
            schematic.catalog = Some(schematic.data.load(&schematic.version).await?);
        }
    }
    Ok(schematic)
}
