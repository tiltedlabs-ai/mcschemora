use super::{
    GeometryAssets, Mesh, Quad, Vertex,
    geometry::{normal, rotate},
};
use crate::{Result, model::Block};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Deserialize)]
struct Catalog {
    states: BTreeMap<String, Vec<Application>>,
    models: BTreeMap<String, Vec<Face>>,
}

#[derive(Deserialize)]
struct Application {
    when: serde_json::Value,
    model: String,
    x: f32,
    y: f32,
}

#[derive(Deserialize)]
struct Face {
    positions: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    texture: String,
    color: [u8; 4],
    shade: bool,
}

fn catalog() -> Result<&'static Catalog> {
    static CATALOG: LazyLock<Result<Catalog>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../data/block-models/models.json"))
            .map_err(|e| format!("Invalid special block models: {e}"))
    });
    CATALOG.as_ref().map_err(Clone::clone)
}

pub(super) fn contains(block: &Block) -> bool {
    catalog().is_ok_and(|c| c.states.contains_key(&block.id))
}

pub(super) fn overlay(block: &Block) -> bool {
    matches!(
        block.id.as_str(),
        "minecraft:bell" | "minecraft:enchanting_table" | "minecraft:lectern"
    )
}

pub(super) fn bake(assets: &GeometryAssets, block: &Block) -> Result<Mesh> {
    let catalog = catalog()?;
    let mut quads = Vec::new();
    for app in &catalog.states[&block.id] {
        if !super::models::condition(&app.when, block)? {
            continue;
        }
        for face in &catalog.models[&app.model] {
            let positions = face.positions.map(|p| {
                rotate(
                    rotate(p, 0, -app.x, [0.5; 3], false),
                    1,
                    -app.y,
                    [0.5; 3],
                    false,
                )
            });
            let Some(normal) = normal(positions) else {
                continue;
            };
            let texture = *assets
                .texture_ids
                .get(&face.texture)
                .ok_or_else(|| format!("Missing special block texture {}", face.texture))?;
            quads.push(Quad {
                vertices: std::array::from_fn(|i| Vertex {
                    position: positions[i],
                    uv: face.uv[i],
                }),
                normal,
                texture,
                color: face.color,
                shade: face.shade,
                tint_index: None,
                texture_flags: serde_json::json!({"force_cutout":true}),
                cull_face: None,
            });
        }
    }
    Ok(Mesh {
        quads,
        occludes: false,
    })
}
