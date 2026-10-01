use super::{
    Cache,
    visuals::{Input, write_json},
};
use crate::Result;
use image::{ImageReader, Rgba, RgbaImage};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{collections::BTreeMap, fs, io::Cursor, path::Path};

const MIN_PAGE_SIZE: u32 = 1024;
const PADDING: u32 = 1;

fn static_frame(image: RgbaImage, metadata: &Value) -> Result<(RgbaImage, Value)> {
    let Some(animation) = metadata.get("animation") else {
        return Ok((image, Value::Null));
    };
    let explicit_width = animation.get("width").and_then(Value::as_u64);
    let explicit_height = animation.get("height").and_then(Value::as_u64);
    let default = u64::from(image.width().min(image.height()));
    let width = explicit_width.unwrap_or_else(|| {
        if explicit_height.is_some() {
            u64::from(image.width())
        } else {
            default
        }
    });
    let height = explicit_height.unwrap_or_else(|| {
        if explicit_width.is_some() {
            u64::from(image.height())
        } else {
            default
        }
    });
    if width == 0
        || height == 0
        || width > u64::from(image.width())
        || height > u64::from(image.height())
        || u64::from(image.width()) % width != 0
        || u64::from(image.height()) % height != 0
    {
        return Err("Invalid animation frame dimensions".into());
    }
    let frames = animation.get("frames").and_then(Value::as_array);
    let first = frames.and_then(|f| f.first());
    let index = match first {
        Some(Value::Number(n)) => n.as_u64().ok_or("Invalid animation frame index")?,
        Some(Value::Object(v)) => v
            .get("index")
            .and_then(Value::as_u64)
            .ok_or("Invalid animation frame object")?,
        Some(_) => return Err("Unsupported animation frame encoding".into()),
        None => 0,
    };
    let columns = u64::from(image.width()) / width;
    let rows = u64::from(image.height()) / height;
    if index >= columns * rows {
        return Err("Animation frame index is outside texture".into());
    }
    let frame = image::imageops::crop_imm(
        &image,
        ((index % columns) * width) as u32,
        ((index / columns) * height) as u32,
        width as u32,
        height as u32,
    )
    .to_image();
    Ok((
        frame,
        json!({"index":index,"width":width,"height":height,"source_width":image.width(),"source_height":image.height()}),
    ))
}

fn missing() -> RgbaImage {
    RgbaImage::from_fn(16, 16, |x, y| {
        if (x < 8) == (y < 8) {
            Rgba([255, 0, 255, 255])
        } else {
            Rgba([0, 0, 0, 255])
        }
    })
}

pub(super) fn prepare(cache: &Cache, inputs: &[Input], output: &Path) -> Result<Vec<Value>> {
    let sidecars: BTreeMap<&str, &Input> = inputs
        .iter()
        .filter_map(|f| f.path.strip_suffix(".mcmeta").map(|path| (path, f)))
        .collect();
    let mut sprites = BTreeMap::new();
    let mut placements = BTreeMap::<String, (usize, u32, u32, u32, u32)>::new();
    let (mut x, mut y, mut row) = (0, 0, 0);
    let mut images: Vec<(String, RgbaImage, Value, Value, Value)> = Vec::new();
    for input in inputs.iter().filter(|f| f.path.ends_with(".png")) {
        let bytes = fs::read(cache.visual_blob(&input.hash)).map_err(|e| e.to_string())?;
        let mut reader = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(4096);
        limits.max_image_height = Some(4096);
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let image = reader
            .decode()
            .map_err(|e| format!("Texture {}: {e}", input.path))?
            .into_rgba8();
        let metadata = if let Some(sidecar) = sidecars.get(input.path.as_str()) {
            cache.json(&cache.visual_blob(&sidecar.hash))?
        } else {
            Value::Null
        };
        let (image, frame) =
            static_frame(image, &metadata).map_err(|e| format!("Texture {}: {e}", input.path))?;
        let (directory, name) = input
            .path
            .trim_end_matches(".png")
            .split_once('/')
            .ok_or("Invalid texture path")?;
        let directory = match directory {
            "blocks" => "block",
            "items" => "item",
            "colormap" => "colormap",
            "entity" => "entity",
            "font" => "font",
            _ => return Err("Unsupported texture directory".into()),
        };
        images.push((
            format!("minecraft:{directory}/{name}"),
            image,
            metadata,
            frame,
            json!({"path":input.path,"blob":input.hash}),
        ));
    }
    images.push((
        "minecraft:missingno".into(),
        missing(),
        Value::Null,
        Value::Null,
        Value::Null,
    ));
    images.sort_by(|a, b| a.0.cmp(&b.0));
    let page_size = images
        .iter()
        .map(|(_, image, _, _, _)| image.width().max(image.height()) + 2 * PADDING)
        .max()
        .unwrap_or(MIN_PAGE_SIZE)
        .next_power_of_two()
        .max(MIN_PAGE_SIZE);
    let mut pages = vec![RgbaImage::new(page_size, page_size)];
    for (name, image, metadata, frame, source) in images {
        let (width, height) = image.dimensions();
        let mut hash = Sha1::new();
        hash.update(width.to_le_bytes());
        hash.update(height.to_le_bytes());
        hash.update(image.as_raw());
        let hash = format!("{:x}", hash.finalize());
        let placement = if let Some(&placement) = placements.get(&hash) {
            placement
        } else {
            let padded_width = width + 2 * PADDING;
            let padded_height = height + 2 * PADDING;
            if x + padded_width > page_size {
                x = 0;
                y += row;
                row = 0;
            }
            if y + padded_height > page_size {
                pages.push(RgbaImage::new(page_size, page_size));
                x = 0;
                y = 0;
                row = 0;
            }
            let page = pages.len() - 1;
            for py in 0..padded_height {
                for px in 0..padded_width {
                    let sx = px.saturating_sub(PADDING).min(width - 1);
                    let sy = py.saturating_sub(PADDING).min(height - 1);
                    pages[page].put_pixel(x + px, y + py, *image.get_pixel(sx, sy));
                }
            }
            let placement = (page, x + PADDING, y + PADDING, width, height);
            placements.insert(hash.clone(), placement);
            x += padded_width;
            row = row.max(padded_height);
            placement
        };
        let (page, x, y, width, height) = placement;
        sprites.insert(name,json!({"atlas":page,"rect":[x,y,width,height],"uv":[f64::from(x)/f64::from(page_size),f64::from(y)/f64::from(page_size),f64::from(x+width)/f64::from(page_size),f64::from(y+height)/f64::from(page_size)],"image_hash":hash,"source":source,"metadata":metadata,"frame":frame}));
    }
    write_json(&output.join("textures.json"), &sprites)?;
    let mut atlas = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let file = format!("atlas-{i}.png");
        page.save_with_format(output.join(&file), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        atlas.push(json!({"file":file,"width":page_size,"height":page_size,"padding":PADDING}));
    }
    Ok(atlas)
}
