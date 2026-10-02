use super::{AlphaMode, Face, Options, PreparedScene, Vertex, View, grid};
use crate::{Result, render::geometry};

pub(super) struct Frame {
    pub faces: Vec<Face>,
    pub grid: Option<grid::Grid>,
}

struct Projection {
    faces: Vec<Face>,
    anchor: [f64; 3],
    min: [f64; 2],
    max: [f64; 2],
}

pub(super) fn prepare(scene: &PreparedScene, options: &Options) -> Result<Frame> {
    let mut projection = project(scene, options.view)?;
    let scale = frame_faces(
        &mut projection.faces,
        projection.min,
        projection.max,
        options.size,
    );
    sort_faces(&mut projection.faces);
    let grid = options.grid.then_some(grid::Grid {
        view: options.view,
        scale,
        anchor: projection.anchor,
        offset: std::array::from_fn(|i| {
            f64::from(options.size[i]) * 0.5 - (projection.min[i] + projection.max[i]) * 0.5 * scale
        }),
    });
    Ok(Frame {
        faces: projection.faces,
        grid,
    })
}

fn project(scene: &PreparedScene, view: View) -> Result<Projection> {
    let anchor = scene
        .instances
        .first()
        .ok_or("Selected scene has no geometry to render")?
        .position;
    let mut faces = Vec::new();
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for instance in &scene.instances {
        instance.validate_transform()?;
        let rotation = rotation_matrix(instance.rotation);
        let offset = std::array::from_fn::<_, 3, _>(|i| instance.position[i] - anchor[i]);
        for draw in &instance.draws {
            let mesh = scene
                .meshes
                .get(draw.mesh)
                .ok_or("Invalid scene mesh index")?;
            for &index in &draw.quads {
                let quad = mesh.quads.get(index).ok_or("Invalid scene quad index")?;
                let texture = scene
                    .textures
                    .get(quad.texture)
                    .ok_or("Invalid scene texture index")?;
                let normal = rotation.map(|row| dot(row, quad.normal.map(f64::from)));
                if !normal.iter().all(|v| v.is_finite()) {
                    return Err("Non-finite face normal".into());
                }
                let vertices = quad.vertices.map(|v| Vertex {
                    position: view.project(std::array::from_fn(|i| {
                        dot(rotation[i], v.position.map(f64::from)) + offset[i]
                    })),
                    uv: v.uv,
                });
                for vertex in &vertices {
                    if !vertex.position.iter().all(|v| v.is_finite())
                        || !vertex.uv.iter().all(|v| v.is_finite())
                    {
                        return Err("Non-finite geometry coordinate".into());
                    }
                    for axis in 0..2 {
                        min[axis] = min[axis].min(vertex.position[axis]);
                        max[axis] = max[axis].max(vertex.position[axis]);
                    }
                }
                let facing = view.project(normal)[2];
                if facing <= 1e-8 {
                    continue;
                }
                let light = if quad.shade {
                    (0.65 + 0.35 * dot(normal, [0.36, 0.8, 0.48]).max(0.)) as f32
                } else {
                    1.
                };
                faces.push(Face {
                    depth: vertices.iter().map(|v| v.position[2]).sum(),
                    grid: !instance.is_entity,
                    vertices,
                    texture: quad.texture,
                    alpha: quad.alpha(texture),
                    light,
                    color: geometry::linear_color(quad.color),
                });
            }
        }
    }
    if faces.is_empty() {
        return Err("Selected scene has no visible geometry to render".into());
    }
    Ok(Projection {
        faces,
        anchor,
        min,
        max,
    })
}

fn frame_faces(faces: &mut [Face], min: [f64; 2], max: [f64; 2], size: [u32; 2]) -> f64 {
    let scale = (0..2)
        .map(|i| f64::from(size[i]) * 0.9 / (max[i] - min[i]).max(1e-9))
        .fold(f64::INFINITY, f64::min);
    for face in faces {
        for vertex in &mut face.vertices {
            for axis in 0..2 {
                vertex.position[axis] = (vertex.position[axis] - (min[axis] + max[axis]) * 0.5)
                    * scale
                    + f64::from(size[axis]) * 0.5;
            }
        }
    }
    scale
}

fn sort_faces(faces: &mut [Face]) {
    faces.sort_by(|a, b| {
        let blended = |face: &Face| face.alpha == AlphaMode::Blend;
        blended(a).cmp(&blended(b)).then_with(|| {
            if blended(a) {
                a.depth.total_cmp(&b.depth)
            } else {
                b.depth.total_cmp(&a.depth)
            }
        })
    });
}

fn rotation_matrix(rotation: [f32; 4]) -> [[f64; 3]; 3] {
    let [x, y, z, w] = rotation.map(f64::from);
    [
        [
            1. - 2. * (y * y + z * z),
            2. * (x * y - z * w),
            2. * (x * z + y * w),
        ],
        [
            2. * (x * y + z * w),
            1. - 2. * (x * x + z * z),
            2. * (y * z - x * w),
        ],
        [
            2. * (x * z - y * w),
            2. * (y * z + x * w),
            1. - 2. * (x * x + y * y),
        ],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
