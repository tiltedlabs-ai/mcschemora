mod indexed;

use super::{SceneOptions, View};
use crate::{
    Result,
    model::{Block, Document},
    sprite_ids,
};
use image::{
    ImageEncoder, RgbaImage,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    sync::OnceLock,
};

#[derive(Clone, Debug)]
pub struct Options {
    pub selection: SceneOptions,
    pub view: View,
    pub cell_size: u32,
    pub grid: bool,
    pub entities: bool,
    pub sprites: BTreeMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            selection: SceneOptions::default(),
            view: View::Top,
            cell_size: 32,
            grid: false,
            entities: true,
            sprites: BTreeMap::new(),
        }
    }
}

pub struct Output {
    pub png: Vec<u8>,
    pub diagnostics: Vec<String>,
}

#[derive(Deserialize)]
struct Entry {
    sheet: String,
    rect: [u32; 4],
}

#[derive(Deserialize)]
struct Manifest {
    sprites: HashMap<String, Entry>,
}

struct Sprite {
    sheet: usize,
    rect: [u32; 4],
}

struct Assets {
    sheets: Vec<RgbaImage>,
    sprites: Vec<Sprite>,
    ids: HashMap<String, usize>,
}

impl Assets {
    fn load() -> Result<Self> {
        let sources: [(&str, &[u8]); 3] = [
            (
                "blocks.png",
                include_bytes!("../../data/wiki-sprites/blocks.png"),
            ),
            (
                "entities.png",
                include_bytes!("../../data/wiki-sprites/entities.png"),
            ),
            (
                "schematic.png",
                include_bytes!("../../data/wiki-sprites/schematic.png"),
            ),
        ];
        let sheets = sources
            .iter()
            .map(|(_, bytes)| {
                image::load_from_memory(bytes)
                    .map(|image| image.into_rgba8())
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>>>()?;
        let manifest: Manifest =
            serde_json::from_slice(include_bytes!("../../data/wiki-sprites/sprites.json"))
                .map_err(|e| e.to_string())?;
        let mut ids = HashMap::new();
        let mut sprites = Vec::new();
        let mut rectangles = HashMap::new();
        for (name, entry) in manifest.sprites {
            let sheet = sources
                .iter()
                .position(|(name, _)| *name == entry.sheet)
                .ok_or("Invalid sprite sheet")?;
            let [x, y, w, h] = entry.rect;
            if w == 0
                || h == 0
                || x.checked_add(w)
                    .is_none_or(|end| end > sheets[sheet].width())
                || y.checked_add(h)
                    .is_none_or(|end| end > sheets[sheet].height())
            {
                return Err(format!("Invalid sprite rectangle: {name}"));
            }
            let index = *rectangles.entry((sheet, entry.rect)).or_insert_with(|| {
                let index = sprites.len();
                sprites.push(Sprite {
                    sheet,
                    rect: entry.rect,
                });
                index
            });
            ids.insert(name, index);
        }
        if !ids.contains_key("SchematicSprite:???") {
            return Err("Missing unknown sprite".into());
        }
        Ok(Self {
            sheets,
            sprites,
            ids,
        })
    }

    fn tile(&self, id: usize, size: u32) -> RgbaImage {
        let sprite = &self.sprites[id];
        let [x, y, width, height] = sprite.rect;
        let source = image::imageops::crop_imm(&self.sheets[sprite.sheet], x, y, width, height);
        let longest = width.max(height);
        let w = ((u64::from(width) * u64::from(size)) / u64::from(longest)).max(1) as u32;
        let h = ((u64::from(height) * u64::from(size)) / u64::from(longest)).max(1) as u32;
        let scaled = image::imageops::resize(&*source, w, h, image::imageops::FilterType::Nearest);
        let mut tile = RgbaImage::new(size, size);
        image::imageops::replace(
            &mut tile,
            &scaled,
            i64::from((size - w) / 2),
            i64::from((size - h) / 2),
        );
        tile
    }
}

fn oriented(block: &Block, view: View) -> Block {
    let side = !matches!(view, View::Top | View::Bottom);
    let directions = [
        ("north", [0., 0., -1.]),
        ("south", [0., 0., 1.]),
        ("east", [1., 0., 0.]),
        ("west", [-1., 0., 0.]),
        ("up", [0., 1., 0.]),
        ("down", [0., -1., 0.]),
    ];
    let names: HashMap<_, _> = directions
        .iter()
        .map(|(name, vector)| {
            let [x, y, z] = view.project(*vector);
            let target = if side { [x, -y, z] } else { [x, z, y] };
            (
                *name,
                directions
                    .iter()
                    .find(|(_, vector)| *vector == target)
                    .unwrap()
                    .0,
            )
        })
        .collect();
    let mut result = block.clone();
    result.properties = block
        .properties
        .iter()
        .map(|(key, value)| {
            let key = names.get(key.as_str()).copied().unwrap_or(key);
            let value = if key == "facing" {
                names.get(value.as_str()).copied().unwrap_or(value)
            } else {
                value
            };
            (key.into(), value.into())
        })
        .collect();
    result
}

fn resolve_block(
    block: &Block,
    display: &str,
    options: &Options,
    assets: &Assets,
    diagnostics: &mut BTreeSet<String>,
) -> usize {
    let state = block.text();
    if let Some(name) = options
        .sprites
        .get(&state)
        .or_else(|| options.sprites.get(&block.name))
    {
        return assets.ids[name];
    }
    let mut sprite = sprite_ids::resolve(&oriented(block, options.view), display);
    let side = !matches!(options.view, View::Top | View::Bottom);
    if side && sprite.name.starts_with("SchematicSprite:") {
        let candidate = if block.name == "minecraft:redstone_wire" {
            let lit = block
                .properties
                .get("power")
                .is_some_and(|power| power != "0");
            diagnostics.insert("Side-view redstone wire symbols omit connection geometry".into());
            format!("SchematicSprite:rd-${}", if lit { "!" } else { "" })
        } else if let Some((prefix, suffix)) = sprite.name.split_once('-') {
            format!("{prefix}-${suffix}")
        } else {
            format!("{}-$", sprite.name)
        };
        if assets.ids.contains_key(&candidate) {
            sprite.name = candidate;
        } else {
            diagnostics.insert(format!(
                "{state}: no side-view symbol; using {}",
                sprite.name
            ));
        }
    }
    if matches!(options.view, View::Bottom) {
        diagnostics.insert("Bottom views use projected icon directions; the wiki artwork does not depict block undersides".into());
    }
    if !sprite.omitted.is_empty() {
        diagnostics.insert(format!(
            "{state}: sprite does not encode {}",
            sprite.omitted.join(", ")
        ));
    }
    if sprite.name.starts_with("BlockSprite:") {
        diagnostics
            .insert("Ordinary block sprites are wiki icons, not view-specific block faces".into());
    }
    match assets.ids.get(&sprite.name) {
        Some(&index) => index,
        None => {
            diagnostics.insert(format!(
                "{state}: missing {}; rendered unknown symbol",
                sprite.name
            ));
            assets.ids["SchematicSprite:???"]
        }
    }
}

struct Cell<'a> {
    block: &'a Block,
    depth: f64,
}

pub fn encode(document: &Document, options: &Options) -> Result<Output> {
    if document.edition != "java" {
        return Err("Sprite rendering requires Java Edition block states".into());
    }
    if matches!(options.view, View::Isometric) {
        return Err("Sprite views must be top, bottom, north, south, east, or west".into());
    }
    if !(1..=128).contains(&options.cell_size) {
        return Err("Sprite cell_size must be between 1 and 128 pixels".into());
    }
    options.selection.validate(document)?;
    static ASSETS: OnceLock<Result<Assets>> = OnceLock::new();
    let assets = ASSETS
        .get_or_init(Assets::load)
        .as_ref()
        .map_err(Clone::clone)?;
    for name in options.sprites.values() {
        if !assets.ids.contains_key(name) {
            return Err(format!("Unknown sprite override: {name}"));
        }
    }
    let mut cells: HashMap<[i64; 2], Cell<'_>> = HashMap::new();
    let mut entity_cells: BTreeMap<[i64; 2], (f64, &str)> = BTreeMap::new();
    let mut positions = HashSet::new();
    let overlap_check = options.selection.region.is_none() && document.regions.len() > 1;
    let mut attached = 0;
    for (name, region) in &document.regions {
        if options
            .selection
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
            let mut position = [0; 3];
            for axis in 0..3 {
                position[axis] = region.origin[axis]
                    .checked_add(local[axis])
                    .ok_or("Sprite coordinate overflow")?;
            }
            if !options.selection.contains(position.map(f64::from)) {
                continue;
            }
            if overlap_check && !positions.insert(position) {
                return Err(format!("Selected regions overlap at {position:?}"));
            }
            attached += usize::from(region.block_entities.contains_key(local));
            let [x, y, depth] = options.view.project(position.map(|v| f64::from(v) + 0.5));
            let key = [x.floor() as i64, y.floor() as i64];
            let cell = cells.entry(key).or_insert(Cell { block, depth });
            if depth > cell.depth {
                *cell = Cell { block, depth };
            }
        }
        if options.entities {
            for entity in &region.entities {
                let position = std::array::from_fn(|axis| {
                    entity.position[axis] + f64::from(region.origin[axis])
                });
                if position.iter().any(|v| {
                    !v.is_finite() || *v < f64::from(i32::MIN) || *v >= f64::from(i32::MAX) + 1.
                }) {
                    return Err("Entity position outside sprite coordinate limits".into());
                }
                if !options.selection.contains(position) {
                    continue;
                }
                let [x, y, _] = options.view.project(position.map(|v| v.floor() + 0.5));
                let depth = options.view.project(position)[2];
                let key = [x.floor() as i64, y.floor() as i64];
                let id = super::entities::id(entity);
                let entry = entity_cells.entry(key).or_insert((depth, id));
                if depth > entry.0 || (depth == entry.0 && id < entry.1) {
                    *entry = (depth, id);
                }
            }
        }
    }
    entity_cells.retain(|key, (depth, _)| cells.get(key).is_none_or(|cell| *depth >= cell.depth));
    if cells.is_empty() && entity_cells.is_empty() {
        return Err("Selected scene has no sprites to render".into());
    }
    let mut min = [i64::MAX; 2];
    let mut max = [i64::MIN; 2];
    for key in cells.keys().chain(entity_cells.keys()) {
        for axis in 0..2 {
            min[axis] = min[axis].min(key[axis]);
            max[axis] = max[axis].max(key[axis]);
        }
    }
    let dimensions: [u64; 2] = std::array::from_fn(|i| {
        (max[i] - min[i] + 1) as u64 * u64::from(options.cell_size) + u64::from(options.grid)
    });
    if dimensions.iter().any(|&v| v > 8192) || dimensions[0] * dimensions[1] > 16_777_216 {
        return Err("Sprite image exceeds 8192 pixels per side or 16777216 total pixels; select a smaller region or cell_size".into());
    }
    let mut diagnostics = BTreeSet::new();
    if attached > 0 {
        diagnostics.insert(format!(
            "{attached} selected block entities have data not represented by sprites"
        ));
    }
    let catalog = document.registry()?;
    let mut resolved = HashMap::new();
    let mut tiles = HashMap::new();
    let origin = |key: [i64; 2]| {
        [
            (key[0] - min[0]) as u32 * options.cell_size,
            (key[1] - min[1]) as u32 * options.cell_size,
        ]
    };
    let mut cells: Vec<_> = cells.into_iter().collect();
    cells.sort_unstable_by_key(|(key, _)| (key[1], key[0]));
    let mut draws = Vec::with_capacity(cells.len());
    for (key, cell) in cells {
        let index = match resolved.get(cell.block) {
            Some(&index) => index,
            None => {
                let index = resolve_block(
                    cell.block,
                    catalog.block_display_name(&cell.block.name)?,
                    options,
                    assets,
                    &mut diagnostics,
                );
                resolved.insert(cell.block, index);
                index
            }
        };
        tiles
            .entry(index)
            .or_insert_with(|| assets.tile(index, options.cell_size));
        draws.push((origin(key), index));
    }
    if entity_cells.is_empty()
        && let Some(png) = indexed::encode(
            dimensions.map(|v| v as u32),
            options.cell_size,
            options.grid,
            &draws,
            &tiles,
        )?
    {
        return Ok(Output {
            png,
            diagnostics: diagnostics.into_iter().collect(),
        });
    }
    let mut output = RgbaImage::new(dimensions[0] as u32, dimensions[1] as u32);
    let stride = output.width() as usize * 4;
    let row_bytes = options.cell_size as usize * 4;
    for ([x, y], index) in draws {
        for (row, pixels) in tiles[&index].as_raw().chunks_exact(row_bytes).enumerate() {
            let start = (y as usize + row) * stride + x as usize * 4;
            output.as_mut()[start..start + row_bytes].copy_from_slice(pixels);
        }
    }
    for (key, (_, id)) in entity_cells {
        let name = options.sprites.get(id).cloned().unwrap_or_else(|| {
            format!(
                "EntitySprite:{}",
                id.strip_prefix("minecraft:")
                    .unwrap_or(id)
                    .replace('_', "-")
            )
        });
        let index = assets.ids.get(&name).copied().unwrap_or_else(|| {
            diagnostics.insert(format!("{id}: missing {name}; rendered unknown symbol"));
            assets.ids["SchematicSprite:???"]
        });
        diagnostics.insert("Entity sprites are generic icons; pose, age, variants, and attached data are not represented".into());
        let tile = tiles
            .entry(index)
            .or_insert_with(|| assets.tile(index, options.cell_size));
        let [x, y] = origin(key);
        image::imageops::overlay(&mut output, tile, i64::from(x), i64::from(y));
    }
    if options.grid {
        let color = image::Rgba([40, 40, 40, 255]);
        for x in (0..output.width()).step_by(options.cell_size as usize) {
            for y in 0..output.height() {
                output.put_pixel(x, y, color);
            }
        }
        for y in (0..output.height()).step_by(options.cell_size as usize) {
            for x in 0..output.width() {
                output.put_pixel(x, y, color);
            }
        }
    }
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Level(1), FilterType::Sub)
        .write_image(
            output.as_raw(),
            output.width(),
            output.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(Output {
        png,
        diagnostics: diagnostics.into_iter().collect(),
    })
}
