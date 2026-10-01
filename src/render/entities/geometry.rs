use crate::{
    Result,
    render::parts::{Cuboid, Face, Part},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Bone {
    name: String,
    parent: Option<String>,
    #[serde(default)]
    pivot: [f32; 3],
    #[serde(default)]
    rotation: [f32; 3],
    #[serde(default)]
    bind_pose_rotation: [f32; 3],
    #[serde(default)]
    mirror: bool,
    #[serde(default)]
    inflate: f32,
    #[serde(default, rename = "neverRender")]
    hidden: bool,
    #[serde(default)]
    cubes: Vec<Cube>,
}

#[derive(Deserialize)]
struct Cube {
    texture: Option<String>,
    origin: [f32; 3],
    size: [f32; 3],
    uv: [f32; 2],
    mirror: Option<bool>,
    inflate: Option<f32>,
    #[serde(default)]
    pivot: [f32; 3],
    #[serde(default)]
    rotation: [f32; 3],
}

fn cuboid(cube: &Cube, bone: &Bone, texture: &str) -> Result<Cuboid> {
    let [w, h, d] = cube.size;
    if cube.size.iter().any(|n| !n.is_finite() || *n < 0.) {
        return Err("Invalid entity cuboid dimensions".into());
    }
    let [u, v] = cube.uv;
    let mut rectangles = BTreeMap::from([
        ("up", [u + d, v + d, u + d + w, v]),
        ("down", [u + d + w, v, u + d + 2. * w, v + d]),
        ("north", [u + d + w, v + d, u + d, v + d + h]),
        (
            "south",
            [u + 2. * d + 2. * w, v + d, u + 2. * d + w, v + d + h],
        ),
        ("west", [u + d, v + d, u, v + d + h]),
        ("east", [u + 2. * d + w, v + d, u + d + w, v + d + h]),
    ]);
    if cube.mirror.unwrap_or(bone.mirror) {
        let east = rectangles["east"];
        let west = rectangles["west"];
        rectangles.insert("east", west);
        rectangles.insert("west", east);
        for uv in rectangles.values_mut() {
            uv.swap(0, 2);
        }
    }
    let inflate = cube.inflate.unwrap_or(bone.inflate);
    Ok(Cuboid {
        from: std::array::from_fn(|i| cube.origin[i] - cube.pivot[i] - inflate),
        to: std::array::from_fn(|i| cube.origin[i] - cube.pivot[i] + cube.size[i] + inflate),
        faces: rectangles
            .into_iter()
            .map(|(direction, uv)| {
                (
                    direction.into(),
                    Face {
                        texture: cube.texture.as_deref().unwrap_or(texture).into(),
                        uv,
                        rotation: 0,
                    },
                )
            })
            .collect(),
    })
}

pub(super) fn parts(model: &Value, texture: &str) -> Result<Part> {
    let mut bones: Vec<Bone> = serde_json::from_value(model["bones"].clone())
        .map_err(|e| format!("Unsupported entity geometry: {e}"))?;
    for bone in &mut bones {
        if bone.hidden {
            bone.cubes.clear();
        }
    }
    let mut remaining: BTreeMap<String, &Bone> = BTreeMap::new();
    for bone in &bones {
        if remaining.insert(bone.name.clone(), bone).is_some() {
            return Err("Duplicate entity bone".into());
        }
    }
    fn build(
        parent: Option<&str>,
        pivot: [f32; 3],
        remaining: &mut BTreeMap<String, &Bone>,
        texture: &str,
    ) -> Result<BTreeMap<String, Part>> {
        let names: Vec<_> = remaining
            .iter()
            .filter(|(_, b)| b.parent.as_deref() == parent)
            .map(|(n, _)| n.clone())
            .collect();
        let mut children = BTreeMap::new();
        for name in names {
            let bone = remaining.remove(&name).unwrap();
            let mut rest = Part {
                rotation: bone.bind_pose_rotation.map(|v| -v),
                ..Part::default()
            };
            for (index, cube) in bone.cubes.iter().enumerate() {
                rest.children.insert(
                    index.to_string(),
                    Part {
                        pivot: std::array::from_fn(|i| cube.pivot[i] - bone.pivot[i]),
                        rotation: cube.rotation.map(|v| -v),
                        cuboids: vec![cuboid(cube, bone, texture)?],
                        ..Part::default()
                    },
                );
            }
            let mut part = Part {
                pivot: std::array::from_fn(|i| bone.pivot[i] - pivot[i]),
                rotation: bone.rotation.map(|v| -v),
                children: build(Some(&name), bone.pivot, remaining, texture)?,
                ..Part::default()
            };
            if part.children.insert("$geometry".into(), rest).is_some() {
                return Err("Reserved entity bone name".into());
            }
            children.insert(name, part);
        }
        Ok(children)
    }
    let root = Part {
        children: build(None, [0.; 3], &mut remaining, texture)?,
        ..Part::default()
    };
    if !remaining.is_empty() {
        return Err("Missing or cyclic entity bone parent".into());
    }
    Ok(root)
}
