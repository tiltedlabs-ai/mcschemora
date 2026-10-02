use super::geometry::{DIRECTIONS, UV_CORNERS, corners, normal, offset};
use super::occlusion::Occlusion;
use super::{Draw, GeometryAssets, Instance, Mesh, Quad, Vertex};
use crate::{
    Result,
    model::{Block, Position},
};
use std::collections::BTreeMap;

pub(super) fn is_fluid(block: &Block) -> bool {
    matches!(
        block.name.as_str(),
        "minecraft:water" | "minecraft:lava" | "minecraft:bubble_column"
    )
}

fn kind(block: &Block) -> Option<&str> {
    if matches!(
        block.name.as_str(),
        "minecraft:bubble_column"
            | "minecraft:kelp"
            | "minecraft:kelp_plant"
            | "minecraft:seagrass"
            | "minecraft:tall_seagrass"
    ) {
        Some("minecraft:water")
    } else if is_fluid(block) {
        Some(block.name.as_str())
    } else if block
        .properties
        .get("waterlogged")
        .is_some_and(|v| v == "true")
    {
        Some("minecraft:water")
    } else {
        None
    }
}

pub(super) fn append(
    assets: &GeometryAssets,
    cells: &super::cells::Cells<'_>,
    occlusion: &Occlusion,
    meshes: &mut Vec<Mesh>,
    instances: &mut Vec<Instance>,
) -> Result<()> {
    let mut cache = BTreeMap::new();
    for cell in &cells.entries {
        let position = cell.position;
        let block = cells.block(cell);
        let Some(fluid_kind) = kind(block) else {
            continue;
        };
        let same = |p: Position| {
            cells
                .get(&p)
                .is_some_and(|other| kind(other) == Some(fluid_kind))
        };
        let above = |p| offset(p, [0, 1, 0]).is_some_and(same);
        let heights: [f32; 4] = std::array::from_fn(|corner| {
            if above(position) {
                return 1.;
            }
            let mut sum = 0.;
            let mut weight = 0.;
            for dx in [corner as i32 % 2 - 1, corner as i32 % 2] {
                for dz in [corner as i32 / 2 - 1, corner as i32 / 2] {
                    let Some(p) = offset(position, [dx, 0, dz]) else {
                        continue;
                    };
                    if same(p) {
                        if above(p) {
                            return 1.;
                        }
                        let level = cells
                            .get(&p)
                            .unwrap()
                            .properties
                            .get("level")
                            .and_then(|v| v.parse::<u8>().ok())
                            .unwrap_or(0);
                        let height = if level >= 8 {
                            8. / 9.
                        } else {
                            (8 - level) as f32 / 9.
                        };
                        let w = if level == 0 || level >= 8 { 10. } else { 1. };
                        sum += height * w;
                        weight += w;
                    } else if !occlusion.contains(&p) {
                        weight += 1.;
                    }
                }
            }
            sum / weight
        });
        let visible: [bool; 6] = std::array::from_fn(|i| {
            let direction = crate::model::direction(DIRECTIONS[i]).unwrap();
            let Some(p) = offset(position, direction) else {
                return true;
            };
            !same(p) && !(occlusion.contains(&p) && (i != 1 || heights.iter().all(|&h| h == 1.)))
        });
        if !visible.iter().any(|&v| v) {
            continue;
        }
        let water = fluid_kind == "minecraft:water";
        let key = (water, heights.map(f32::to_bits), visible);
        let mesh = if let Some(&id) = cache.get(&key) {
            id
        } else {
            let fluid = if water { "water" } else { "lava" };
            let texture = |flow| {
                let name = format!(
                    "minecraft:block/{fluid}_{}",
                    if flow { "flow" } else { "still" }
                );
                assets
                    .texture_ids
                    .get(&name)
                    .copied()
                    .ok_or_else(|| format!("Missing fluid texture {name}"))
            };
            let mut quads = Vec::new();
            for (i, direction) in DIRECTIONS.iter().enumerate() {
                if !visible[i] {
                    continue;
                }
                let mut positions = corners(direction, [0.; 3], [1.; 3])?;
                for p in &mut positions {
                    if p[1] == 1. {
                        p[1] = heights[p[2] as usize * 2 + p[0] as usize];
                    }
                }
                let flow = i >= 2 || (i == 1 && heights.iter().any(|h| *h != heights[0]));
                let vertices = std::array::from_fn(|j| {
                    let mut uv = UV_CORNERS[j];
                    if i >= 2 {
                        uv[1] = 1. - positions[j][1];
                    }
                    if flow {
                        uv = uv.map(|v| v * 0.5);
                        if i == 1 {
                            let dx = heights[0] + heights[2] - heights[1] - heights[3];
                            let dz = heights[0] + heights[1] - heights[2] - heights[3];
                            let angle = dz.atan2(dx) - std::f32::consts::FRAC_PI_2;
                            let (sin, cos) = angle.sin_cos();
                            let x = positions[j][0] - 0.5;
                            let z = positions[j][2] - 0.5;
                            uv = [
                                0.5 + (x * cos - z * sin) * 0.5,
                                0.5 + (x * sin + z * cos) * 0.5,
                            ];
                        }
                    }
                    let mut position = positions[j];
                    if i >= 2 {
                        let normal = crate::model::direction(direction).unwrap();
                        for axis in [0, 2] {
                            position[axis] -= normal[axis] as f32 * 0.0001;
                        }
                    }
                    Vertex { position, uv }
                });
                quads.push(Quad {
                    vertices,
                    normal: normal(positions).ok_or("Degenerate fluid face")?,
                    texture: texture(flow)?,
                    tint_index: None,
                    shade: water,
                    color: if water { [63, 118, 228, 255] } else { [255; 4] },
                    texture_flags: if water {
                        serde_json::json!({"force_translucent":true})
                    } else {
                        serde_json::Value::Null
                    },
                    cull_face: None,
                });
            }
            let id = meshes.len();
            meshes.push(Mesh {
                quads,
                occludes: false,
            });
            cache.insert(key, id);
            id
        };
        instances.push(Instance {
            is_entity: false,
            position: position.map(f64::from),
            rotation: [0., 0., 0., 1.],
            name: block.text(),
            draws: vec![Draw {
                mesh,
                quads: (0..meshes[mesh].quads.len()).collect(),
            }],
        });
    }
    Ok(())
}
