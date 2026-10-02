use super::Application;
use crate::{
    Result,
    model::Pos,
    render::{
        AlphaMode, GeometryAssets, Mesh, Quad, Vertex,
        geometry::{UV_CORNERS, corners, normal, rotate},
    },
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Model {
    #[serde(default)]
    elements: Vec<Element>,
}
#[derive(Deserialize)]
struct Element {
    from: [f32; 3],
    to: [f32; 3],
    #[serde(default)]
    rotation: Option<Rotation>,
    #[serde(default = "yes")]
    shade: bool,
    faces: BTreeMap<String, Face>,
}
fn yes() -> bool {
    true
}
#[derive(Deserialize)]
struct Rotation {
    origin: [f32; 3],
    axis: String,
    angle: f32,
    #[serde(default)]
    rescale: bool,
}
#[derive(Deserialize)]
struct Face {
    texture: String,
    uv: [f32; 4],
    #[serde(default)]
    rotation: i32,
    #[serde(default)]
    cullface: Option<String>,
    #[serde(default)]
    tintindex: Option<i32>,
    #[serde(default)]
    texture_flags: Value,
}

pub(super) fn bake(assets: &GeometryAssets, application: &Application) -> Result<Mesh> {
    if application.x % 90 != 0 || application.y % 90 != 0 {
        return Err("Blockstate rotation must be a multiple of 90 degrees".into());
    }
    let model: Model = serde_json::from_value(
        assets
            .models
            .get(&application.model)
            .ok_or("Missing visual model")?
            .clone(),
    )
    .map_err(|e| e.to_string())?;
    let mut quads = Vec::new();
    let mut occludes = false;
    for element in model.elements {
        if !element
            .from
            .iter()
            .chain(&element.to)
            .all(|v| v.is_finite())
        {
            return Err("Invalid model element bounds".into());
        }
        let full_cube =
            element.from == [0.; 3] && element.to == [16.; 3] && element.rotation.is_none();
        let mut opaque_faces = 0;
        for (direction, face) in &element.faces {
            if face.rotation % 90 != 0 || !face.uv.iter().all(|v| v.is_finite()) {
                return Err("Invalid model face UV rotation".into());
            }
            let texture = *assets
                .texture_ids
                .get(&face.texture)
                .ok_or_else(|| format!("Missing visual texture {}", face.texture))?;
            if assets.textures[texture].alpha == AlphaMode::Opaque
                && face.texture_flags["force_translucent"] != true
            {
                opaque_faces += 1;
            }
            if let Some(quad) = bake_face(&element, direction, face, application, texture)? {
                quads.push(quad);
            }
        }
        occludes |= full_cube && opaque_faces == 6;
    }
    Ok(Mesh { quads, occludes })
}

fn bake_face(
    element: &Element,
    direction: &str,
    face: &Face,
    application: &Application,
    texture: usize,
) -> Result<Option<Quad>> {
    let mut positions = corners(
        direction,
        element.from.map(|v| v / 16.),
        element.to.map(|v| v / 16.),
    )?;
    let original = corners(direction, [0.; 3], [1.; 3])?;
    let mut uv = face_uv(face);
    for position in &mut positions {
        if let Some(rotation) = &element.rotation {
            *position = rotate_element(*position, rotation)?;
        }
        *position = rotate_state(*position, application);
    }
    let Some(normal) = normal(positions) else {
        return Ok(None);
    };
    if application.uvlock {
        uv = lock_uv(uv, original, application)?;
    }
    let cull_face = cull_face(face, positions, application)?;
    Ok(Some(Quad {
        vertices: std::array::from_fn(|i| Vertex {
            position: positions[i],
            uv: uv[i],
        }),
        normal,
        texture,
        tint_index: face.tintindex.filter(|&index| index >= 0),
        shade: element.shade,
        color: [255; 4],
        texture_flags: face.texture_flags.clone(),
        cull_face,
    }))
}

fn face_uv(face: &Face) -> [[f32; 2]; 4] {
    std::array::from_fn(|i| {
        let corner = UV_CORNERS[(i + (face.rotation.rem_euclid(360) / 90) as usize) % 4];
        [
            (face.uv[0] + corner[0] * (face.uv[2] - face.uv[0])) / 16.,
            (face.uv[1] + corner[1] * (face.uv[3] - face.uv[1])) / 16.,
        ]
    })
}

fn cull_face(
    face: &Face,
    positions: [[f32; 3]; 4],
    application: &Application,
) -> Result<Option<Pos>> {
    Ok(face
        .cullface
        .as_ref()
        .map(|direction| {
            crate::model::direction(if direction == "bottom" {
                "down"
            } else if direction == "top" {
                "up"
            } else {
                direction
            })
        })
        .transpose()?
        .map(|direction| {
            rotate_state(direction.map(|v| v as f32 + 0.5), application)
                .map(|v| (v - 0.5).round() as i32)
        })
        .filter(|direction| boundary_face(positions, *direction)))
}

fn rotate_element(p: [f32; 3], rotation: &Rotation) -> Result<[f32; 3]> {
    let axis = match rotation.axis.as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return Err("Invalid model rotation axis".into()),
    };
    if !rotation.origin.iter().all(|v| v.is_finite())
        || ![0., 22.5, 45.].contains(&rotation.angle.abs())
    {
        return Err("Invalid model element rotation".into());
    }
    Ok(rotate(
        p,
        axis,
        rotation.angle,
        rotation.origin.map(|v| v / 16.),
        rotation.rescale,
    ))
}

fn rotate_state(p: [f32; 3], application: &Application) -> [f32; 3] {
    rotate(
        rotate(
            p,
            0,
            -(application.x.rem_euclid(360) as f32),
            [0.5; 3],
            false,
        ),
        1,
        -(application.y.rem_euclid(360) as f32),
        [0.5; 3],
        false,
    )
}

fn lock_uv(
    uv: [[f32; 2]; 4],
    original: [[f32; 3]; 4],
    application: &Application,
) -> Result<[[f32; 2]; 4]> {
    let rotated = original.map(|p| rotate_state(p, application));
    let n = normal(rotated)
        .ok_or("Invalid UV lock normal")?
        .map(|v| v.round() as i32);
    let canonical = corners(crate::model::direction_name(n), [0.; 3], [1.; 3])?;
    let mut mapped = [[0.; 2]; 4];
    for i in 0..4 {
        let j = canonical
            .iter()
            .position(|p| (0..3).all(|axis| (p[axis] - rotated[i][axis]).abs() < 1e-4))
            .ok_or("Invalid UV lock rotation")?;
        mapped[i] = UV_CORNERS[j];
    }
    Ok(uv.map(|[u, v]| {
        std::array::from_fn(|axis| {
            mapped[0][axis]
                + u * (mapped[3][axis] - mapped[0][axis])
                + v * (mapped[1][axis] - mapped[0][axis])
        })
    }))
}

fn boundary_face(positions: [[f32; 3]; 4], direction: Pos) -> bool {
    let Some(axis) = direction.iter().position(|&n| n != 0) else {
        return false;
    };
    let plane = if direction[axis] > 0 { 1. } else { 0. };
    positions
        .iter()
        .all(|p| (p[axis] - plane).abs() < 1e-5 && p.iter().all(|v| *v >= -1e-5 && *v <= 1.00001))
}
