use super::{AlphaMode, Draw, PreparedScene, Quad, geometry};
use crate::Result;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs};

#[derive(Default)]
struct Buffer {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Buffer {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let mut view =
            json!({"buffer": 0, "byteOffset": self.bytes.len(), "byteLength": bytes.len()});
        if let Some(target) = target {
            view["target"] = json!(target);
        }
        self.bytes.extend_from_slice(bytes);
        let index = self.views.len();
        self.views.push(view);
        index
    }

    fn floats<const N: usize>(&mut self, values: &[[f32; N]], bounds: bool) -> Result<usize> {
        let mut bytes = Vec::with_capacity(values.len() * N * 4);
        let mut min = [f32::INFINITY; N];
        let mut max = [f32::NEG_INFINITY; N];
        for value in values {
            for i in 0..N {
                if !value[i].is_finite() {
                    return Err("Non-finite geometry coordinate".into());
                }
                min[i] = min[i].min(value[i]);
                max[i] = max[i].max(value[i]);
                bytes.extend_from_slice(&value[i].to_le_bytes());
            }
        }
        let view = self.view(&bytes, Some(34962));
        let mut accessor = json!({"bufferView": view, "componentType": 5126, "count": values.len(), "type": format!("VEC{N}")});
        if bounds {
            accessor["min"] = json!(min.as_slice());
            accessor["max"] = json!(max.as_slice());
        }
        let index = self.accessors.len();
        self.accessors.push(accessor);
        Ok(index)
    }

    fn indices(&mut self, values: &[u32]) -> usize {
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        let view = self.view(&bytes, Some(34963));
        let index = self.accessors.len();
        self.accessors.push(json!({"bufferView": view, "componentType": 5125, "count": values.len(), "type": "SCALAR"}));
        index
    }
}

fn container(document: &Value, mut binary: Vec<u8>) -> Result<Vec<u8>> {
    let mut json = serde_json::to_vec(document).map_err(|e| e.to_string())?;
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let total = 12usize
        .checked_add(8)
        .and_then(|n| n.checked_add(json.len()))
        .and_then(|n| n.checked_add(8))
        .and_then(|n| n.checked_add(binary.len()))
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("GLB exceeds the 4 GiB format limit")?;
    let mut bytes = Vec::with_capacity(total as usize);
    bytes.extend_from_slice(b"glTF");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&total.to_le_bytes());
    bytes.extend_from_slice(&(json.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"JSON");
    bytes.extend_from_slice(&json);
    bytes.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    bytes.extend_from_slice(b"BIN\0");
    bytes.extend_from_slice(&binary);
    Ok(bytes)
}

pub fn encode(scene: &PreparedScene) -> Result<Vec<u8>> {
    let mut buffer = Buffer::default();
    let mut images = Vec::new();
    let mut textures = Vec::new();
    let mut materials = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();
    let mut pages = BTreeMap::new();
    let mut material_ids = BTreeMap::new();
    let mut mesh_ids: BTreeMap<&[Draw], usize> = BTreeMap::new();
    for instance in &scene.instances {
        instance.validate_transform()?;
        let key = instance.draws.as_slice();
        let mesh = if let Some(&mesh) = mesh_ids.get(key) {
            mesh
        } else {
            let mut groups: BTreeMap<_, Vec<&Quad>> = BTreeMap::new();
            for draw in &instance.draws {
                let mesh = scene
                    .meshes
                    .get(draw.mesh)
                    .ok_or("Invalid scene mesh index")?;
                for &index in &draw.quads {
                    let quad = mesh.quads.get(index).ok_or("Invalid scene quad index")?;
                    let sprite = scene
                        .textures
                        .get(quad.texture)
                        .ok_or("Invalid scene texture index")?;
                    groups
                        .entry((
                            sprite.atlas,
                            match quad.alpha(sprite) {
                                AlphaMode::Opaque => "OPAQUE",
                                AlphaMode::Mask => "MASK",
                                AlphaMode::Blend => "BLEND",
                            },
                            quad.color,
                            quad.tint_index,
                            quad.shade,
                            quad.texture_flags.to_string(),
                        ))
                        .or_default()
                        .push(quad);
                }
            }
            if groups.is_empty() {
                continue;
            }
            let mut primitives = Vec::new();
            for ((atlas, alpha, color, tint, shade, _), quads) in groups {
                let flags = &quads[0].texture_flags;
                let material = if let Some(&index) = material_ids.get(&(atlas, alpha, color)) {
                    index
                } else {
                    let page = if let Some(&page) = pages.get(&atlas) {
                        page
                    } else {
                        let path = scene
                            .atlases
                            .get(atlas)
                            .ok_or("Invalid scene atlas index")?;
                        let png = fs::read(path)
                            .map_err(|e| format!("Read atlas {}: {e}", path.display()))?;
                        let view = buffer.view(&png, None);
                        let page = images.len();
                        images.push(json!({"bufferView": view, "mimeType": "image/png"}));
                        textures.push(json!({"sampler": 0, "source": page}));
                        pages.insert(atlas, page);
                        page
                    };
                    let mut value = json!({"name": format!("atlas-{atlas}-{alpha}"), "pbrMetallicRoughness": {"baseColorTexture": {"index": page}, "metallicFactor": 0, "roughnessFactor": 1}, "alphaMode": alpha});
                    if color != [255; 4] {
                        value["pbrMetallicRoughness"]["baseColorFactor"] =
                            json!(geometry::linear_color(color));
                    }
                    if alpha == "MASK" {
                        value["alphaCutoff"] = json!(0.5);
                    }
                    let index = materials.len();
                    materials.push(value);
                    material_ids.insert((atlas, alpha, color), index);
                    index
                };
                let mut positions = Vec::with_capacity(quads.len() * 4);
                let mut normals = Vec::with_capacity(quads.len() * 4);
                let mut uvs = Vec::with_capacity(quads.len() * 4);
                let mut indices = Vec::with_capacity(quads.len() * 6);
                for quad in &quads {
                    let sprite = &scene.textures[quad.texture];
                    let base = u32::try_from(positions.len())
                        .map_err(|_| "GLB primitive has too many vertices")?;
                    if base > u32::MAX - 4 {
                        return Err("GLB primitive has too many vertices".into());
                    }
                    for vertex in &quad.vertices {
                        positions.push(vertex.position);
                        normals.push(quad.normal);
                        uvs.push([
                            sprite.uv[0] + vertex.uv[0] * (sprite.uv[2] - sprite.uv[0]),
                            sprite.uv[1] + vertex.uv[1] * (sprite.uv[3] - sprite.uv[1]),
                        ]);
                    }
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                let position = buffer.floats(&positions, true)?;
                let normal = buffer.floats(&normals, false)?;
                let uv = buffer.floats(&uvs, false)?;
                let indices = buffer.indices(&indices);
                primitives.push(json!({"attributes": {"POSITION": position, "NORMAL": normal, "TEXCOORD_0": uv}, "indices": indices, "material": material, "extras": {"tint_index": tint, "shade": shade, "texture_flags": flags}}));
            }
            let index = meshes.len();
            meshes.push(json!({"primitives": primitives}));
            mesh_ids.insert(key, index);
            index
        };
        nodes.push(json!({"name": instance.name, "mesh": mesh, "translation": instance.position, "rotation": instance.rotation}));
    }
    if nodes.is_empty() {
        return Err("Selected scene has no visible geometry to export".into());
    }
    let document = json!({
        "asset": {"version": "2.0", "generator": concat!("schemora/", env!("CARGO_PKG_VERSION"))},
        "scene": 0, "scenes": [{"nodes": (0..nodes.len()).collect::<Vec<_>>()}],
        "nodes": nodes, "meshes": meshes, "materials": materials, "textures": textures,
        "images": images, "samplers": [{"magFilter": 9728, "minFilter": 9728, "wrapS": 33071, "wrapT": 33071}],
        "buffers": [{"byteLength": buffer.bytes.len()}], "bufferViews": buffer.views, "accessors": buffer.accessors,
        "extras": {"diagnostics": scene.diagnostics}
    });
    container(&document, buffer.bytes)
}
