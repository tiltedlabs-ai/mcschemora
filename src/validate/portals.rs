use super::{Check, DOWN, Point, SIDES, Severity, UP, add, facing, name, neg, prop};
use crate::model::Block;
use std::collections::HashSet;

// Portal components are traversed once. A malformed component never causes an
// unbounded rectangular volume scan: Nether openings are at most 21 by 21.
pub(super) fn check(c: &mut Check<'_, '_>, b: &Block, visited: &mut HashSet<Point>) {
    let mut component = vec![c.cell.point];
    visited.insert(c.cell.point);
    let nether = name(b) == "nether_portal";
    let dirs: &[Point] = if nether {
        &[UP, DOWN, [1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]]
    } else {
        &[[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]]
    };
    let mut i = 0;
    while i < component.len() {
        for &d in dirs {
            let p = add(component[i], d);
            // Components contain stored portal blocks only. Do not scan region
            // bounds to distinguish air from unknown space during this search.
            if c.scene.state_at(p).is_some_and(|state| {
                state.valid
                    && state.block.id == b.id
                    && (!nether || prop(&state.block, "axis") == prop(b, "axis"))
            }) && visited.insert(p)
            {
                component.push(p);
            }
        }
        i += 1;
    }
    let mut min = component[0];
    let mut max = min;
    for p in &component {
        for a in 0..3 {
            min[a] = min[a].min(p[a]);
            max[a] = max[a].max(p[a]);
        }
    }
    if nether {
        check_nether_portal(c, b, &component, min, max);
    } else {
        check_end_portal(c, &component, min, max);
    }
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/portal/PortalShape.java
fn check_nether_portal(
    c: &mut Check<'_, '_>,
    b: &Block,
    component: &[Point],
    min: Point,
    max: Point,
) {
    let a = if prop(b, "axis") == "x" { 0 } else { 2 };
    let other = 2 - a;
    let width = max[a] - min[a] + 1;
    let height = max[1] - min[1] + 1;
    if max[other] != min[other] || width > 21 || height > 21 {
        c.emit(
            "portal.nether",
            "portal must be a flat opening at most 21 by 21",
            Severity::Error,
        );
        return;
    }
    let mut unknown = false;
    let incomplete = width < 2 || height < 3 || component.len() as i64 != width * height;
    let mut invalid = false;
    for y in -1..=height {
        for x in -1..=width {
            let edge_x = x == -1 || x == width;
            let edge_y = y == -1 || y == height;
            if edge_x && edge_y {
                continue;
            } // Frame corners are optional.
            let mut p = min;
            p[a] += x;
            p[1] += y;
            match c.scene.get(p) {
                None => unknown = true,
                Some(o) => {
                    invalid |= if edge_x || edge_y {
                        name(o) != "obsidian"
                    } else {
                        name(o) != "nether_portal" || prop(o, "axis") != prop(b, "axis")
                    }
                }
            }
        }
    }
    // A cropped portal component may continue outside the schematic.
    c.require(
        "portal.nether",
        if invalid {
            Some(false)
        } else if unknown {
            None
        } else {
            Some(!incomplete)
        },
        "invalid obsidian frame, opening, or portal axis",
    );
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/EndPortalFrameBlock.java
// The End exit fountain also uses end_portal blocks, without entry frame blocks.
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/levelgen/feature/EndPodiumFeature.java
fn check_end_portal(c: &mut Check<'_, '_>, component: &[Point], min: Point, max: Point) {
    if max[0] - min[0] == 2 && max[2] - min[2] == 2 && component.len() == 9 {
        let mut valid = true;
        let mut unknown = false;
        for (_, d) in SIDES {
            for k in 0..3 {
                let p = if d[0] == 0 {
                    [
                        min[0] + k,
                        min[1],
                        if d[2] < 0 { min[2] - 1 } else { max[2] + 1 },
                    ]
                } else {
                    [
                        if d[0] < 0 { min[0] - 1 } else { max[0] + 1 },
                        min[1],
                        min[2] + k,
                    ]
                };
                match c.scene.get(p) {
                    None => unknown = true,
                    Some(o) => {
                        valid &= name(o) == "end_portal_frame"
                            && prop(o, "eye") == "true"
                            && facing(o) == neg(d)
                    }
                }
            }
        }
        c.require(
            "portal.end",
            if !valid {
                Some(false)
            } else if unknown {
                None
            } else {
                Some(true)
            },
            "entry portal requires twelve inward-facing frames with eyes",
        );
    } else if max[0] - min[0] == 4 && max[2] - min[2] == 4 && component.len() == 20 {
        let center = [min[0] + 2, min[1], min[2] + 2];
        let mut valid = true;
        let mut unknown = false;
        for x in -3..=3 {
            for z in -3..=3 {
                let distance = (x * x + z * z) as f64;
                if distance >= 12.25 {
                    continue;
                }
                let p = add(center, [x, 0, z]);
                let expected = if distance > 6.25 || (x == 0 && z == 0) {
                    "bedrock"
                } else {
                    "end_portal"
                };
                match c.scene.get(p) {
                    Some(o) => valid &= name(o) == expected,
                    None => unknown = true,
                }
            }
        }
        c.require(
            "portal.end_exit",
            if !valid {
                Some(false)
            } else if unknown {
                None
            } else {
                Some(true)
            },
            "exit portal does not match the bedrock fountain opening",
        );
    } else {
        let boundary_unknown = component
            .iter()
            .any(|&p| SIDES.iter().any(|(_, d)| c.scene.get(add(p, *d)).is_none()));
        c.require(
            "portal.end",
            if boundary_unknown { None } else { Some(false) },
            "portal is neither a 3 by 3 entry opening nor an End exit opening",
        );
    }
}
