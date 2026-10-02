mod attachments;
mod cells;
mod entities;
mod fluids;
mod geometry;
pub mod glb;
mod models;
mod occlusion;
pub mod parts;
pub mod png;
mod special;
pub mod sprites;
mod view;

pub use view::View;

use models::Builder;
use occlusion::Occlusion;

use crate::{
    Result,
    model::{Document, Pos},
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Debug, Serialize)]
pub struct Texture {
    pub name: String,
    pub atlas: usize,
    pub uv: [f32; 4],
    pub size: [u32; 2],
    pub alpha: AlphaMode,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Serialize)]
pub struct Quad {
    pub vertices: [Vertex; 4],
    pub normal: [f32; 3],
    pub texture: usize,
    pub tint_index: Option<i32>,
    pub shade: bool,
    pub color: [u8; 4],
    pub texture_flags: Value,
    pub cull_face: Option<Pos>,
}

impl Quad {
    fn alpha(&self, texture: &Texture) -> AlphaMode {
        if self.texture_flags["force_translucent"].as_bool() == Some(true) {
            AlphaMode::Blend
        } else if self.texture_flags["force_cutout"].as_bool() == Some(true) {
            AlphaMode::Mask
        } else {
            texture.alpha
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Mesh {
    pub quads: Vec<Quad>,
    pub occludes: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Draw {
    pub mesh: usize,
    pub quads: Vec<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Instance {
    pub is_entity: bool,
    pub position: [f64; 3],
    pub rotation: [f32; 4],
    pub name: String,
    pub draws: Vec<Draw>,
}

impl Instance {
    fn validate_transform(&self) -> Result<()> {
        if !self.position.iter().all(|v| v.is_finite())
            || !self.rotation.iter().all(|v| v.is_finite())
            || (self.rotation.iter().map(|v| v * v).sum::<f32>() - 1.).abs() > 1e-4
        {
            return Err("Invalid scene instance transform".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub region: String,
    pub position: Pos,
    pub block: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreparedScene {
    pub atlases: Vec<PathBuf>,
    #[serde(skip)]
    pub atlas_images: std::sync::Arc<Vec<image::RgbaImage>>,
    pub textures: Vec<Texture>,
    pub meshes: Vec<Mesh>,
    pub instances: Vec<Instance>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Default)]
pub struct SceneOptions {
    pub region: Option<String>,
    pub x: Option<[i32; 2]>,
    pub y: Option<[i32; 2]>,
    pub z: Option<[i32; 2]>,
}

impl SceneOptions {
    pub(crate) fn validate(&self, document: &Document) -> Result<()> {
        for (axis, range) in ["X", "Y", "Z"].into_iter().zip([self.x, self.y, self.z]) {
            if range.is_some_and(|range| range[0] > range[1]) {
                return Err(format!("{axis} range start exceeds end"));
            }
        }
        if let Some(name) = &self.region {
            document.region(name)?;
        }
        Ok(())
    }

    pub(crate) fn contains(&self, position: [f64; 3]) -> bool {
        [self.x, self.y, self.z]
            .into_iter()
            .zip(position)
            .all(|(range, value)| {
                range.is_none_or(|range| {
                    value >= f64::from(range[0]) && value < f64::from(range[1]) + 1.
                })
            })
    }
}

#[derive(Debug)]
pub struct GeometryAssets {
    atlases: Vec<PathBuf>,
    atlas_images: std::sync::Arc<Vec<image::RgbaImage>>,
    textures: Vec<Texture>,
    texture_ids: BTreeMap<String, usize>,
    models: BTreeMap<String, Value>,
    states: BTreeMap<String, Value>,
}

fn read(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

impl GeometryAssets {
    pub fn load(path: &Path) -> Result<Self> {
        let manifest = read(&path.join("manifest.json"))?;
        let mut atlases = Vec::new();
        let mut images = Vec::new();
        for atlas in manifest["atlases"]
            .as_array()
            .ok_or("Missing visual atlases")?
        {
            let file = atlas["file"].as_str().ok_or("Missing atlas filename")?;
            crate::catalog::source::safe_path(file)?;
            let file = path.join(file);
            images.push(
                image::open(&file)
                    .map_err(|e| format!("{}: {e}", file.display()))?
                    .into_rgba8(),
            );
            atlases.push(file);
        }
        let sprites = read(&path.join("textures.json"))?;
        let mut textures = Vec::new();
        let mut texture_ids = BTreeMap::new();
        for (name, sprite) in sprites.as_object().ok_or("Invalid prepared textures")? {
            let atlas = sprite["atlas"].as_u64().ok_or("Invalid texture atlas")? as usize;
            let rect: [u32; 4] =
                serde_json::from_value(sprite["rect"].clone()).map_err(|e| e.to_string())?;
            let uv = serde_json::from_value(sprite["uv"].clone()).map_err(|e| e.to_string())?;
            let image = images.get(atlas).ok_or("Texture atlas outside bundle")?;
            if rect[2] == 0
                || rect[3] == 0
                || rect[0]
                    .checked_add(rect[2])
                    .is_none_or(|n| n > image.width())
                || rect[1]
                    .checked_add(rect[3])
                    .is_none_or(|n| n > image.height())
            {
                return Err("Texture rectangle outside atlas".into());
            }
            let mut alpha = AlphaMode::Opaque;
            for y in rect[1]..rect[1] + rect[3] {
                for x in rect[0]..rect[0] + rect[2] {
                    match image.get_pixel(x, y)[3] {
                        0 if alpha != AlphaMode::Blend => alpha = AlphaMode::Mask,
                        1..=254 => alpha = AlphaMode::Blend,
                        _ => (),
                    }
                }
            }
            texture_ids.insert(name.clone(), textures.len());
            textures.push(Texture {
                name: name.clone(),
                atlas,
                uv,
                size: [rect[2], rect[3]],
                alpha,
            });
        }
        if !texture_ids.contains_key("minecraft:missingno") {
            return Err("Visual bundle has no missing texture".into());
        }
        Ok(Self {
            atlases,
            atlas_images: std::sync::Arc::new(images),
            textures,
            texture_ids,
            models: serde_json::from_value(read(&path.join("models.json"))?)
                .map_err(|e| e.to_string())?,
            states: serde_json::from_value(read(&path.join("blockstates.json"))?)
                .map_err(|e| e.to_string())?,
        })
    }

    pub fn prepare(&self, document: &Document, options: &SceneOptions) -> Result<PreparedScene> {
        if document.edition != "java" {
            return Err("Geometry preparation requires Java Edition visuals".into());
        }
        options.validate(document)?;
        let cells = cells::Cells::new(document, options)?;
        let mut selected_entities = Vec::new();
        let mut diagnostics = Vec::new();
        for &(name, region) in &cells.regions {
            for entity in &region.entities {
                let position: [f64; 3] =
                    std::array::from_fn(|i| entity.position[i] + f64::from(region.origin[i]));
                if !position.iter().all(|v| v.is_finite()) {
                    return Err("Non-finite entity position".into());
                }
                if options.contains(position) {
                    selected_entities.push((name, entity, position));
                }
            }
        }
        let mut builder = Builder {
            assets: self,
            meshes: Vec::new(),
            applications: BTreeMap::new(),
            states: BTreeMap::new(),
        };
        let mut instances = Vec::new();
        let mut states: Vec<Vec<Option<std::sync::Arc<models::StateGeometry>>>> = cells
            .regions
            .iter()
            .map(|(_, region)| vec![None; region.blocks.palette_len()])
            .collect();
        let mut decorated = BTreeMap::new();
        let mut occlusion = Occlusion::new(cells.entries.iter().map(|cell| cell.position));
        let mut tinted = BTreeSet::new();
        let mut attached: BTreeMap<
            (crate::model::Block, String),
            std::sync::Arc<models::StateGeometry>,
        > = BTreeMap::new();
        for cell in &cells.entries {
            let position = &cell.position;
            let block = cells.block(cell);
            let (region, source) = cells.regions[cell.region];
            if matches!(
                block.name.as_str(),
                "minecraft:barrier" | "minecraft:light" | "minecraft:structure_void"
            ) {
                continue;
            }
            if fluids::is_fluid(block) {
                continue;
            }
            let mut state = states[cell.region][cell.palette as usize]
                .get_or_insert_with(|| builder.state(block))
                .clone();
            let local = std::array::from_fn(|i| position[i] - source.origin[i]);
            let data = source.block_entities.get(&local);
            if (data.is_some() && attachments::supported(block))
                || block.name == "minecraft:spawner"
            {
                let key = (
                    block.clone(),
                    data.map(fastsnbt::to_string)
                        .transpose()
                        .map_err(|e| e.to_string())?
                        .unwrap_or_default(),
                );
                if let Some(cached) = attached.get(&key) {
                    state = cached.clone();
                } else {
                    let modified = std::sync::Arc::make_mut(&mut state);
                    if let Some(data) = data {
                        for choices in &mut modified.parts {
                            for (mesh, _) in choices {
                                let mut geometry = builder.meshes[*mesh].clone();
                                if let Err(message) =
                                    attachments::decorate(self, block, data, &mut geometry)
                                {
                                    modified.messages.push(message);
                                }
                                *mesh = builder.meshes.len();
                                builder.meshes.push(geometry);
                            }
                        }
                    }
                    match attachments::contents(&mut builder, block, data, document.registry()?) {
                        Ok(mesh) if !mesh.quads.is_empty() => {
                            if block.name == "minecraft:moving_piston" {
                                modified.parts.clear();
                            }
                            let index = builder.meshes.len();
                            builder.meshes.push(mesh);
                            modified.parts.push(vec![(index, 1)]);
                        }
                        Err(message) => modified.messages.push(message),
                        _ => (),
                    }
                    modified.refresh_metadata(&builder.meshes);
                    attached.insert(key, state.clone());
                }
                decorated.insert(*position, state.clone());
            }
            let text = &state.name;
            if state.tinted && tinted.insert(block) {
                diagnostics.push(Diagnostic {
                    region: region.clone(),
                    position: *position,
                    block: text.clone(),
                    message: "Tint indices are preserved; tint colors are not evaluated".into(),
                });
            }
            for message in &state.messages {
                diagnostics.push(Diagnostic {
                    region: region.clone(),
                    position: *position,
                    block: text.clone(),
                    message: message.clone(),
                });
            }
            if state
                .selected_meshes(*position)
                .any(|mesh| builder.meshes[mesh].occludes)
            {
                occlusion.insert(*position);
            }
        }
        for cell in &cells.entries {
            let position = cell.position;
            let Some(state) = decorated
                .get(&position)
                .or(states[cell.region][cell.palette as usize].as_ref())
            else {
                continue;
            };
            if state.fully_cullable && occlusion.encloses(position) {
                continue;
            }
            let block = cells.block(cell);
            let display = matches!(
                block.name.as_str(),
                "minecraft:spawner" | "minecraft:trial_spawner"
            ) && state.parts.len() > 1;
            let mut draws = Vec::new();
            for (part, draw) in state
                .draws(position, &builder.meshes, &occlusion)
                .enumerate()
            {
                if draw.quads.is_empty() {
                    continue;
                }
                if display && part == state.parts.len() - 1 {
                    instances.push(Instance {
                        is_entity: true,
                        position: position.map(f64::from),
                        rotation: [0., 0., 0., 1.],
                        name: format!("{} display", state.name),
                        draws: vec![draw],
                    });
                } else {
                    draws.push(draw);
                }
            }
            if !draws.is_empty() {
                instances.push(Instance {
                    is_entity: false,
                    position: position.map(f64::from),
                    rotation: [0., 0., 0., 1.],
                    name: state.name.clone(),
                    draws,
                });
            }
        }
        fluids::append(
            self,
            &cells,
            &occlusion,
            &mut builder.meshes,
            &mut instances,
        )?;
        let mut entity_meshes: BTreeMap<String, Result<usize>> = BTreeMap::new();
        for (region, entity, position) in selected_entities {
            let result = entities::facing(entity).and_then(|rotation| {
                let mesh = entity_meshes
                    .entry(entities::id(entity).into())
                    .or_insert_with(|| {
                        let mesh = entities::bake(self, entities::id(entity))?;
                        let id = builder.meshes.len();
                        builder.meshes.push(mesh);
                        Ok(id)
                    })
                    .clone()?;
                Ok(entities::instance(
                    entities::id(entity),
                    position,
                    rotation,
                    mesh,
                    builder.meshes[mesh].quads.len(),
                ))
            });
            match result {
                Ok(instance) => instances.push(instance),
                Err(message) => diagnostics.push(Diagnostic {
                    region: region.clone(),
                    position: position.map(|v| v.floor() as i32),
                    block: entities::id(entity).into(),
                    message,
                }),
            }
        }
        Ok(PreparedScene {
            atlases: self.atlases.clone(),
            atlas_images: self.atlas_images.clone(),
            textures: self.textures.clone(),
            meshes: builder.meshes,
            instances,
            diagnostics,
        })
    }
}
