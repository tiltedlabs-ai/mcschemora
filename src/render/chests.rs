use super::{
    GeometryAssets, Mesh,
    parts::{Cuboid, Face, Part},
};
use crate::{Result, model::Block};
use std::collections::BTreeMap;

fn cuboid(from: [f32; 3], to: [f32; 3], texture: &str, v: f32, classic: bool) -> Cuboid {
    let [w, h, d] = std::array::from_fn(|i| to[i] - from[i]);
    let faces = if classic {
        [
            ("down", [d + w, v + d, d + 2. * w, v]),
            ("up", [d, v, d + w, v + d]),
            ("north", [2. * d + w, v + d, 2. * d + 2. * w, v + d + h]),
            ("south", [d, v + d, d + w, v + d + h]),
            ("west", [0., v + d, d, v + d + h]),
            ("east", [d + w, v + d, 2. * d + w, v + d + h]),
        ]
    } else {
        [
            ("down", [d, v, d + w, v + d]),
            ("up", [d + w, v + d, d + 2. * w, v]),
            ("north", [d + w, v + d + h, d, v + d]),
            ("south", [2. * d + 2. * w, v + d + h, 2. * d + w, v + d]),
            ("west", [d, v + d + h, 0., v + d]),
            ("east", [2. * d + w, v + d + h, d + w, v + d]),
        ]
    };
    Cuboid {
        from,
        to,
        faces: faces
            .into_iter()
            .map(|(direction, uv)| {
                (
                    direction.into(),
                    Face {
                        texture: texture.into(),
                        uv,
                        rotation: 0,
                    },
                )
            })
            .collect(),
    }
}

fn half(cuboid: &mut Cuboid, offset: f32) {
    let start = cuboid.from[0];
    let end = cuboid.to[0];
    let left = start.max(offset);
    let right = end.min(offset + 16.);
    for (direction, face) in &mut cuboid.faces {
        let (a, b) = match direction.as_str() {
            "up" | "down" | "south" => (
                (left - start) / (end - start),
                (right - start) / (end - start),
            ),
            "north" => ((end - right) / (end - start), (end - left) / (end - start)),
            _ => continue,
        };
        let [u0, v0, u1, v1] = face.uv;
        face.uv = [u0 + (u1 - u0) * a, v0, u0 + (u1 - u0) * b, v1];
    }
    if left > start {
        cuboid.faces.remove("west");
    }
    if right < end {
        cuboid.faces.remove("east");
    }
    cuboid.from[0] = left - offset;
    cuboid.to[0] = right - offset;
}

pub(super) fn bake(assets: &GeometryAssets, block: &Block) -> Result<Mesh> {
    let kind = match block.name.as_str() {
        "minecraft:chest" => "normal",
        "minecraft:trapped_chest" => "trapped",
        "minecraft:ender_chest" => "ender",
        _ => return Err("Unsupported chest kind".into()),
    };
    let facing = block
        .properties
        .get("facing")
        .map(String::as_str)
        .unwrap_or("north");
    let angle = match facing {
        "south" => 0.,
        "east" => 90.,
        "north" => 180.,
        "west" => 270.,
        _ => return Err(format!("Invalid chest facing {facing}")),
    };
    let side = if kind == "ender" {
        "single"
    } else {
        block
            .properties
            .get("type")
            .map(String::as_str)
            .unwrap_or("single")
    };
    let (x, width, latch, latch_width) = match side {
        "single" => (1., 14., 7., 2.),
        "left" => (0., 15., 0., 1.),
        "right" => (1., 15., 15., 1.),
        _ => return Err(format!("Invalid chest type {side}")),
    };
    let classic = assets
        .texture_ids
        .contains_key("minecraft:entity/chest/normal_double");
    let double = side != "single";
    let suffix = if double {
        if classic {
            "_double"
        } else if side == "left" {
            "_left"
        } else {
            "_right"
        }
    } else {
        ""
    };
    let texture = format!("minecraft:entity/chest/{kind}{suffix}");
    let (x, width, latch, latch_width) = if classic && double {
        (1., 30., 15., 2.)
    } else {
        (x, width, latch, latch_width)
    };
    let mut bottom = cuboid([x, 0., 1.], [x + width, 10., 15.], &texture, 19., classic);
    let mut lid = cuboid([x, 0., 0.], [x + width, 5., 14.], &texture, 0., classic);
    let mut lock = cuboid(
        [latch, -2., 14.],
        [latch + latch_width, 2., 15.],
        &texture,
        0.,
        classic,
    );
    if classic && double {
        let offset = if side == "left" { 16. } else { 0. };
        for part in [&mut bottom, &mut lid, &mut lock] {
            half(part, offset);
        }
    }
    let model = Part {
        pivot: [-8., 0., -8.],
        cuboids: vec![bottom],
        children: BTreeMap::from([(
            "lid".into(),
            Part {
                pivot: [0., 9., 1.],
                cuboids: vec![lid, lock],
                ..Part::default()
            },
        )]),
        ..Part::default()
    };
    assets.bake_parts(&Part {
        pivot: [8., 0., 8.],
        rotation: [0., angle, 0.],
        children: BTreeMap::from([("chest".into(), model)]),
        ..Part::default()
    })
}
