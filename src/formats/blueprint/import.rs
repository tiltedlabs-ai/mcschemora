//! Imports supported Minecraft Wiki layered-blueprint templates into Java schematics.

use super::sprites;
use crate::{
    Result,
    catalog::MinecraftData,
    formats::ImportOptions,
    model::{Block, Bounds, Region, Schematic},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Imports one UTF-8 layered-blueprint template using a loaded, explicit Java catalog.
///
/// options.version must name a fixed version already loaded into source. Palette overrides
/// resolve ambiguous symbols; inferred states are reported in import_diagnostics.
pub fn decode(
    data: &[u8],
    source: Arc<MinecraftData>,
    options: &ImportOptions,
) -> Result<Schematic> {
    let version = options
        .version
        .as_deref()
        .filter(|s| !s.is_empty() && *s != "latest")
        .ok_or(
            "Blueprint import requires an explicit Java version, for example version='1.21.1'",
        )?;
    let text = std::str::from_utf8(data).map_err(|e| format!("Blueprint must be UTF-8: {e}"))?;
    let text = text
        .trim()
        .strip_prefix('\u{feff}')
        .unwrap_or(text.trim())
        .replace("\r\n", "\n");
    let body = text
        .strip_prefix("{{")
        .and_then(|s| s.strip_suffix("}}"))
        .ok_or("Expected a single {{layered blueprint|...}} template")?;
    if body.contains(['{', '}']) {
        return Err("Nested wiki templates are unsupported in blueprints".into());
    }
    let mut parts = body.split('|');
    if !parts
        .next()
        .unwrap_or("")
        .trim()
        .replace('_', " ")
        .eq_ignore_ascii_case("layered blueprint")
    {
        return Err("Expected the layered blueprint template".into());
    }
    let mut definitions = BTreeMap::new();
    let mut settings = BTreeMap::new();
    let mut layers = Vec::new();
    while let Some(part) = parts.next() {
        if part.trim().starts_with("----") {
            let grid = parts.next().ok_or("Layer title has no grid")?;
            let grid = grid.strip_prefix('\n').unwrap_or(grid);
            let grid = grid.strip_suffix('\n').unwrap_or(grid);
            let rows: Vec<Vec<char>> = grid.split('\n').map(|row| row.chars().collect()).collect();
            if rows.iter().flatten().any(|c| c.is_control()) {
                return Err("Blueprint grids cannot contain tabs or control characters".into());
            }
            layers.push(rows);
        } else if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if key.chars().count() == 1 && !key.chars().next().unwrap().is_whitespace() {
                if definitions
                    .insert(key.to_string(), value.to_string())
                    .is_some()
                {
                    return Err(format!("Duplicate blueprint palette symbol {key:?}"));
                }
            } else if matches!(key, "name" | "sheet" | "scale" | "default") {
                if settings.insert(key, value).is_some() {
                    return Err(format!("Duplicate blueprint setting {key:?}"));
                }
            } else {
                return Err(format!("Unsupported blueprint parameter {key:?}"));
            }
        } else if !part.trim().is_empty() {
            return Err("Expected a sprite definition or ----layer title".into());
        }
    }
    if layers.is_empty() {
        return Err("Blueprint has no layers".into());
    }
    let width = layers.iter().flatten().map(Vec::len).max().unwrap_or(0);
    let depth = layers.iter().map(Vec::len).max().unwrap_or(0);
    if width == 0 || width.checked_mul(depth).is_none_or(|n| n > 65536) {
        return Err("Blueprint must contain 1 through 65,536 cells per layer".into());
    }
    let size = [width, layers.len(), depth]
        .map(|n| i32::try_from(n).map_err(|_| "Blueprint dimensions overflow"));
    let bounds = Bounds::new([0; 3], [size[0]?, size[1]?, size[2]?])?;
    let origin = options.origin.unwrap_or([0; 3]);
    Bounds::new(origin, bounds.size)?;
    let used: BTreeSet<String> = layers
        .iter()
        .flatten()
        .flatten()
        .filter(|&&c| c != ' ')
        .map(char::to_string)
        .collect();
    for symbol in options.palette.keys() {
        if symbol.chars().count() != 1 || !used.contains(symbol) {
            return Err(format!(
                "Palette override {symbol:?} must name a symbol used in the blueprint"
            ));
        }
    }
    let mut schematic = Schematic::new("java", version, source)?;
    let catalog = schematic.registry()?;
    let sheet = settings.get("sheet").copied().unwrap_or("BlockSprite");
    let references: BTreeMap<String, String> = used
        .iter()
        .filter(|key| !options.palette.contains_key(*key))
        .filter_map(|key| {
            definitions
                .get(key)
                .map(|value| (key.clone(), sprites::normalize(value, sheet)))
        })
        .collect();
    let reverse = sprites::reverse(catalog, &references.values().cloned().collect())?;
    let mut palette = BTreeMap::new();
    let mut errors = Vec::new();
    let mut notices = Vec::new();
    for symbol in used {
        if let Some(state) = options.palette.get(&symbol) {
            palette.insert(
                symbol.clone(),
                catalog
                    .resolve(&Block::parse(state)?)
                    .map_err(|e| format!("Palette {symbol:?}: {e}"))?,
            );
        } else if let Some(sprite) = references.get(&symbol) {
            match reverse.get(sprite) {
                Some(Some(imported)) => {
                    if !imported.defaulted.is_empty() {
                        notices.push(format!(
                            "Blueprint palette {symbol:?} ({sprite}): defaulted {}",
                            imported.defaulted.join(", ")
                        ));
                    }
                    palette.insert(symbol, imported.block.clone());
                }
                candidate => errors.push(format!(
                    "{symbol:?} ({sprite}): {}",
                    if candidate.is_some() {
                        "sprite has multiple possible block identities"
                    } else {
                        "no block mapping is available"
                    }
                )),
            }
        } else {
            errors.push(format!("{symbol:?}: missing sprite definition"));
        }
    }
    if !errors.is_empty() {
        return Err(format!(
            "Cannot resolve blueprint palette:\n{}\nSupply palette={{'symbol': block(...)}} overrides for these symbols.",
            errors.join("\n")
        ));
    }
    let mut region = Region::new(origin);
    region.bounds = bounds;
    for (y, layer) in layers.iter().enumerate() {
        for (z, row) in layer.iter().enumerate() {
            for (x, symbol) in row.iter().enumerate() {
                if *symbol != ' ' {
                    region.blocks.set(
                        [x as i32, y as i32, z as i32],
                        &palette[&symbol.to_string()],
                    );
                }
            }
        }
    }
    schematic.regions.insert("main".into(), region);
    schematic.import_diagnostics = notices;
    Ok(schematic)
}
