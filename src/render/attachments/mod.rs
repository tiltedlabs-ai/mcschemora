mod signs;

use super::{
    GeometryAssets, Mesh, Quad, Vertex,
    geometry::{normal, rotate},
    models::Builder,
};
use crate::{
    Result,
    model::{Block, Compound},
};
use fastnbt::Value;
use std::collections::BTreeMap;

pub(super) fn supported(block: &Block) -> bool {
    block.name.ends_with("_sign")
        || block.name.ends_with("_banner")
        || matches!(
            block.name.as_str(),
            "minecraft:decorated_pot"
                | "minecraft:spawner"
                | "minecraft:trial_spawner"
                | "minecraft:campfire"
                | "minecraft:soul_campfire"
                | "minecraft:vault"
                | "minecraft:suspicious_sand"
                | "minecraft:suspicious_gravel"
                | "minecraft:beacon"
                | "minecraft:moving_piston"
        )
}

fn compound(value: Option<&Value>) -> Option<&Compound> {
    match value {
        Some(Value::Compound(v)) => Some(v),
        _ => None,
    }
}
fn string(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(v)) => Some(v),
        _ => None,
    }
}
fn list(value: Option<&Value>) -> &[Value] {
    match value {
        Some(Value::List(v)) => v,
        _ => &[],
    }
}
fn number(value: Option<&Value>) -> f32 {
    match value {
        Some(Value::Byte(v)) => *v as f32,
        Some(Value::Short(v)) => *v as f32,
        Some(Value::Int(v)) => *v as f32,
        Some(Value::Float(v)) => *v,
        Some(Value::Double(v)) => *v as f32,
        _ => 0.,
    }
}
pub(super) fn dye(name: &str) -> [u8; 4] {
    let value: u32 = match name {
        "orange" => 0xf9801d,
        "magenta" => 0xc74ebd,
        "light_blue" => 0x3ab3da,
        "yellow" => 0xfed83d,
        "lime" => 0x80c71f,
        "pink" => 0xf38baa,
        "gray" => 0x474f52,
        "light_gray" => 0x9d9d97,
        "cyan" => 0x169c9c,
        "purple" => 0x8932b8,
        "blue" => 0x3c44aa,
        "brown" => 0x835432,
        "green" => 0x5e7c16,
        "red" => 0xb02e26,
        "black" => 0x1d1d21,
        _ => 0xf9fffe,
    };
    [(value >> 16) as u8, (value >> 8) as u8, value as u8, 255]
}
fn transform(mesh: &mut Mesh, scale: f32, position: [f32; 3], angle: f32) {
    for q in &mut mesh.quads {
        q.normal = rotate(q.normal, 1, angle, [0.; 3], false);
        for v in &mut q.vertices {
            v.position = rotate(v.position, 1, angle, [0.5; 3], false);
            v.position = std::array::from_fn(|i| v.position[i] * scale + position[i]);
        }
    }
}
pub(super) fn decorate(
    assets: &GeometryAssets,
    block: &Block,
    data: &Compound,
    mesh: &mut Mesh,
) -> Result<()> {
    if block.name.ends_with("_sign") {
        signs::bake(assets, block, data, mesh)?;
    }
    if block.name.ends_with("_banner") {
        let flag: Vec<_> = mesh
            .quads
            .iter()
            .filter(|q| assets.textures[q.texture].name == "minecraft:entity/banner/base")
            .cloned()
            .collect();
        for (layer, pattern) in list(data.get("patterns")).iter().take(16).enumerate() {
            let Some(p) = compound(Some(pattern)) else {
                continue;
            };
            let id = string(p.get("pattern"))
                .unwrap_or("base")
                .trim_start_matches("minecraft:");
            let name = format!("minecraft:entity/banner/{id}");
            let texture = *assets
                .texture_ids
                .get(&name)
                .ok_or_else(|| format!("Unknown banner pattern {id}"))?;
            for mut q in flag.clone() {
                q.texture = texture;
                q.color = dye(string(p.get("color")).unwrap_or("white"));
                for v in &mut q.vertices {
                    for i in 0..3 {
                        v.position[i] += q.normal[i] * 0.0001 * (layer + 1) as f32;
                    }
                }
                mesh.quads.push(q);
            }
        }
    }
    if block.name == "minecraft:decorated_pot" {
        let sherds = list(data.get("sherds"));
        for q in &mut mesh.quads {
            if assets.textures[q.texture].name
                != "minecraft:entity/decorated_pot/decorated_pot_side"
            {
                continue;
            }
            let angle = match block.properties.get("facing").map(String::as_str) {
                Some("east") => 90.,
                Some("south") => 180.,
                Some("west") => 270.,
                _ => 0.,
            };
            let n = rotate(q.normal, 1, angle, [0.; 3], false);
            let index = if n[2] < -0.5 {
                0
            } else if n[0] > 0.5 {
                1
            } else if n[0] < -0.5 {
                2
            } else {
                3
            };
            let Some(id) = sherds.get(index).and_then(|v| string(Some(v))) else {
                continue;
            };
            if let Some(pattern) = id
                .trim_start_matches("minecraft:")
                .strip_suffix("_pottery_sherd")
            {
                let name = format!("minecraft:entity/decorated_pot/{pattern}_pottery_pattern");
                q.texture = *assets
                    .texture_ids
                    .get(&name)
                    .ok_or_else(|| format!("Unknown pottery sherd {id}"))?;
            }
        }
    }
    Ok(())
}

fn item(
    builder: &mut Builder<'_>,
    registry: &crate::registry::Registry,
    data: &Compound,
) -> Result<Mesh> {
    let id = string(data.get("id")).unwrap_or("minecraft:air");
    if id == "minecraft:air" {
        return Ok(Mesh {
            quads: Vec::new(),
            occludes: false,
        });
    }
    if builder.assets.states.contains_key(id) {
        let state = builder.state(&registry.resolve(&Block::new(id, BTreeMap::new())?)?);
        return Ok(Mesh {
            quads: state
                .parts
                .iter()
                .filter_map(|p| p.first())
                .flat_map(|(m, _)| builder.meshes[*m].quads.clone())
                .collect(),
            occludes: false,
        });
    }
    let name = format!("minecraft:item/{}", id.trim_start_matches("minecraft:"));
    let texture = *builder
        .assets
        .texture_ids
        .get(&name)
        .ok_or_else(|| format!("No display texture for item {id}"))?;
    let positions = [[0., 0., 0.5], [0., 1., 0.5], [1., 1., 0.5], [1., 0., 0.5]];
    let quad = Quad {
        vertices: std::array::from_fn(|i| Vertex {
            position: positions[i],
            uv: [[0., 1.], [0., 0.], [1., 0.], [1., 1.]][i],
        }),
        normal: normal(positions).unwrap(),
        texture,
        color: [255; 4],
        shade: true,
        tint_index: None,
        texture_flags: serde_json::json!({"force_cutout":true}),
        cull_face: None,
    };
    let mut back = quad.clone();
    back.vertices.reverse();
    back.normal = back.normal.map(|v| -v);
    Ok(Mesh {
        quads: vec![quad, back],
        occludes: false,
    })
}

pub(super) fn contents(
    builder: &mut Builder<'_>,
    block: &Block,
    data: Option<&Compound>,
    registry: &crate::registry::Registry,
) -> Result<Mesh> {
    let mut mesh = Mesh {
        quads: Vec::new(),
        occludes: false,
    };
    let empty = Compound::new();
    let data = data.unwrap_or(&empty);
    if block.name == "minecraft:moving_piston"
        && let Some(state) = compound(data.get("blockState"))
    {
        let id = string(state.get("Name")).unwrap_or("minecraft:air");
        if id == "minecraft:moving_piston" {
            return Err("Moving piston contains another moving piston".into());
        }
        let properties = compound(state.get("Properties"))
            .map(|p| {
                p.iter()
                    .filter_map(|(k, v)| string(Some(v)).map(|v| (k.clone(), v.into())))
                    .collect()
            })
            .unwrap_or_default();
        let state = builder.state(&registry.resolve(&Block::new(id, properties)?)?);
        mesh.quads = state
            .parts
            .iter()
            .filter_map(|p| p.first())
            .flat_map(|(m, _)| builder.meshes[*m].quads.clone())
            .collect();
        let direction = [
            [0., -1., 0.],
            [0., 1., 0.],
            [0., 0., -1.],
            [0., 0., 1.],
            [-1., 0., 0.],
            [1., 0., 0.],
        ][(number(data.get("facing")) as usize).min(5)];
        let progress = if data.contains_key("progress") {
            number(data.get("progress")).clamp(0., 1.)
        } else {
            0.5
        };
        let displacement = if number(data.get("extending")) != 0. {
            progress - 1.
        } else {
            1. - progress
        };
        transform(&mut mesh, 1., direction.map(|v| v * displacement), 0.);
    }
    if block.name == "minecraft:beacon"
        && number(data.get("Levels")) > 0.
        && let Some(&texture) = builder
            .assets
            .texture_ids
            .get("minecraft:entity/beacon_beam")
    {
        for face in ["north", "south", "west", "east"] {
            let positions = super::geometry::corners(face, [0.4, 1., 0.4], [0.6, 17., 0.6])?;
            mesh.quads.push(Quad {
                vertices: std::array::from_fn(|i| Vertex {
                    position: positions[i],
                    uv: super::geometry::UV_CORNERS[i],
                }),
                normal: normal(positions).unwrap(),
                texture,
                color: [255; 4],
                shade: false,
                tint_index: None,
                texture_flags: serde_json::Value::Null,
                cull_face: None,
            });
        }
    }
    if block.name == "minecraft:spawner" || block.name == "minecraft:trial_spawner" {
        let spawn = compound(data.get("SpawnData")).or_else(|| compound(data.get("spawn_data")));
        let entity = spawn.and_then(|s| compound(s.get("entity")));
        let id =
            entity
                .and_then(|e| string(e.get("id")))
                .or(if block.name == "minecraft:spawner" {
                    Some("minecraft:pig")
                } else {
                    None
                });
        if let Some(id) = id {
            let mut mob = super::entities::bake(builder.assets, id)?;
            transform(&mut mob, 0.35, [0.5, 0.2, 0.5], 45.);
            mesh.quads.extend(mob.quads);
        }
    }
    if matches!(
        block.name.as_str(),
        "minecraft:campfire" | "minecraft:soul_campfire"
    ) {
        for (i, value) in list(data.get("Items")).iter().take(4).enumerate() {
            if let Some(data) = compound(Some(value)) {
                let mut model = item(builder, registry, data)?;
                for q in &mut model.quads {
                    q.normal = rotate(q.normal, 0, 90., [0.; 3], false);
                    for v in &mut q.vertices {
                        v.position = rotate(v.position, 0, 90., [0.5; 3], false);
                    }
                }
                let slot = if data.contains_key("Slot") {
                    number(data.get("Slot")) as usize
                } else {
                    i
                };
                transform(
                    &mut model,
                    0.25,
                    [
                        0.125 + (slot % 2) as f32 * 0.5,
                        0.35,
                        0.125 + (slot / 2) as f32 * 0.5,
                    ],
                    0.,
                );
                mesh.quads.extend(model.quads);
            }
        }
    }
    let display = if block.name == "minecraft:vault" {
        compound(data.get("shared_data")).and_then(|d| compound(d.get("display_item")))
    } else if matches!(
        block.name.as_str(),
        "minecraft:suspicious_sand" | "minecraft:suspicious_gravel"
    ) {
        if number(
            block
                .properties
                .get("dusted")
                .and_then(|v| v.parse::<i32>().ok())
                .map(Value::Int)
                .as_ref(),
        ) > 0.
        {
            compound(data.get("item"))
        } else {
            None
        }
    } else {
        None
    };
    if let Some(data) = display {
        let mut model = item(builder, registry, data)?;
        transform(
            &mut model,
            0.35,
            [
                0.325,
                0.35,
                if block.name == "minecraft:vault" {
                    0.325
                } else {
                    0.88
                },
            ],
            0.,
        );
        mesh.quads.extend(model.quads);
    }
    Ok(mesh)
}
