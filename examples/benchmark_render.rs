#[path = "benchmark_render/mob_farm.rs"]
mod mob_farm;

use schemora::{
    Result,
    model::{Block, Document},
    registry::MinecraftData,
    render::{GeometryAssets, SceneOptions, glb},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc, time::Instant};

fn states(entries: &[Value]) -> Result<Vec<Block>> {
    let mut result = Vec::new();
    for entry in entries {
        let mut combinations = vec![BTreeMap::new()];
        for property in entry["states"].as_array().unwrap() {
            let values: Vec<String> = if let Some(values) = property["values"].as_array() {
                values.iter().map(|v| v.as_str().unwrap().into()).collect()
            } else if property["type"] == "bool" {
                vec!["true".into(), "false".into()]
            } else {
                (0..property["num_values"].as_u64().unwrap())
                    .map(|v| v.to_string())
                    .collect()
            };
            combinations = combinations
                .into_iter()
                .flat_map(|properties| {
                    values.iter().map(move |value| {
                        let mut properties = properties.clone();
                        properties.insert(property["name"].as_str().unwrap().into(), value.clone());
                        properties
                    })
                })
                .collect();
        }
        assert_eq!(
            combinations.len() as u64,
            entry["maxStateId"].as_u64().unwrap() - entry["minStateId"].as_u64().unwrap() + 1
        );
        for properties in combinations {
            result.push(Block::new(entry["name"].as_str().unwrap(), properties)?);
        }
    }
    Ok(result)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let cache = args
        .get(1)
        .ok_or("usage: benchmark_render CACHE OUTPUT [REPEATS] [CASE]")?;
    let output = PathBuf::from(args.get(2).ok_or("missing output directory")?);
    let repeats: usize = args
        .get(3)
        .map_or(Ok(3), |n| n.parse())
        .map_err(|e| format!("Invalid repeats: {e}"))?;
    if repeats == 0 {
        return Err("Repeats must be positive".into());
    }
    fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let data = Arc::new(MinecraftData::new(Some(cache.into()), true)?);
    let blocks: Value = serde_json::from_slice(
        &fs::read(data.dataset_path("1.21.1", "blocks")?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let entities: Value = serde_json::from_slice(
        &fs::read(data.dataset_path("1.21.1", "entities")?).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let all_states = states(blocks.as_array().unwrap())?;
    let mobs: Vec<_> = entities
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            !matches!(
                e["type"].as_str().unwrap(),
                "other" | "projectile" | "player"
            )
        })
        .collect();
    let start = Instant::now();
    let assets = GeometryAssets::load(&data.visuals("1.21.1")?)?;
    let assets_ms = start.elapsed().as_secs_f64() * 1000.;
    let mut reports = BTreeMap::new();
    let cases = ["all_states", "dense_build", "repeated_mobs", "mob_farm"];
    if args
        .get(4)
        .is_some_and(|case| !cases.contains(&case.as_str()))
    {
        return Err(format!("Unknown case; expected one of {cases:?}"));
    }
    for case in cases {
        if args.get(4).is_some_and(|selected| selected != case) {
            continue;
        }
        let author_start = Instant::now();
        let mut document = Document::new("java", "1.21.1", data.clone())?;
        if case == "mob_farm" {
            mob_farm::build(&mut document)?;
        } else if case == "all_states" {
            document.set_blocks(
                "main",
                all_states.iter().enumerate().map(|(i, b)| {
                    (
                        [((i % 192) * 3) as i32, 0, ((i / 192) * 3) as i32],
                        b.clone(),
                    )
                }),
            )?;
        } else if case == "dense_build" {
            let palette = [
                "stone",
                "oak_planks",
                "bricks",
                "glass",
                "oak_stairs",
                "oak_fence",
                "redstone_wire",
            ];
            document.set_blocks(
                "main",
                (0..32768)
                    .map(|i| {
                        let p = [i % 32, (i / 1024) % 32, (i / 32) % 32];
                        let name = if p[1] < 24 {
                            palette[(p[0] / 8) as usize % 3]
                        } else {
                            palette[(i as usize) % palette.len()]
                        };
                        Ok((p, Block::new(name, BTreeMap::new())?))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )?;
        }
        if matches!(case, "all_states" | "repeated_mobs") {
            for repeat in 0..if case == "repeated_mobs" { 16 } else { 1 } {
                for (i, entity) in mobs.iter().enumerate() {
                    document.add_entity(
                        "main",
                        [
                            (i % 16) as f64 * 24.,
                            4.,
                            -24. - ((i / 16) + repeat * 6) as f64 * 24.,
                        ],
                        std::collections::HashMap::from([(
                            "id".into(),
                            fastnbt::Value::String(format!(
                                "minecraft:{}",
                                entity["name"].as_str().unwrap()
                            )),
                        )]),
                    )?;
                }
            }
        }
        let author_ms = author_start.elapsed().as_secs_f64() * 1000.;
        let warm = assets.prepare(&document, &SceneOptions::default())?;
        drop(glb::encode(&warm)?);
        let mut preparation = Vec::new();
        let mut encoding = Vec::new();
        let mut bytes = Vec::new();
        for _ in 0..repeats {
            let start = Instant::now();
            let scene = assets.prepare(&document, &SceneOptions::default())?;
            preparation.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            bytes = glb::encode(&scene)?;
            encoding.push(start.elapsed().as_secs_f64() * 1000.);
        }
        let length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let gltf: Value =
            serde_json::from_slice(&bytes[20..20 + length]).map_err(|e| e.to_string())?;
        let meshes = gltf["meshes"].as_array().unwrap();
        let mut primitives = 0;
        let mut triangles = 0;
        for node in gltf["nodes"].as_array().unwrap() {
            for primitive in meshes[node["mesh"].as_u64().unwrap() as usize]["primitives"]
                .as_array()
                .unwrap()
            {
                primitives += 1;
                triangles +=
                    gltf["accessors"][primitive["indices"].as_u64().unwrap() as usize]["count"]
                        .as_u64()
                        .unwrap()
                        / 3;
            }
        }
        let mut diagnostics = BTreeMap::<String, usize>::new();
        for diagnostic in &warm.diagnostics {
            *diagnostics.entry(diagnostic.message.clone()).or_default() += 1;
        }
        let texture_bytes: u64 = gltf["images"]
            .as_array()
            .unwrap()
            .iter()
            .map(|image| {
                gltf["bufferViews"][image["bufferView"].as_u64().unwrap() as usize]["byteLength"]
                    .as_u64()
                    .unwrap()
            })
            .sum();
        let stored_triangles: u64 = meshes
            .iter()
            .flat_map(|m| m["primitives"].as_array().unwrap())
            .map(|p| {
                gltf["accessors"][p["indices"].as_u64().unwrap() as usize]["count"]
                    .as_u64()
                    .unwrap()
                    / 3
            })
            .sum();
        let report = json!({"author_ms":author_ms,"assets_ms":assets_ms,"stored_triangles":stored_triangles,"texture_bytes":texture_bytes,"json_bytes":length,"bounds":document.regions["main"].bounds.size,"prepare_ms":median(&mut preparation),"encode_ms":median(&mut encoding),"bytes":bytes.len(),"nodes":gltf["nodes"].as_array().unwrap().len(),"meshes":meshes.len(),"materials":gltf["materials"].as_array().unwrap().len(),"primitive_instances":primitives,"triangles":triangles,"diagnostics":diagnostics,"blocks":document.regions["main"].blocks.len(),"entities":document.regions["main"].entities.len()});
        println!(
            "{case}: {} ms prepare, {} ms encode, {} nodes, {primitives} primitive instances, {} bytes",
            report["prepare_ms"],
            report["encode_ms"],
            report["nodes"],
            bytes.len()
        );
        fs::write(output.join(format!("{case}.glb")), bytes).map_err(|e| e.to_string())?;
        reports.insert(case, report);
    }
    let report = json!({"version":"1.21.1","profile":"release","repeats":repeats,"assets_ms":assets_ms,"block_types":blocks.as_array().unwrap().len(),"block_states":all_states.len(),"mob_types":mobs.len(),"cases":reports});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
