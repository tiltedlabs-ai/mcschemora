use super::{AlphaMode, GeometryAssets, Mesh, Quad, Vertex};
use crate::{
    Result,
    model::{Block, Pos},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
struct Application {
    model: String,
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
    #[serde(default)]
    uvlock: bool,
    #[serde(default = "one")]
    weight: u32,
}
fn one() -> u32 {
    1
}

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

#[derive(Clone)]
pub(super) struct StateGeometry {
    pub(super) parts: Vec<Vec<(usize, u32)>>,
    pub(super) messages: Vec<String>,
}
pub(super) struct Builder<'a> {
    pub(super) assets: &'a GeometryAssets,
    pub(super) meshes: Vec<Mesh>,
    pub(super) applications: BTreeMap<(String, i32, i32, bool), usize>,
    pub(super) states: BTreeMap<Block, StateGeometry>,
}

fn property_matches(actual: Option<&String>, expected: &str) -> bool {
    let (invert, expected) = expected
        .strip_prefix('!')
        .map_or((false, expected), |v| (true, v));
    let matches = actual.is_some_and(|actual| expected.split('|').any(|value| value == actual));
    if invert { !matches } else { matches }
}

fn condition(value: &Value, block: &Block) -> Result<bool> {
    let fields = value.as_object().ok_or("Invalid multipart condition")?;
    let mut matches = true;
    for (key, value) in fields {
        let result = if key == "OR" || key == "AND" {
            let items = value
                .as_array()
                .ok_or("Invalid multipart logical condition")?;
            let values = items
                .iter()
                .map(|item| condition(item, block))
                .collect::<Result<Vec<_>>>()?;
            if key == "OR" {
                values.into_iter().any(|v| v)
            } else {
                values.into_iter().all(|v| v)
            }
        } else {
            let expected = match value {
                Value::String(value) => value.clone(),
                Value::Bool(value) => value.to_string(),
                Value::Number(value) => value.to_string(),
                _ => return Err("Invalid multipart property".into()),
            };
            property_matches(block.properties.get(key), &expected)
        };
        matches &= result;
    }
    Ok(matches)
}

fn applications(value: &Value) -> Result<Vec<Application>> {
    let values: Vec<Application> = if value.is_array() {
        serde_json::from_value(value.clone())
    } else {
        serde_json::from_value(value.clone()).map(|v| vec![v])
    }
    .map_err(|e| e.to_string())?;
    if values.is_empty() || values.iter().any(|a| a.weight == 0) {
        return Err("Model choices require positive weights".into());
    }
    Ok(values)
}

impl Builder<'_> {
    pub(super) fn state(&mut self, block: &Block) -> StateGeometry {
        if let Some(state) = self.states.get(block) {
            return state.clone();
        }
        let state = match self.compile_state(block) {
            Ok(parts) => StateGeometry {
                parts,
                messages: Vec::new(),
            },
            Err(message) => {
                let key = ("__placeholder__".into(), 0, 0, false);
                let mesh = if let Some(&mesh) = self.applications.get(&key) {
                    mesh
                } else {
                    let mesh = self.meshes.len();
                    let texture = self.assets.texture_ids["minecraft:missingno"];
                    let quads = DIRECTIONS
                        .iter()
                        .map(|name| {
                            let positions = corners(name, [0.; 3], [1.; 3]).unwrap();
                            Quad {
                                vertices: std::array::from_fn(|i| Vertex {
                                    position: positions[i],
                                    uv: UV_CORNERS[i],
                                }),
                                normal: normal(positions).unwrap(),
                                texture,
                                tint_index: None,
                                shade: true,
                                texture_flags: Value::Null,
                                cull_face: None,
                            }
                        })
                        .collect();
                    self.meshes.push(Mesh {
                        quads,
                        occludes: false,
                    });
                    self.applications.insert(key, mesh);
                    mesh
                };
                StateGeometry {
                    parts: vec![vec![(mesh, 1)]],
                    messages: vec![format!("{message}; showing missing-geometry placeholder")],
                }
            }
        };
        self.states.insert(block.clone(), state.clone());
        state
    }

    fn compile_state(&mut self, block: &Block) -> Result<Vec<Vec<(usize, u32)>>> {
        let state = self
            .assets
            .states
            .get(&block.name)
            .ok_or_else(|| format!("No blockstate visuals for {}", block.name))?;
        let mut parts = Vec::new();
        if let Some(variants) = state.get("variants") {
            let mut selected = None;
            for (key, value) in variants.as_object().ok_or("Invalid blockstate variants")? {
                let mut matches = true;
                for property in key.split(',').filter(|s| !s.is_empty()) {
                    let (key, value) = property
                        .split_once('=')
                        .ok_or("Invalid blockstate variant predicate")?;
                    matches &= property_matches(block.properties.get(key), value);
                }
                if matches {
                    if selected.is_some() {
                        return Err("Ambiguous blockstate variants".into());
                    }
                    selected = Some(value);
                }
            }
            parts.push(applications(
                selected.ok_or("No matching blockstate variant")?,
            )?);
        }
        if let Some(multipart) = state.get("multipart") {
            for part in multipart.as_array().ok_or("Invalid multipart blockstate")? {
                if part
                    .get("when")
                    .map(|when| condition(when, block))
                    .transpose()?
                    .unwrap_or(true)
                {
                    parts.push(applications(
                        part.get("apply").ok_or("Missing multipart application")?,
                    )?);
                }
            }
        }
        if parts.is_empty() {
            return Err("Blockstate has no matching geometry".into());
        }
        parts
            .into_iter()
            .map(|part| {
                part.into_iter()
                    .map(|application| {
                        self.mesh(&application)
                            .map(|mesh| (mesh, application.weight))
                    })
                    .collect()
            })
            .collect()
    }

    fn mesh(&mut self, application: &Application) -> Result<usize> {
        let key = (
            application.model.clone(),
            application.x,
            application.y,
            application.uvlock,
        );
        if let Some(&mesh) = self.applications.get(&key) {
            return Ok(mesh);
        }
        if application.x % 90 != 0 || application.y % 90 != 0 {
            return Err("Blockstate rotation must be a multiple of 90 degrees".into());
        }
        let model: Model = serde_json::from_value(
            self.assets
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
                let texture = *self
                    .assets
                    .texture_ids
                    .get(&face.texture)
                    .ok_or_else(|| format!("Missing visual texture {}", face.texture))?;
                if self.assets.textures[texture].alpha == AlphaMode::Opaque
                    && face.texture_flags["force_translucent"] != true
                {
                    opaque_faces += 1;
                }
                let mut positions = corners(
                    direction,
                    element.from.map(|v| v / 16.),
                    element.to.map(|v| v / 16.),
                )?;
                let original = corners(direction, [0.; 3], [1.; 3])?;
                let mut uv: [[f32; 2]; 4] = std::array::from_fn(|i| {
                    let corner =
                        UV_CORNERS[(i + (face.rotation.rem_euclid(360) / 90) as usize) % 4];
                    [
                        (face.uv[0] + corner[0] * (face.uv[2] - face.uv[0])) / 16.,
                        (face.uv[1] + corner[1] * (face.uv[3] - face.uv[1])) / 16.,
                    ]
                });
                for position in &mut positions {
                    if let Some(rotation) = &element.rotation {
                        *position = rotate_element(*position, rotation)?;
                    }
                    *position = rotate_state(*position, application);
                }
                let Some(normal) = normal(positions) else {
                    continue;
                };
                if application.uvlock {
                    uv = lock_uv(uv, original, application)?;
                }
                let cull_face = face
                    .cullface
                    .as_ref()
                    .map(|direction| crate::model::direction(direction))
                    .transpose()?
                    .map(|direction| {
                        rotate_state(direction.map(|v| v as f32 + 0.5), application)
                            .map(|v| (v - 0.5).round() as i32)
                    })
                    .filter(|direction| boundary_face(positions, *direction));
                quads.push(Quad {
                    vertices: std::array::from_fn(|i| Vertex {
                        position: positions[i],
                        uv: uv[i],
                    }),
                    normal,
                    texture,
                    tint_index: face.tintindex.filter(|&index| index >= 0),
                    shade: element.shade,
                    texture_flags: face.texture_flags.clone(),
                    cull_face,
                });
            }
            occludes |= full_cube && opaque_faces == 6;
        }
        if quads.is_empty() {
            return Err(format!(
                "Model {} needs specialized geometry",
                application.model
            ));
        }
        let mesh = self.meshes.len();
        self.meshes.push(Mesh { quads, occludes });
        self.applications.insert(key, mesh);
        Ok(mesh)
    }
}

const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
const UV_CORNERS: [[f32; 2]; 4] = [[0., 0.], [0., 1.], [1., 1.], [1., 0.]];

fn corners(direction: &str, f: [f32; 3], t: [f32; 3]) -> Result<[[f32; 3]; 4]> {
    let [x0, y0, z0] = f;
    let [x1, y1, z1] = t;
    Ok(match direction {
        "down" => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        "up" => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        "north" => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        "south" => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        "west" => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        "east" => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
        _ => return Err(format!("Invalid model face {direction}")),
    })
}

fn normal(positions: [[f32; 3]; 4]) -> Option<[f32; 3]> {
    let a: [f32; 3] = std::array::from_fn(|i| positions[1][i] - positions[0][i]);
    let b: [f32; 3] = std::array::from_fn(|i| positions[2][i] - positions[0][i]);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
    (length > 1e-7).then(|| n.map(|v| v / length))
}

fn rotate(mut p: [f32; 3], axis: usize, angle: f32, origin: [f32; 3], rescale: bool) -> [f32; 3] {
    let (sin, cos) = angle.to_radians().sin_cos();
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    let x = p[a] - origin[a];
    let y = p[b] - origin[b];
    let scale = if rescale { 1. / cos.abs() } else { 1. };
    p[a] = origin[a] + (x * cos - y * sin) * scale;
    p[b] = origin[b] + (x * sin + y * cos) * scale;
    p
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

pub(super) fn choice_hash(position: Pos, block: &str, part: usize) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in position
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .chain(block.bytes())
        .chain((part as u64).to_le_bytes())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    hash
}
