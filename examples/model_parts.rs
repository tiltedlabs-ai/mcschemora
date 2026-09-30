use schemora::{
    Result,
    model::Document,
    registry::MinecraftData,
    render::{
        Draw, GeometryAssets, Instance, SceneOptions, glb,
        parts::{Cuboid, Face, Part},
    },
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn cuboid(size: f32, texture: &str) -> Cuboid {
    Cuboid {
        from: [0.; 3],
        to: [size; 3],
        faces: ["down", "up", "north", "south", "west", "east"]
            .into_iter()
            .map(|direction| {
                (
                    direction.into(),
                    Face {
                        texture: texture.into(),
                        uv: [16., 0., 0., 16.],
                        rotation: if direction == "up" { 90 } else { 0 },
                    },
                )
            })
            .collect(),
    }
}

fn main() -> Result<()> {
    let cache = std::env::args_os().nth(1).map(PathBuf::from);
    let data = Arc::new(MinecraftData::new(cache.clone(), cache.is_some())?);
    let assets = GeometryAssets::load(&data.visuals("1.21.1")?)?;
    let mut root = Part {
        pivot: [16., 0., 0.],
        rotation: [0., 90., 0.],
        scale: [2., 1., 1.],
        cuboids: vec![cuboid(8., "minecraft:entity/pig/pig")],
        children: BTreeMap::from([(
            "arm".into(),
            Part {
                pivot: [8., 8., 0.],
                rotation: [0., 0., 90.],
                scale: [1., 2., 1.],
                cuboids: vec![cuboid(4., "minecraft:block/oak_planks")],
                children: BTreeMap::from([(
                    "tip".into(),
                    Part {
                        pivot: [4., 0., 0.],
                        cuboids: vec![cuboid(2., "minecraft:block/stone")],
                        ..Part::default()
                    },
                )]),
            },
        )]),
    };
    let mesh = assets.bake_parts(&root)?;
    assert_eq!(mesh.quads.len(), 18);
    assert!(!mesh.occludes && mesh.quads.iter().all(|q| q.cull_face.is_none()));
    let bounds = [
        ([1., 0., -1.], [1.5, 0.5, 0.]),
        ([1., 0.5, -1.], [1.25, 0.75, 0.]),
        ([1., 0.75, -1.], [1.125, 0.875, -0.5]),
    ];
    for (quads, (expected_min, expected_max)) in mesh.quads.chunks(6).zip(bounds) {
        for axis in 0..3 {
            let values: Vec<_> = quads
                .iter()
                .flat_map(|q| q.vertices.iter().map(|v| v.position[axis]))
                .collect();
            assert!(
                (values.iter().copied().fold(f32::INFINITY, f32::min) - expected_min[axis]).abs()
                    < 1e-5
            );
            assert!(
                (values.iter().copied().fold(f32::NEG_INFINITY, f32::max) - expected_max[axis])
                    .abs()
                    < 1e-5
            );
        }
    }
    for quad in &mesh.quads {
        assert!((quad.normal.iter().map(|n| n * n).sum::<f32>() - 1.).abs() < 1e-5);
    }
    let uv_max: [f32; 2] = std::array::from_fn(|axis| {
        mesh.quads[..6]
            .iter()
            .flat_map(|q| q.vertices.iter().map(|v| v.uv[axis]))
            .fold(0., f32::max)
    });
    assert_eq!(uv_max, [0.25, 0.5]);
    root.children.get_mut("arm").unwrap().rotation[2] = 30.;
    let posed = assets.bake_parts(&root)?;
    assert!(posed.quads[6..].iter().zip(&mesh.quads[6..]).any(|(a, b)| {
        a.vertices
            .iter()
            .zip(&b.vertices)
            .any(|(a, b)| a.position != b.position)
    }));
    assert_eq!(
        posed.quads[0].vertices[0].position,
        mesh.quads[0].vertices[0].position
    );
    root.scale[0] = 0.;
    assert!(assets.bake_parts(&root).is_err());
    let document = Document::new("java", "1.21.1", data)?;
    let mut scene = assets.prepare(&document, &SceneOptions::default())?;
    scene.meshes = vec![mesh, posed];
    scene.instances = (0..2)
        .map(|mesh| Instance {
            position: [mesh as i32 * 3, 0, 0],
            block: format!("model_parts_{mesh}"),
            draws: vec![Draw {
                mesh,
                quads: (0..18).collect(),
            }],
        })
        .collect();
    let output = PathBuf::from("examples/output/model_parts.glb");
    std::fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&output, glb::encode(&scene)?).map_err(|e| e.to_string())?;
    println!(
        "Model part hierarchy, poses, pixel UVs, and normals passed: {}",
        output.display()
    );
    Ok(())
}
