use crate::Result;
use image::RgbaImage;
use std::collections::{BTreeSet, HashMap};

pub(super) fn encode(
    dimensions: [u32; 2],
    cell_size: u32,
    grid: bool,
    draws: &[([u32; 2], usize)],
    tiles: &HashMap<usize, RgbaImage>,
) -> Result<Option<Vec<u8>>> {
    let mut colors = BTreeSet::from([[0, 0, 0, 0]]);
    if grid {
        colors.insert([40, 40, 40, 255]);
    }
    for tile in tiles.values() {
        for pixel in tile.pixels() {
            colors.insert(pixel.0);
            if colors.len() > 256 {
                return Ok(None);
            }
        }
    }
    let palette: HashMap<_, _> = colors
        .iter()
        .enumerate()
        .map(|(index, color)| (*color, index as u8))
        .collect();
    let indexed: HashMap<_, Vec<u8>> = tiles
        .iter()
        .map(|(id, tile)| (*id, tile.pixels().map(|pixel| palette[&pixel.0]).collect()))
        .collect();
    let [width, height] = dimensions.map(|value| value as usize);
    let size = cell_size as usize;
    let mut pixels = vec![0; width * height];
    for &([x, y], id) in draws {
        for (row, source) in indexed[&id].chunks_exact(size).enumerate() {
            let start = (y as usize + row) * width + x as usize;
            pixels[start..start + size].copy_from_slice(source);
        }
    }
    if grid {
        let color = palette[&[40, 40, 40, 255]];
        for row in pixels.chunks_exact_mut(width) {
            for x in (0..width).step_by(size) {
                row[x] = color;
            }
        }
        for y in (0..height).step_by(size) {
            pixels[y * width..(y + 1) * width].fill(color);
        }
    }
    let rgb: Vec<u8> = colors
        .iter()
        .flat_map(|color| color[..3].iter().copied())
        .collect();
    let alpha: Vec<u8> = colors.iter().map(|color| color[3]).collect();
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, dimensions[0], dimensions[1]);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(rgb);
        encoder.set_trns(alpha);
        encoder.set_deflate_compression(png::DeflateCompression::Level(1));
        encoder.set_filter(png::Filter::Sub);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(Some(output))
}
