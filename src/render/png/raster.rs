use super::{AlphaMode, Face, PreparedScene, Vertex, grid};
use crate::{
    Result,
    render::{Texture, geometry},
};
use image::RgbaImage;
use std::sync::LazyLock;

fn edge(a: [f64; 3], b: [f64; 3], p: [f64; 3]) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}

fn top_left(a: [f64; 3], b: [f64; 3]) -> bool {
    b[1] < a[1] || (b[1] == a[1] && b[0] > a[0])
}

fn srgb_exact(value: f32) -> u8 {
    let value = if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1. / 2.4) - 0.055
    };
    (value.clamp(0., 1.) * 255.).round() as u8
}

fn srgb(value: f32) -> u8 {
    static TABLE: LazyLock<[(u8, u8); 4096]> = LazyLock::new(|| {
        std::array::from_fn(|i| {
            (
                srgb_exact(i as f32 / 4096.),
                srgb_exact((i + 1) as f32 / 4096.),
            )
        })
    });
    let index = (value.clamp(0., 1.) * 4096.).min(4095.) as usize;
    let (low, high) = TABLE[index];
    if low == high { low } else { srgb_exact(value) }
}

struct Raster {
    grid: Option<grid::Grid>,
    start_y: u32,
    size: [u32; 2],
    pixels: Vec<[f32; 4]>,
    depth: Vec<f64>,
    colors: &'static [f32; 256],
    tile_depth: Vec<f64>,
    tile_writes: Vec<usize>,
}

impl Raster {
    fn triangle(
        &mut self,
        mut vertices: [Vertex; 3],
        face: &Face,
        texture: &Texture,
        atlas: &RgbaImage,
    ) {
        let mut area = edge(
            vertices[0].position,
            vertices[1].position,
            vertices[2].position,
        );
        if area == 0. {
            return;
        }
        if area < 0. {
            vertices.swap(1, 2);
            area = -area;
        }
        let p = vertices.map(|v| v.position);
        let lines = self
            .grid
            .filter(|_| face.grid)
            .map(|grid| grid.triangle(p, area));
        let min: [u32; 2] = std::array::from_fn(|axis| {
            p.iter()
                .map(|v| v[axis])
                .fold(f64::INFINITY, f64::min)
                .floor()
                .clamp(
                    if axis == 1 {
                        f64::from(self.start_y)
                    } else {
                        0.
                    },
                    f64::from(self.size[axis] + if axis == 1 { self.start_y } else { 0 }),
                ) as u32
        });
        let max: [u32; 2] = std::array::from_fn(|axis| {
            p.iter()
                .map(|v| v[axis])
                .fold(f64::NEG_INFINITY, f64::max)
                .ceil()
                .clamp(
                    if axis == 1 {
                        f64::from(self.start_y)
                    } else {
                        0.
                    },
                    f64::from(self.size[axis] + if axis == 1 { self.start_y } else { 0 }),
                ) as u32
        });
        let inclusive = [
            top_left(p[1], p[2]),
            top_left(p[2], p[0]),
            top_left(p[0], p[1]),
        ];
        let origin = [
            (texture.uv[0] * atlas.width() as f32).round() as u32,
            (texture.uv[1] * atlas.height() as f32).round() as u32,
        ];
        let nearest = p.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
        let columns = self.size[0].div_ceil(8) as usize;
        for ty in min[1] / 8..max[1].div_ceil(8) {
            for tx in min[0] / 8..max[0].div_ceil(8) {
                let tile = (ty - self.start_y / 8) as usize * columns + tx as usize;
                if nearest <= self.tile_depth[tile] {
                    continue;
                }
                for y in min[1].max(ty * 8)..max[1].min(ty * 8 + 8) {
                    for x in min[0].max(tx * 8)..max[0].min(tx * 8 + 8) {
                        let point = [f64::from(x) + 0.5, f64::from(y) + 0.5, 0.];
                        let edges = [
                            edge(p[1], p[2], point),
                            edge(p[2], p[0], point),
                            edge(p[0], p[1], point),
                        ];
                        if edges
                            .iter()
                            .zip(inclusive)
                            .any(|(&v, inclusive)| v < 0. || (v == 0. && !inclusive))
                        {
                            continue;
                        }
                        let weights = edges.map(|e| e / area);
                        let depth: f64 = (0..3).map(|i| weights[i] * p[i][2]).sum();
                        let index =
                            (y - self.start_y) as usize * self.size[0] as usize + x as usize;
                        if depth <= self.depth[index] {
                            continue;
                        }
                        let uv: [u32; 2] = std::array::from_fn(|axis| {
                            let uv: f64 = (0..3)
                                .map(|i| weights[i] * f64::from(vertices[i].uv[axis]))
                                .sum();
                            origin[axis]
                                + (uv.clamp(0., 1.) * f64::from(texture.size[axis]))
                                    .floor()
                                    .min(f64::from(texture.size[axis] - 1))
                                    as u32
                        });
                        let color = atlas.get_pixel(uv[0], uv[1]).0;
                        let alpha = match face.alpha {
                            AlphaMode::Opaque => 1.,
                            AlphaMode::Mask if color[3] >= 128 => 1.,
                            AlphaMode::Mask => continue,
                            AlphaMode::Blend => f32::from(color[3]) / 255. * face.color[3],
                        };
                        if alpha == 0. {
                            continue;
                        }
                        let (grid_scale, grid_white) = lines
                            .as_ref()
                            .map_or((1., 0.), |lines| lines.color(point[0], point[1]));
                        let pixel = &mut self.pixels[index];
                        for i in 0..3 {
                            let surface =
                                self.colors[color[i] as usize] * face.color[i] * face.light;
                            pixel[i] = (surface * grid_scale + grid_white) * alpha
                                + pixel[i] * (1. - alpha);
                        }
                        pixel[3] = alpha + pixel[3] * (1. - alpha);
                        if face.alpha != AlphaMode::Blend {
                            self.depth[index] = depth;
                            self.tile_writes[tile] += 1;
                        }
                    }
                }
                if self.tile_writes[tile] >= 64 {
                    let mut depth = f64::INFINITY;
                    for y in ty * 8..(self.size[1] + self.start_y).min(ty * 8 + 8) {
                        for x in tx * 8..self.size[0].min(tx * 8 + 8) {
                            depth = depth.min(
                                self.depth[(y - self.start_y) as usize * self.size[0] as usize
                                    + x as usize],
                            );
                        }
                    }
                    self.tile_depth[tile] = depth;
                    self.tile_writes[tile] = 0;
                }
            }
        }
    }
}

pub(super) fn render(
    scene: &PreparedScene,
    faces: &[Face],
    size: [u32; 2],
    grid: Option<grid::Grid>,
    threads: usize,
) -> Result<Vec<u8>> {
    let workers = threads.min(size[1] as usize);
    let band_height = size[1].div_ceil(workers as u32).div_ceil(8).max(16) * 8;
    let mut bands = vec![Vec::new(); size[1].div_ceil(band_height) as usize];
    for face in faces {
        let min_y = face
            .vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f64::INFINITY, f64::min);
        let max_y = face
            .vertices
            .iter()
            .map(|v| v.position[1])
            .fold(f64::NEG_INFINITY, f64::max);
        if max_y < 0. || min_y >= f64::from(size[1]) {
            continue;
        }
        let first = (min_y.max(0.) as u32 / band_height) as usize;
        let last = (max_y.max(0.) as u32 / band_height).min(bands.len() as u32 - 1) as usize;
        for band in &mut bands[first..=last] {
            band.push(face);
        }
    }
    let pixels = std::thread::scope(|scope| {
        let jobs: Vec<_> = bands
            .into_iter()
            .enumerate()
            .map(|(index, faces)| {
                let start_y = index as u32 * band_height;
                scope.spawn(move || {
                    render_band(
                        scene,
                        &faces,
                        [size[0], band_height.min(size[1] - start_y)],
                        start_y,
                        grid,
                    )
                })
            })
            .collect();
        let mut pixels = Vec::with_capacity(size[0] as usize * size[1] as usize * 4);
        for job in jobs {
            pixels.extend(job.join().map_err(|_| "PNG render worker panicked")?);
        }
        Ok::<_, String>(pixels)
    })?;
    Ok(pixels)
}

fn render_band(
    scene: &PreparedScene,
    faces: &[&Face],
    size: [u32; 2],
    start_y: u32,
    grid: Option<grid::Grid>,
) -> Vec<u8> {
    let count = size[0] as usize * size[1] as usize;
    let mut raster = Raster {
        grid,
        size,
        start_y,
        tile_depth: vec![f64::NEG_INFINITY; (size[0].div_ceil(8) * size[1].div_ceil(8)) as usize],
        tile_writes: vec![0; (size[0].div_ceil(8) * size[1].div_ceil(8)) as usize],
        pixels: vec![[0.; 4]; count],
        depth: vec![f64::NEG_INFINITY; count],
        colors: geometry::linear_colors(),
    };
    for face in faces {
        let texture = &scene.textures[face.texture];
        let atlas = &scene.atlas_images[texture.atlas];
        for indices in [[0, 1, 2], [0, 2, 3]] {
            raster.triangle(indices.map(|i| face.vertices[i]), face, texture, atlas);
        }
    }
    raster
        .pixels
        .into_iter()
        .flat_map(|pixel| {
            if pixel[3] == 0. {
                [0; 4]
            } else {
                [
                    srgb(pixel[0] / pixel[3]),
                    srgb(pixel[1] / pixel[3]),
                    srgb(pixel[2] / pixel[3]),
                    (pixel[3] * 255.).round() as u8,
                ]
            }
        })
        .collect()
}
