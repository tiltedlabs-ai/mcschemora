//! Hierarchical cuboid models baked against the native geometry texture catalog.

use super::{
    GeometryAssets, Mesh, Quad, Vertex,
    geometry::{UV_CORNERS, corners, normal, rotate},
};
use crate::Result;
use std::collections::BTreeMap;

/// A hierarchical model part with a local transform and named children.
#[derive(Clone, Debug)]
pub struct Part {
    /// Local translation in model pixels; 16 pixels equal one block.
    pub pivot: [f32; 3],
    /// Euler rotations in degrees, applied about X, Y, then Z.
    pub rotation: [f32; 3],
    /// Positive scale factors along local X, Y, and Z.
    pub scale: [f32; 3],
    /// Cuboids attached to this part.
    pub cuboids: Vec<Cuboid>,
    /// Child parts inheriting this transform.
    pub children: BTreeMap<String, Part>,
}

impl Default for Part {
    fn default() -> Self {
        Self {
            pivot: [0.; 3],
            rotation: [0.; 3],
            scale: [1.; 3],
            cuboids: Vec::new(),
            children: BTreeMap::new(),
        }
    }
}

/// An axis-aligned box in model pixel coordinates.
#[derive(Clone, Debug)]
pub struct Cuboid {
    /// Minimum local model coordinates.
    pub from: [f32; 3],
    /// Maximum local model coordinates.
    pub to: [f32; 3],
    /// Faces indexed by cardinal direction name.
    pub faces: BTreeMap<String, Face>,
}

/// Texture mapping for a model-part face.
#[derive(Clone, Debug, Default)]
pub struct Face {
    /// Texture identifier present in the geometry assets.
    pub texture: String,
    /// Texture pixel rectangle [u0, v0, u1, v1].
    pub uv: [f32; 4],
    /// UV rotation in degrees; multiples of 90 are supported.
    pub rotation: u16,
}

struct Frame {
    axes: [[f32; 3]; 3],
    origin: [f32; 3],
}

impl Frame {
    fn vector(&self, p: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| (0..3).map(|j| self.axes[j][i] * p[j]).sum())
    }

    fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let vector = self.vector(p);
        std::array::from_fn(|i| vector[i] + self.origin[i])
    }

    fn child(&self, part: &Part) -> Result<Self> {
        if !part
            .pivot
            .iter()
            .chain(&part.rotation)
            .chain(&part.scale)
            .all(|v| v.is_finite())
            || part.scale.iter().any(|&v| v <= 0.)
        {
            return Err("Model part transforms require finite values and positive scale".into());
        }
        let axes = std::array::from_fn(|i| {
            let mut axis = [0.; 3];
            axis[i] = part.scale[i];
            for dimension in 0..3 {
                axis = rotate(
                    axis,
                    dimension,
                    part.rotation[dimension].rem_euclid(360.),
                    [0.; 3],
                    false,
                );
            }
            self.vector(axis)
        });
        let origin = self.point(part.pivot);
        if !origin
            .iter()
            .chain(axes.iter().flatten())
            .all(|v| v.is_finite())
        {
            return Err("Model part transform overflow".into());
        }
        Ok(Self { axes, origin })
    }
}

impl GeometryAssets {
    /// Bakes a part hierarchy into block-unit mesh geometry using loaded textures.
    pub fn bake_parts(&self, root: &Part) -> Result<Mesh> {
        let identity = Frame {
            axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
            origin: [0.; 3],
        };
        let mut quads = Vec::new();
        self.bake_part(root, &identity, &mut quads)?;
        Ok(Mesh {
            quads,
            occludes: false,
        })
    }

    fn bake_part(&self, part: &Part, parent: &Frame, quads: &mut Vec<Quad>) -> Result<()> {
        let frame = parent.child(part)?;
        for cuboid in &part.cuboids {
            if !cuboid.from.iter().chain(&cuboid.to).all(|v| v.is_finite()) {
                return Err("Cuboid bounds must be finite".into());
            }
            for (direction, face) in &cuboid.faces {
                let positions = corners(direction, cuboid.from, cuboid.to)?
                    .map(|p| frame.point(p).map(|v| v / 16.));
                if !positions.iter().flatten().all(|v| v.is_finite()) {
                    return Err("Model part vertex overflow".into());
                }
                let Some(normal) = normal(positions) else {
                    continue;
                };
                let texture = *self
                    .texture_ids
                    .get(&face.texture)
                    .ok_or_else(|| format!("Missing model part texture {}", face.texture))?;
                let size = self.textures[texture].size.map(|v| v as f32);
                if face.rotation % 90 != 0
                    || !face
                        .uv
                        .iter()
                        .enumerate()
                        .all(|(i, &v)| v.is_finite() && v >= 0. && v <= size[i % 2])
                {
                    return Err(format!(
                        "Invalid pixel UV rectangle or rotation for {}",
                        face.texture
                    ));
                }
                if !normal.iter().all(|v| v.is_finite())
                    || normal.iter().map(|v| v * v).sum::<f32>() < 0.5
                {
                    return Err("Model part normal overflow".into());
                }
                quads.push(Quad {
                    vertices: std::array::from_fn(|i| {
                        let corner = UV_CORNERS[(i + usize::from(face.rotation / 90)) % 4];
                        Vertex {
                            position: positions[i],
                            uv: std::array::from_fn(|axis| {
                                (face.uv[axis] + corner[axis] * (face.uv[axis + 2] - face.uv[axis]))
                                    / size[axis]
                            }),
                        }
                    }),
                    normal,
                    texture,
                    tint_index: None,
                    shade: true,
                    color: [255; 4],
                    texture_flags: serde_json::Value::Null,
                    cull_face: None,
                });
            }
        }
        for (name, child) in &part.children {
            self.bake_part(child, &frame, quads)
                .map_err(|e| format!("Model part {name:?}: {e}"))?;
        }
        Ok(())
    }
}
