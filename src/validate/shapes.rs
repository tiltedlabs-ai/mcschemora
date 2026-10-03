use super::{DOWN, Point, UP, name, prop};
use crate::{catalog::Registry, model::Block};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ShapeIds {
    One(u32),
    States(Vec<u32>),
}

/// https://github.com/PrismarineJS/minecraft-data/blob/master/schematic/blockCollisionShapes.md
#[derive(Debug, Deserialize)]
pub(crate) struct Shapes {
    blocks: HashMap<String, ShapeIds>,
    shapes: HashMap<u32, Vec<[f64; 6]>>,
}

#[derive(Clone, Copy)]
pub(super) enum Support {
    Full,
    Center,
    Rigid,
}

impl Shapes {
    pub(super) fn boxes(&self, b: &Block, registry: &Registry) -> Option<&[[f64; 6]]> {
        let id = match self.blocks.get(name(b))? {
            ShapeIds::One(id) => *id,
            ShapeIds::States(ids) => {
                let (offset, count) = registry.state_offset(b)?;
                // Some upstream versions reuse older shape tables. Never index
                // a different state layout as though it were this version's.
                if count != ids.len() {
                    return None;
                }
                *ids.get(offset)?
            }
        };
        self.shapes.get(&id).map(Vec::as_slice)
    }
}

// Subtract covered rectangles. This handles unions of boxes without rounding
// coordinates to a voxel grid (some shapes use fractions smaller than 1/16).
fn covered(rects: &[[f64; 4]], target: [f64; 4]) -> bool {
    if rects
        .iter()
        .any(|r| r[0] <= target[0] && r[1] <= target[1] && r[2] >= target[2] && r[3] >= target[3])
    {
        return true;
    }
    let mut remaining = vec![target];
    let mut next = Vec::new();
    for &[x0, z0, x1, z1] in rects {
        next.clear();
        for [a, b, c, d] in remaining.drain(..) {
            let (l, t, r, u) = (a.max(x0), b.max(z0), c.min(x1), d.min(z1));
            if l >= r || t >= u {
                next.push([a, b, c, d]);
                continue;
            }
            for piece in [[a, b, l, d], [r, b, c, d], [l, b, r, t], [l, u, r, d]] {
                if piece[0] < piece[2] && piece[1] < piece[3] {
                    next.push(piece);
                }
            }
        }
        std::mem::swap(&mut remaining, &mut next);
        if remaining.is_empty() {
            return true;
        }
    }
    false
}

// Minecraft distinguishes full faces, a center post, and an outer support rim.
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/SupportType.java
pub(super) fn face_rectangles(boxes: &[[f64; 6]], face: Point) -> Vec<[f64; 4]> {
    let axis = face.iter().position(|&n| n != 0).unwrap();
    let plane = if face[axis] > 0 { 1.0 } else { 0.0 };
    let [u, v] = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    boxes
        .iter()
        .filter(|b| b[axis] <= plane && b[axis + 3] >= plane)
        .map(|b| [b[u], b[v], b[u + 3], b[v + 3]])
        .collect()
}

fn shape_support(rects: &[[f64; 4]], kind: Support) -> bool {
    if rects.is_empty() {
        return false;
    }
    match kind {
        Support::Full => covered(rects, [0., 0., 1., 1.]),
        Support::Center => covered(rects, [7. / 16., 7. / 16., 9. / 16., 9. / 16.]),
        Support::Rigid => [
            [0., 0., 0.125, 1.],
            [0.875, 0., 1., 1.],
            [0.125, 0., 0.875, 0.125],
            [0.125, 0.875, 0.875, 1.],
        ]
        .into_iter()
        .all(|r| covered(rects, r)),
    }
}

/// North, east, south, west arm coverage, followed by the center post.
pub(super) fn wall_cover(rects: &[[f64; 4]]) -> [bool; 5] {
    [
        [7. / 16., 0., 9. / 16., 9. / 16.],
        [7. / 16., 7. / 16., 1., 9. / 16.],
        [7. / 16., 7. / 16., 9. / 16., 1.],
        [0., 7. / 16., 9. / 16., 9. / 16.],
        [7. / 16., 7. / 16., 9. / 16., 9. / 16.],
    ]
    .map(|target| covered(rects, target))
}

pub(super) fn face_index(face: Point) -> usize {
    let axis = face.iter().position(|&n| n != 0).unwrap();
    axis * 2 + usize::from(face[axis] > 0)
}

pub(super) fn support(
    b: &Block,
    rects: Option<&[[f64; 4]]>,
    face: Point,
    kind: Support,
) -> Option<bool> {
    let n = name(b);
    if b.is_air() || matches!(n, "water" | "lava") {
        return Some(false);
    }
    // Leaves blocks override support shape or use world-dependent shapes.
    // LeavesBlock.getBlockSupportShape returns an empty shape.
    // https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/LeavesBlock.java
    if n.ends_with("_leaves") {
        return Some(false);
    }
    if matches!(n, "moving_piston" | "scaffolding" | "powder_snow") {
        return None;
    }
    // Gates have no lower support, even when their collision box touches y=0.
    // https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/FenceGateBlock.java
    if n.ends_with("_fence_gate") && face == DOWN {
        return Some(false);
    }
    if n == "soul_sand" {
        return Some(true);
    }
    if n == "snow" && face == UP {
        return Some(prop(b, "layers") == "8");
    }
    if n.ends_with("_fence") || n.ends_with("_wall") {
        return Some(face == UP && matches!(kind, Support::Center));
    }
    Some(shape_support(rects?, kind))
}
