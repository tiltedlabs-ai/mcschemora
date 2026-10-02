use schemora::{
    Result,
    model::{Block, Document},
    registry::MinecraftData,
    render::{GeometryAssets, SceneOptions, glb, png},
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn main() -> Result<()> {
    let cache = std::env::args_os().nth(1).map(PathBuf::from);
    let data = Arc::new(MinecraftData::new(cache.clone(), cache.is_some())?);
    let assets = GeometryAssets::load(&data.visuals("1.21.1")?)?;
    let fluid = |name: &str, level: u8| {
        Block::new(name, BTreeMap::from([("level".into(), level.to_string())]))
    };
    let mut pair = Document::new("java", "1.21.1", data.clone())?;
    pair.set_blocks(
        "main",
        [
            ([0, 0, 0], fluid("water", 0)?),
            ([1, 0, 0], fluid("water", 5)?),
        ],
    )?;
    let scene = assets.prepare(&pair, &SceneOptions::default())?;
    assert!(scene.diagnostics.is_empty());
    assert_eq!(
        scene
            .instances
            .iter()
            .flat_map(|i| &i.draws)
            .map(|d| d.quads.len())
            .sum::<usize>(),
        10
    );
    let tops: Vec<_> = scene
        .instances
        .iter()
        .map(|i| {
            let mesh = &scene.meshes[i.draws[0].mesh];
            let top = mesh.quads.iter().find(|q| q.normal[1] > 0.).unwrap();
            top.vertices.map(|v| {
                [
                    v.position[0] + i.position[0] as f32,
                    v.position[1],
                    v.position[2],
                ]
            })
        })
        .collect();
    for p in tops[0].iter().filter(|p| p[0] == 1.) {
        assert!(tops[1].contains(p));
    }
    pair.set_blocks("main", [([0, 1, 0], fluid("water", 0)?)])?;
    let scene = assets.prepare(&pair, &SceneOptions::default())?;
    let lower = scene
        .instances
        .iter()
        .find(|i| i.position == [0.; 3])
        .unwrap();
    assert!(
        scene.meshes[lower.draws[0].mesh]
            .quads
            .iter()
            .all(|q| q.normal[1] <= 0.)
    );
    let slice = assets.prepare(
        &pair,
        &SceneOptions {
            region: None,
            y: Some([0, 0]),
            ..SceneOptions::default()
        },
    )?;
    assert!(
        slice.meshes[slice.instances[0].draws[0].mesh]
            .quads
            .iter()
            .any(|q| q.normal[1] > 0.)
    );

    let mut document = Document::new("java", "1.21.1", data)?;
    document.set_blocks(
        "main",
        (-1..18).flat_map(|x| {
            (-1..9).map(move |z| {
                (
                    [x, 0, z],
                    Block::new("stone_bricks", BTreeMap::new()).unwrap(),
                )
            })
        }),
    )?;
    for (z, name) in [(0, "water"), (5, "lava")] {
        for level in 0..16u8 {
            document.set_blocks("main", [([i32::from(level), 1, z], fluid(name, level)?)])?;
        }
        for x in 0..4 {
            for dz in 1..4 {
                document.set_blocks("main", [([x, 1, z + dz], fluid(name, 0)?)])?;
            }
        }
        for y in 2..5 {
            document.set_blocks("main", [([0, y, z + 2], fluid(name, 8)?)])?;
        }
    }
    for (x, name, extra) in [
        (6, "oak_slab", ("type", "bottom")),
        (8, "oak_slab", ("type", "top")),
        (10, "oak_stairs", ("facing", "south")),
        (12, "oak_fence", ("east", "true")),
    ] {
        document.set_blocks(
            "main",
            [(
                [x, 1, 3],
                Block::new(
                    name,
                    BTreeMap::from([
                        ("waterlogged".into(), "true".into()),
                        (extra.0.into(), extra.1.into()),
                    ]),
                )?,
            )],
        )?;
    }
    let scene = assets.prepare(&document, &SceneOptions::default())?;
    assert!(scene.diagnostics.is_empty());
    assert!(
        scene
            .meshes
            .iter()
            .flat_map(|m| &m.quads)
            .all(|q| scene.textures[q.texture].name != "minecraft:missingno")
    );
    for x in [6, 8, 10, 12] {
        assert_eq!(
            scene
                .instances
                .iter()
                .filter(|i| i.position == [f64::from(x), 1., 3.])
                .count(),
            2
        );
    }
    let output = PathBuf::from("examples/output");
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    std::fs::write(output.join("fluids.glb"), glb::encode(&scene)?).map_err(|e| e.to_string())?;
    std::fs::write(
        output.join("fluids.png"),
        png::encode(
            &scene,
            &png::Options {
                size: [1400, 900],
                ..Default::default()
            },
        )?,
    )
    .map_err(|e| e.to_string())?;
    println!("Fluid levels, shared edges, internal faces, stacked fluids, and Y selection passed.");
    Ok(())
}
