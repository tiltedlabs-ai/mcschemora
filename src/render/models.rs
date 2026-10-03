mod mesh;

use super::geometry::{DIRECTIONS, UV_CORNERS, corners, normal};
use super::{Draw, GeometryAssets, Mesh, Quad, Vertex};
use crate::{
    Result,
    model::{Block, Position},
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

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

#[derive(Clone)]
pub(super) struct StateGeometry {
    pub(super) name: String,
    pub(super) parts: Vec<Vec<(usize, u32)>>,
    pub(super) messages: Vec<String>,
    pub(super) tinted: bool,
    pub(super) fully_cullable: bool,
}

impl StateGeometry {
    pub(super) fn selected_meshes(&self, position: Position) -> impl Iterator<Item = usize> + '_ {
        self.parts.iter().enumerate().map(move |(part, choices)| {
            if let [(mesh, _)] = choices.as_slice() {
                *mesh
            } else {
                let total: u64 = choices.iter().map(|(_, weight)| u64::from(*weight)).sum();
                let mut pick = choice_hash(position, &self.name, part) % total;
                choices
                    .iter()
                    .find_map(|(mesh, weight)| {
                        if pick < u64::from(*weight) {
                            Some(*mesh)
                        } else {
                            pick -= u64::from(*weight);
                            None
                        }
                    })
                    .unwrap()
            }
        })
    }

    pub(super) fn refresh_metadata(&mut self, meshes: &[Mesh]) {
        self.fully_cullable = self.parts.iter().flatten().all(|(mesh, _)| {
            meshes[*mesh].quads.iter().all(|quad| {
                quad.cull_face.is_some_and(|direction| {
                    direction.iter().map(|&v| i64::from(v).abs()).sum::<i64>() == 1
                })
            })
        });
        self.tinted = self.parts.iter().flatten().any(|(mesh, _)| {
            meshes[*mesh]
                .quads
                .iter()
                .any(|quad| quad.tint_index.is_some() && quad.color == [255; 4])
        });
    }

    pub(super) fn draws<'a>(
        &'a self,
        position: Position,
        meshes: &'a [Mesh],
        occlusion: &'a super::occlusion::Occlusion,
    ) -> impl Iterator<Item = Draw> + 'a {
        self.selected_meshes(position).map(move |mesh| Draw {
            mesh,
            quads: meshes[mesh]
                .quads
                .iter()
                .enumerate()
                .filter(|(_, quad)| {
                    !quad
                        .cull_face
                        .and_then(|direction| super::geometry::offset(position, direction))
                        .is_some_and(|neighbor| occlusion.contains(&neighbor))
                })
                .map(|(index, _)| index)
                .collect(),
        })
    }
}

type ModelKey = (String, i32, i32, bool, [u8; 4]);

pub(super) struct Builder<'a> {
    pub(super) assets: &'a GeometryAssets,
    pub(super) meshes: Vec<Mesh>,
    pub(super) applications: BTreeMap<ModelKey, usize>,
    pub(super) states: BTreeMap<Block, Arc<StateGeometry>>,
}

fn property_matches(actual: Option<&String>, expected: &str) -> bool {
    let (invert, expected) = expected
        .strip_prefix('!')
        .map_or((false, expected), |v| (true, v));
    let matches = actual.is_some_and(|actual| expected.split('|').any(|value| value == actual));
    if invert { !matches } else { matches }
}

pub(super) fn condition(value: &Value, block: &Block) -> Result<bool> {
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
    pub(super) fn state(&mut self, block: &Block) -> Arc<StateGeometry> {
        if let Some(state) = self.states.get(block) {
            return state.clone();
        }
        let (parts, messages) = match self.compile_state(block) {
            Ok(parts) => (parts, Vec::new()),
            Err(message) => {
                let key = ("__placeholder__".into(), 0, 0, false, [255; 4]);
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
                                color: [255; 4],
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
                (
                    vec![vec![(mesh, 1)]],
                    vec![format!("{message}; showing missing-geometry placeholder")],
                )
            }
        };
        let mut state = StateGeometry {
            name: block.text(),
            parts,
            messages,
            tinted: false,
            fully_cullable: false,
        };
        state.refresh_metadata(&self.meshes);
        let state = Arc::new(state);
        self.states.insert(block.clone(), state.clone());
        state
    }

    fn compile_state(&mut self, block: &Block) -> Result<Vec<Vec<(usize, u32)>>> {
        if block.id == "minecraft:moving_piston" {
            let mut head = block.clone();
            head.id = "minecraft:piston_head".into();
            head.properties.insert("short".into(), "false".into());
            return self.compile_state(&head);
        }

        if super::special::contains(block) && !super::special::overlay(block) {
            let geometry = super::special::bake(self.assets, block)?;
            let mesh = self.meshes.len();
            self.meshes.push(geometry);
            return Ok(vec![vec![(mesh, 1)]]);
        }
        let state = self
            .assets
            .states
            .get(&block.id)
            .ok_or_else(|| format!("No blockstate visuals for {}", block.id))?;
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

        let color = state_color(block)?;
        let mut result: Vec<Vec<(usize, u32)>> = parts
            .into_iter()
            .map(|part| {
                part.into_iter()
                    .map(|application| {
                        self.mesh(&application, color)
                            .map(|mesh| (mesh, application.weight))
                    })
                    .collect()
            })
            .collect::<Result<_>>()?;
        if super::special::overlay(block) {
            let geometry = super::special::bake(self.assets, block)?;
            let mesh = self.meshes.len();
            self.meshes.push(geometry);
            result.push(vec![(mesh, 1)]);
        }
        Ok(result)
    }

    fn mesh(&mut self, application: &Application, color: [u8; 4]) -> Result<usize> {
        let key = (
            application.model.clone(),
            application.x,
            application.y,
            application.uvlock,
            color,
        );
        if let Some(&mesh) = self.applications.get(&key) {
            return Ok(mesh);
        }
        let mut geometry = mesh::bake(self.assets, application)?;
        if color != [255; 4] {
            for quad in &mut geometry.quads {
                if quad.tint_index == Some(0) {
                    quad.color = color;
                }
            }
        }
        let mesh = self.meshes.len();
        self.meshes.push(geometry);
        self.applications.insert(key, mesh);
        Ok(mesh)
    }
}

fn state_color(block: &Block) -> Result<[u8; 4]> {
    if block.id != "minecraft:redstone_wire" {
        return Ok([255; 4]);
    }
    let power = block
        .properties
        .get("power")
        .map(String::as_str)
        .unwrap_or("0")
        .parse::<u8>()
        .ok()
        .filter(|&power| power <= 15)
        .ok_or("Invalid redstone wire power")?;
    let strength = f32::from(power) / 15.;
    let red = if power == 0 {
        0.3
    } else {
        strength * 0.6 + 0.4
    };
    let green = (strength * strength * 0.7 - 0.5).max(0.);
    let blue = (strength * strength * 0.6 - 0.7).max(0.);
    Ok([red, green, blue, 1.].map(|channel| (channel * 255.) as u8))
}

fn choice_hash(position: Position, block: &str, part: usize) -> u64 {
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
