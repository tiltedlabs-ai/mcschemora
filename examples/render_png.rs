#[path = "benchmark_render/mob_farm.rs"]
mod mob_farm;

use schemora::{
    Result,
    model::{Block, Document},
    registry::MinecraftData,
    render::{GeometryAssets, SceneOptions, View, png},
};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

fn benchmark(
    name: &str,
    document: &Document,
    assets: &GeometryAssets,
    view: View,
) -> Result<serde_json::Value> {
    let mut results = Vec::new();
    for size in [512, 1024, 1536, 2048] {
        let options = png::Options {
            size: [size; 2],
            view,
            grid: false,
        };
        let warm = assets.prepare(document, &SceneOptions::default())?;
        drop(png::encode(&warm, &options)?);
        let mut prepare = Vec::new();
        let mut encode = Vec::new();
        let mut total = Vec::new();
        let mut bytes = 0;
        for _ in 0..7 {
            let start = Instant::now();
            let scene = assets.prepare(document, &SceneOptions::default())?;
            let prepared = start.elapsed().as_secs_f64() * 1000.;
            let encoded = Instant::now();
            let image = png::encode(&scene, &options)?;
            encode.push(encoded.elapsed().as_secs_f64() * 1000.);
            total.push(start.elapsed().as_secs_f64() * 1000.);
            prepare.push(prepared);
            bytes = image.len();
        }
        let summary = |mut values: Vec<f64>| {
            values.sort_by(f64::total_cmp);
            serde_json::json!({"min":values[0],"median":values[3],"max":values[6]})
        };
        let report = serde_json::json!({
            "size": size, "prepare_ms": summary(prepare),
            "png_ms": summary(encode), "total_ms": summary(total), "bytes": bytes
        });
        println!("{name}: {report}");
        results.push(report);
    }
    Ok(serde_json::json!({
        "blocks": document.regions["main"].blocks.len(),
        "entities": document.regions["main"].entities.len(),
        "measurements":results
    }))
}

fn main() -> Result<()> {
    let cache = std::env::args_os().nth(1).map(PathBuf::from);
    let data = Arc::new(MinecraftData::new(cache.clone(), cache.is_some())?);
    let assets = GeometryAssets::load(&data.visuals("1.21.1")?)?;
    let output = PathBuf::from("examples/output");
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let mut document = Document::new("java", "1.21.1", data.clone())?;
    let block = |name: &str, properties: &[(&str, &str)]| {
        Block::new(
            name,
            properties
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    };
    document.set_blocks(
        "main",
        (-4..=4).flat_map(|x| {
            (-4..=4).map(move |z| {
                (
                    [x, 0, z],
                    Block::new("stone_bricks", BTreeMap::new()).unwrap(),
                )
            })
        }),
    )?;
    document.set_blocks(
        "main",
        [
            ([-2, 1, 0], block("oak_stairs", &[("facing", "south")])?),
            (
                [-1, 1, 0],
                block("oak_fence", &[("east", "true"), ("west", "true")])?,
            ),
            ([0, 1, 0], block("chest", &[("facing", "south")])?),
            ([2, 1, 0], block("red_stained_glass", &[])?),
            ([2, 1, 1], block("blue_stained_glass", &[])?),
            ([2, 1, 2], block("glass", &[])?),
            ([-2, 1, 2], block("dandelion", &[])?),
            ([0, 1, -2], block("oak_log", &[])?),
            ([0, 2, -2], block("oak_leaves", &[])?),
        ],
    )?;
    for (name, position) in [("pig", [-2., 1., -2.]), ("villager", [2., 1., -2.])] {
        document.add_entity(
            "main",
            position,
            HashMap::from([(
                "id".into(),
                fastnbt::Value::String(format!("minecraft:{name}")),
            )]),
        )?;
    }
    let options = png::Options {
        size: [960, 720],
        ..Default::default()
    };
    let scene = assets.prepare(&document, &SceneOptions::default())?;
    let start = Instant::now();
    let bytes = png::encode(&scene, &options)?;
    println!(
        "Gallery PNG: {:.1} ms, {} bytes",
        start.elapsed().as_secs_f64() * 1000.,
        bytes.len()
    );
    let image = image::load_from_memory(&bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    assert_eq!(image.dimensions(), (960, 720));
    assert_eq!(image.get_pixel(0, 0).0, [0; 4]);
    assert!(image.pixels().any(|p| p[3] == 255));
    assert!(
        png::encode(
            &scene,
            &png::Options {
                size: [0, 720],
                ..Default::default()
            }
        )
        .is_err()
    );
    std::fs::write(output.join("render.png"), &bytes).map_err(|e| e.to_string())?;
    let top_options = png::Options {
        size: [960, 720],
        view: View::Top,
        grid: false,
    };
    let top = png::encode(&scene, &top_options)?;
    std::fs::write(output.join("render-top-down.png"), &top).map_err(|e| e.to_string())?;
    for (view, file) in [
        (View::Top, "render-top-down-grid.png"),
        (View::Isometric, "render-grid.png"),
    ] {
        let bytes = png::encode(
            &scene,
            &png::Options {
                size: [960, 720],
                view,
                grid: true,
            },
        )?;
        std::fs::write(output.join(file), bytes).map_err(|e| e.to_string())?;
    }
    let mut reversed_top = scene.clone();
    reversed_top.instances.reverse();
    assert_eq!(top, png::encode(&reversed_top, &top_options)?);
    for size in [[1, 1], [7, 19], [513, 257]] {
        let bytes = png::encode(
            &scene,
            &png::Options {
                size,
                view: View::Top,
                grid: false,
            },
        )?;
        let image = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        assert_eq!([image.width(), image.height()], size);
    }
    let mut shifted = document.clone();
    shifted.regions.get_mut("main").unwrap().origin = [1_000_000, -500_000, -1_000_000];
    assert_eq!(
        bytes,
        png::encode(
            &assets.prepare(&shifted, &SceneOptions::default())?,
            &options
        )?
    );
    let mut reversed = scene.clone();
    reversed.instances.reverse();
    assert_eq!(bytes, png::encode(&reversed, &options)?);
    let benchmark_gallery = if std::env::args().any(|arg| arg == "--bench") {
        Some(benchmark("gallery", &document, &assets, View::Isometric)?)
    } else {
        None
    };
    document.add_region("distant", [1000, 0, 0])?;
    document.set_blocks("distant", [([0; 3], block("stone", &[])?)])?;
    assert_eq!(
        bytes,
        png::encode(
            &assets.prepare(
                &document,
                &SceneOptions {
                    region: Some("main".into()),
                    y: None,
                    ..SceneOptions::default()
                }
            )?,
            &options
        )?
    );
    let layer = assets.prepare(
        &document,
        &SceneOptions {
            region: Some("main".into()),
            y: Some([0, 0]),
            ..SceneOptions::default()
        },
    )?;
    let bytes = png::encode(&layer, &options)?;
    std::fs::write(output.join("render-layer.png"), bytes).map_err(|e| e.to_string())?;
    let mut farm = Document::new("java", "1.21.1", data)?;
    mob_farm::build(&mut farm)?;
    let scene = assets.prepare(&farm, &SceneOptions::default())?;
    let start = Instant::now();
    let bytes = png::encode(
        &scene,
        &png::Options {
            size: [1536; 2],
            ..Default::default()
        },
    )?;
    println!(
        "Mob farm PNG: {:.1} ms, {} bytes",
        start.elapsed().as_secs_f64() * 1000.,
        bytes.len()
    );
    std::fs::write(output.join("mob-farm.png"), bytes).map_err(|e| e.to_string())?;
    std::fs::write(
        output.join("mob-farm-top-down.png"),
        png::encode(
            &scene,
            &png::Options {
                size: [1536; 2],
                view: View::Top,
                grid: false,
            },
        )?,
    )
    .map_err(|e| e.to_string())?;
    if let Some(gallery) = benchmark_gallery {
        let top_down = benchmark("mob_farm_top_down", &farm, &assets, View::Top)?;
        let farm = benchmark("mob_farm", &farm, &assets, View::Isometric)?;
        let report = serde_json::json!({
            "repeats":7,"warmups":1,"version":"1.21.1",
            "gallery":gallery,"mob_farm":farm,"mob_farm_top_down":top_down
        });
        std::fs::write(
            output.join("png-benchmark.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    println!(
        "Framing, depth ordering, large translations, region selection, and Y filtering passed."
    );
    Ok(())
}
