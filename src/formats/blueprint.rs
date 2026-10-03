//! Minecraft Wiki layered-blueprint export and import.

pub mod import;
mod sprites;

use crate::{
    Result,
    model::{Block, MAX_VOLUME, Position, Schematic},
    transform::Transform,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const SYMBOLS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!$%()+,./:;?@^_~";

/// Selection and presentation settings for Minecraft Wiki layered-blueprint output.
#[derive(Clone, Debug)]
pub struct Options {
    /// Nonempty blueprint title.
    pub name: String,
    /// Named region to export; None includes all regions.
    pub region: Option<String>,
    /// Inclusive global Y range; None includes all layers containing blocks.
    pub y: Option<[i32; 2]>,
    /// Number of quarter turns about Y.
    pub rotation: i32,
    /// Block IDs or full state strings mapped to wiki sprite identifiers.
    pub sprites: BTreeMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            name: "Blueprint".into(),
            region: None,
            y: None,
            rotation: 0,
            sprites: BTreeMap::new(),
        }
    }
}

/// Blueprint markup and diagnostics describing omitted data.
#[derive(Clone, Debug)]
pub struct Output {
    /// UTF-8 Minecraft Wiki layered-blueprint markup.
    pub text: String,
    /// State details, attached data, and entities omitted from the sprite plan.
    pub diagnostics: Vec<String>,
}

fn escaped(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '|' => "&#124;".into(),
            '{' => "&#123;".into(),
            '}' => "&#125;".into(),
            '[' => "&#91;".into(),
            ']' => "&#93;".into(),
            _ => c.to_string(),
        })
        .collect()
}

/// Exports selected Java blocks as layered wiki markup without modifying the schematic.
///
/// Returns diagnostics for visual omissions. Overlapping selected cells and oversized
/// layers are errors.
pub fn encode(schematic: &Schematic, options: &Options) -> Result<Output> {
    if schematic.edition != "java" {
        return Err("Layered blueprints require Java Edition block states".into());
    }
    if options.name.trim().is_empty() || options.name.chars().any(char::is_control) {
        return Err("Blueprint name must be nonempty and contain no control characters".into());
    }
    if options.y.is_some_and(|range| range[0] > range[1]) {
        return Err("Y range start exceeds end".into());
    }
    if let Some(name) = &options.region {
        schematic.region(name)?;
    }
    for sprite in options.sprites.values() {
        if sprite.trim().is_empty()
            || sprite
                .chars()
                .any(|c| c.is_control() || "|{}[]<>".contains(c))
        {
            return Err(
                "Sprite overrides must be nonempty sprite identifiers, not wiki markup".into(),
            );
        }
    }
    let catalog = schematic.registry()?;
    let transform = Transform::rotate("y", options.rotation, [0.5; 3])?;
    let selected_y = |y: i32| options.y.is_none_or(|range| y >= range[0] && y <= range[1]);
    let mut cells = BTreeMap::<Position, Block>::new();
    let mut palette = BTreeMap::<Block, String>::new();
    let mut diagnostics = BTreeSet::new();
    let mut entities = 0;
    let mut attached = 0;
    for (name, region) in &schematic.regions {
        if options
            .region
            .as_ref()
            .is_some_and(|selected| selected != name)
        {
            continue;
        }
        for (local, block) in region.blocks.iter() {
            if block.is_air() {
                continue;
            }
            let mut global_position = [0; 3];
            for axis in 0..3 {
                global_position[axis] = region.origin[axis]
                    .checked_add(local[axis])
                    .ok_or("Blueprint coordinate overflow")?;
            }
            if !selected_y(global_position[1]) {
                continue;
            }
            let position = transform.cell(global_position)?;
            if !palette.contains_key(block) {
                let display = transform.block(&catalog.resolve(block)?, catalog)?;
                let sprite =
                    crate::sprite_ids::resolve(&display, catalog.block_display_name(&display.id)?);
                let override_name = options
                    .sprites
                    .get(&block.text())
                    .or_else(|| options.sprites.get(&block.id));
                if override_name.is_none() && !sprite.omitted.is_empty() {
                    diagnostics.insert(format!(
                        "{}: sprite does not encode {}",
                        block.text(),
                        sprite.omitted.join(", ")
                    ));
                }
                palette.insert(block.clone(), override_name.cloned().unwrap_or(sprite.name));
            }
            if cells.insert(position, block.clone()).is_some() {
                return Err(format!(
                    "Selected regions overlap at schematic-global position {global_position:?}"
                ));
            }
            attached += usize::from(region.block_entities.contains_key(local));
        }
        entities += region
            .entities
            .iter()
            .filter(|entity| {
                let y = entity.position[1] + f64::from(region.origin[1]);
                options
                    .y
                    .is_none_or(|range| y >= f64::from(range[0]) && y < f64::from(range[1]) + 1.)
            })
            .count();
    }
    if cells.is_empty() {
        return Err("Selected blueprint has no non-air blocks".into());
    }
    if palette.len() > SYMBOLS.len() {
        return Err(format!(
            "Blueprint needs {} palette symbols; maximum is {}. Select a smaller region or Y range",
            palette.len(),
            SYMBOLS.len()
        ));
    }
    let min: Position = std::array::from_fn(|axis| cells.keys().map(|p| p[axis]).min().unwrap());
    let max: Position = std::array::from_fn(|axis| cells.keys().map(|p| p[axis]).max().unwrap());
    let width = (i64::from(max[0]) - i64::from(min[0]) + 3) as usize;
    let depth = (i64::from(max[2]) - i64::from(min[2]) + 3) as usize;
    let y = options.y.unwrap_or([min[1], max[1]]);
    let height = (i64::from(y[1]) - i64::from(y[0]) + 1) as usize;
    if width.checked_mul(depth).is_none_or(|size| size > 65536)
        || width
            .checked_mul(depth)
            .and_then(|size| size.checked_mul(height))
            .is_none_or(|size| size > MAX_VOLUME)
    {
        return Err(
            "Blueprint exceeds 65,536 cells per padded layer or 16,777,216 total cells".into(),
        );
    }
    if entities > 0 {
        diagnostics.insert(format!(
            "{entities} entities are not represented by block layers"
        ));
    }
    if attached > 0 {
        diagnostics.insert(format!(
            "{attached} block entities have attached data not represented by sprites"
        ));
    }
    let symbols: BTreeMap<_, _> = palette.keys().zip(SYMBOLS.chars()).collect();
    let mut text = format!(
        "{{{{layered blueprint|name={}\n",
        escaped(options.name.trim())
    );
    for (block, sprite) in &palette {
        writeln!(text, "|{}={sprite}", symbols[block]).unwrap();
    }
    let border = " ".repeat(width);
    let mut layers = BTreeMap::<i32, Vec<(Position, char)>>::new();
    for (position, block) in &cells {
        layers
            .entry(position[1])
            .or_default()
            .push((*position, symbols[block]));
    }
    for (index, level) in (y[0]..=y[1]).enumerate() {
        writeln!(text, "|----Layer {}|", index + 1).unwrap();
        writeln!(text, "{border}").unwrap();
        let mut rows = vec![vec![b' '; width]; depth - 2];
        if let Some(layer) = layers.get(&level) {
            for &(position, symbol) in layer {
                rows[(i64::from(position[2]) - i64::from(min[2])) as usize]
                    [(i64::from(position[0]) - i64::from(min[0]) + 1) as usize] = symbol as u8;
            }
        }
        for row in rows {
            text.push_str(std::str::from_utf8(&row).unwrap());
            text.push('\n');
        }
        writeln!(text, "{border}").unwrap();
    }
    text.push_str("}}\n");
    Ok(Output {
        text,
        diagnostics: diagnostics.into_iter().collect(),
    })
}
