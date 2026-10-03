//! CPU rendering of prepared scene geometry to PNG bytes.

mod grid;
mod projection;
mod raster;

use super::{AlphaMode, PreparedScene, View};
use crate::Result;
use image::{
    ImageEncoder,
    codecs::png::{CompressionType, FilterType, PngEncoder},
};

/// Output dimensions, camera view, and grid settings for textured PNG rendering.
#[derive(Clone, Debug)]
pub struct Options {
    /// Width and height in pixels, each from 1 through 4096.
    pub size: [u32; 2],
    /// Camera orientation; default is isometric.
    pub view: View,
    /// Whether to draw outlined block edges, excluding entities.
    pub grid: bool,
    pub threads: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            size: [1024; 2],
            view: View::default(),
            grid: false,
            threads: 8,
        }
    }
}

#[derive(Clone, Copy)]
struct Vertex {
    position: [f64; 3],
    uv: [f32; 2],
}

struct Face {
    depth: f64,
    grid: bool,
    vertices: [Vertex; 4],
    texture: usize,
    alpha: AlphaMode,
    light: f32,
    color: [f32; 4],
}

/// Renders a prepared scene as an automatically framed, transparent PNG.
///
/// Uses CPU rasterization, nearest-neighbor textures, and directional lighting.
pub fn encode(scene: &PreparedScene, options: &Options) -> Result<Vec<u8>> {
    if options.size.iter().any(|&v| v == 0 || v > 4096) {
        return Err("PNG dimensions must be between 1 and 4096 pixels".into());
    }
    if options.threads == 0 {
        return Err("PNG threads must be greater than zero".into());
    }
    let frame = projection::prepare(scene, options)?;
    validate_textures(scene)?;
    let pixels = raster::render(
        scene,
        &frame.faces,
        options.size,
        frame.grid,
        options.threads,
    )?;
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Best, FilterType::Adaptive)
        .write_image(
            &pixels,
            options.size[0],
            options.size[1],
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn validate_textures(scene: &PreparedScene) -> Result<()> {
    let atlases = &scene.atlas_images;
    for texture in &scene.textures {
        let atlas = atlases
            .get(texture.atlas)
            .ok_or("Invalid scene atlas index")?;
        for (axis, limit) in [atlas.width(), atlas.height()].into_iter().enumerate() {
            let origin = texture.uv[axis] * limit as f32;
            if !origin.is_finite()
                || origin < 0.
                || texture.size[axis] == 0
                || origin.round() as f64 + f64::from(texture.size[axis]) > f64::from(limit)
            {
                return Err("Texture rectangle outside atlas".into());
            }
        }
    }
    Ok(())
}
