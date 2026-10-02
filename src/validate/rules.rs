use super::shapes::Support;
use super::{Check, DOWN, SIDES, Severity, UP, add, clockwise, facing, name, neg, prop};
use crate::model::{Block, direction};

pub(super) type Rule = fn(&mut Check<'_, '_>, &Block);

/// Choose checks once per distinct state, instead of testing every rule at
/// every coordinate. Individual checks retain their guards for clarity.
pub(super) fn for_block(block: &Block) -> Vec<Rule> {
    let n = name(block);
    let support = matches!(
        n,
        "torch"
            | "soul_torch"
            | "redstone_torch"
            | "wall_torch"
            | "soul_wall_torch"
            | "redstone_wall_torch"
            | "ladder"
            | "tripwire_hook"
            | "lever"
            | "rail"
            | "powered_rail"
            | "detector_rail"
            | "activator_rail"
            | "repeater"
            | "comparator"
            | "redstone_wire"
            | "lantern"
            | "soul_lantern"
    ) || n.ends_with("_button")
        || n.ends_with("_pressure_plate")
        || (n.ends_with("_sign") && !n.ends_with("_hanging_sign"));
    let rules: [(bool, Rule); 12] = [
        (n.ends_with("_bed"), check_bed),
        (n.ends_with("_door"), check_door),
        (tall_plant(n), check_tall_plant),
        (
            matches!(n, "chest" | "trapped_chest") && prop(block, "type") != "single",
            check_chest,
        ),
        (
            matches!(
                n,
                "piston" | "sticky_piston" | "piston_head" | "moving_piston"
            ),
            check_piston,
        ),
        (support, check_support),
        (
            crop(n) || soil_plant(n) || matches!(n, "nether_wart" | "sugar_cane" | "cactus"),
            check_plant,
        ),
        (falls(n), check_gravity),
        (
            n.ends_with("_fence")
                || n.ends_with("_wall")
                || n == "iron_bars"
                || n.ends_with("glass_pane"),
            check_connections,
        ),
        (n.ends_with("_wall"), check_wall_height),
        (n.ends_with("_stairs"), check_stairs),
        (n == "redstone_wire", check_redstone),
    ];
    rules
        .into_iter()
        .filter_map(|(applies, rule)| applies.then_some(rule))
        .collect()
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/BedBlock.java
fn check_bed(c: &mut Check<'_, '_>, b: &Block) {
    if !name(b).ends_with("_bed") {
        return;
    }
    let part = prop(b, "part");
    let other_part = if part == "foot" { "head" } else { "foot" };
    let d = if part == "foot" {
        facing(b)
    } else {
        neg(facing(b))
    };
    c.require(
        "bed.pair",
        c.at(d).map(|o| {
            o.name == b.name
                && prop(o, "part") == other_part
                && prop(o, "facing") == prop(b, "facing")
        }),
        "incomplete bed or mismatched direction",
    );
    // Beds do not have a canSurvive support requirement. Floating beds are valid.
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/DoorBlock.java
fn check_door(c: &mut Check<'_, '_>, b: &Block) {
    if !name(b).ends_with("_door") {
        return;
    }
    let lower = prop(b, "half") == "lower";
    c.require(
        "door.pair",
        c.at(if lower { UP } else { DOWN }).map(|o| {
            o.name == b.name
                && prop(o, "half") == if lower { "upper" } else { "lower" }
                && ["facing", "hinge", "open", "powered"]
                    .iter()
                    .all(|k| prop(o, k) == prop(b, k))
        }),
        "incomplete door or mismatched halves",
    );
    if lower {
        c.needs_support(DOWN, UP, Support::Full);
    }
}

fn tall_plant(n: &str) -> bool {
    matches!(
        n,
        "sunflower"
            | "lilac"
            | "rose_bush"
            | "peony"
            | "tall_grass"
            | "large_fern"
            | "tall_seagrass"
            | "small_dripleaf"
            | "pitcher_plant"
            | "pitcher_crop"
    )
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/DoublePlantBlock.java
fn check_tall_plant(c: &mut Check<'_, '_>, b: &Block) {
    if !tall_plant(name(b)) {
        return;
    }
    if name(b) == "pitcher_crop" && prop(b, "age").parse::<u8>().unwrap_or(0) < 3 {
        return;
    }
    let lower = prop(b, "half") == "lower";
    c.require(
        "plant.pair",
        c.at(if lower { UP } else { DOWN }).map(|o| {
            o.name == b.name
                && prop(o, "half") == if lower { "upper" } else { "lower" }
                && prop(o, "age") == prop(b, "age")
        }),
        "missing or mismatched plant half",
    );
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/ChestBlock.java
fn check_chest(c: &mut Check<'_, '_>, b: &Block) {
    if !matches!(name(b), "chest" | "trapped_chest") || prop(b, "type") == "single" {
        return;
    }
    let left = prop(b, "type") == "left";
    let d = clockwise(facing(b));
    c.require(
        "chest.pair",
        c.at(if left { d } else { neg(d) }).map(|o| {
            o.name == b.name
                && prop(o, "facing") == prop(b, "facing")
                && prop(o, "type") == if left { "right" } else { "left" }
        }),
        "missing or mismatched double-chest half",
    );
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/piston/PistonHeadBlock.java
fn check_piston(c: &mut Check<'_, '_>, b: &Block) {
    let n = name(b);
    if n == "piston_head" {
        let expected = if prop(b, "type") == "sticky" {
            "sticky_piston"
        } else {
            "piston"
        };
        let result = c.at(neg(facing(b))).map(|o| {
            (name(o) == "moving_piston" && facing(o) == facing(b))
                || (name(o) == expected && prop(o, "extended") == "true" && facing(o) == facing(b))
        });
        c.require(
            "piston.head",
            result,
            "head has no matching extended piston",
        );
    } else if matches!(n, "piston" | "sticky_piston") && prop(b, "extended") == "true" {
        let result = c.at(facing(b)).map(|o| {
            (name(o) == "moving_piston" && facing(o) == facing(b))
                || (name(o) == "piston_head"
                    && facing(o) == facing(b)
                    && (prop(o, "type") == "sticky") == (n == "sticky_piston"))
        });
        c.require(
            "piston.base",
            result,
            "extended piston has no matching head",
        );
    }
    if n == "moving_piston" || (n == "piston_head" && prop(b, "short") == "true") {
        c.emit(
            "piston.transient",
            "moving piston state needs tick simulation",
            Severity::Warning,
        );
    }
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/TorchBlock.java
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/FaceAttachedHorizontalDirectionalBlock.java
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/BaseRailBlock.java
fn check_support(c: &mut Check<'_, '_>, b: &Block) {
    let n = name(b);
    if matches!(n, "torch" | "soul_torch" | "redstone_torch") {
        c.needs_support(DOWN, UP, Support::Center);
    } else if matches!(
        n,
        "wall_torch" | "soul_wall_torch" | "redstone_wall_torch" | "ladder" | "tripwire_hook"
    ) {
        c.needs_support(neg(facing(b)), facing(b), Support::Full);
    } else if n == "lever" || n.ends_with("_button") {
        let d = match prop(b, "face") {
            "floor" => DOWN,
            "ceiling" => UP,
            _ => neg(facing(b)),
        };
        c.needs_support(d, neg(d), Support::Full);
    } else if matches!(
        n,
        "rail" | "powered_rail" | "detector_rail" | "activator_rail" | "repeater" | "comparator"
    ) {
        c.needs_support(DOWN, UP, Support::Rigid);
        if let Some(d) = prop(b, "shape")
            .strip_prefix("ascending_")
            .and_then(|d| direction(d).ok())
        {
            c.needs_support(d.map(i64::from), UP, Support::Rigid);
        }
    } else if n == "redstone_wire" {
        if c.at(DOWN).is_some_and(|o| name(o) == "hopper") {
            return;
        }
        c.needs_support(DOWN, UP, Support::Full);
    } else if matches!(n, "lantern" | "soul_lantern") {
        let d = if prop(b, "hanging") == "true" {
            UP
        } else {
            DOWN
        };
        c.needs_support(d, neg(d), Support::Center);
    } else if n.ends_with("_pressure_plate") {
        c.needs_support(DOWN, UP, Support::Center);
    } else if n.ends_with("_sign") && !n.ends_with("_hanging_sign") {
        // Signs use isSolid, which is not the collision/full-face predicate.
        // Air is a definite failure; non-full shapes need additional game data.
        let d = if n.ends_with("_wall_sign") {
            neg(facing(b))
        } else {
            DOWN
        };
        let result = c.at(d).and_then(|o| {
            if o.is_air() || matches!(name(o), "water" | "lava") {
                Some(false)
            } else if c.scene.support(add(c.cell.point, d), UP, Support::Full) == Some(true) {
                Some(true)
            } else {
                None
            }
        });
        c.require("sign.support", result, "sign has no solid supporting block");
    }
}

fn soil(n: &str) -> bool {
    matches!(
        n,
        "grass_block"
            | "dirt"
            | "coarse_dirt"
            | "podzol"
            | "mycelium"
            | "rooted_dirt"
            | "moss_block"
            | "mud"
            | "muddy_mangrove_roots"
            | "pale_moss_block"
    )
}

fn crop(n: &str) -> bool {
    matches!(
        n,
        "wheat"
            | "carrots"
            | "potatoes"
            | "beetroots"
            | "melon_stem"
            | "pumpkin_stem"
            | "attached_melon_stem"
            | "attached_pumpkin_stem"
            | "torchflower_crop"
            | "pitcher_crop"
    )
}

fn soil_plant(n: &str) -> bool {
    matches!(
        n,
        "sunflower"
            | "lilac"
            | "rose_bush"
            | "peony"
            | "tall_grass"
            | "large_fern"
            | "pitcher_plant"
            | "short_grass"
            | "grass"
            | "fern"
            | "dandelion"
            | "poppy"
            | "blue_orchid"
            | "allium"
            | "azure_bluet"
            | "red_tulip"
            | "orange_tulip"
            | "white_tulip"
            | "pink_tulip"
            | "oxeye_daisy"
            | "cornflower"
            | "lily_of_the_valley"
            | "torchflower"
    ) || (n.ends_with("_sapling") && n != "bamboo_sapling")
}

fn falls(n: &str) -> bool {
    matches!(
        n,
        "sand"
            | "red_sand"
            | "gravel"
            | "anvil"
            | "chipped_anvil"
            | "damaged_anvil"
            | "dragon_egg"
            | "suspicious_sand"
            | "suspicious_gravel"
    ) || n.ends_with("_concrete_powder")
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/CropBlock.java
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/SugarCaneBlock.java
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/CactusBlock.java
fn check_plant(c: &mut Check<'_, '_>, b: &Block) {
    let n = name(b);
    if prop(b, "half") == "upper" {
        return;
    }
    if crop(n) {
        c.require(
            "plant.soil",
            c.at(DOWN).map(|o| name(o) == "farmland"),
            "crop requires farmland",
        );
    } else if n == "nether_wart" {
        c.require(
            "plant.soil",
            c.at(DOWN).map(|o| name(o) == "soul_sand"),
            "nether wart requires soul sand",
        );
    } else if n == "sugar_cane" {
        if c.at(DOWN).is_some_and(|o| name(o) == "sugar_cane") {
            return;
        }
        c.require(
            "plant.soil",
            c.at(DOWN).map(|o| {
                soil(name(o)) || matches!(name(o), "sand" | "red_sand" | "suspicious_sand")
            }),
            "invalid sugar cane ground",
        );
        let mut known = true;
        let mut water = false;
        for (_, d) in SIDES {
            match c.at(add(DOWN, d)) {
                Some(o) => {
                    water |= matches!(name(o), "water" | "frosted_ice")
                        || prop(o, "waterlogged") == "true"
                }
                None => known = false,
            }
        }
        c.require(
            "plant.water",
            if water {
                Some(true)
            } else {
                known.then_some(false)
            },
            "sugar cane requires water beside its ground block",
        );
    } else if n == "cactus" {
        c.require(
            "plant.soil",
            c.at(DOWN)
                .map(|o| matches!(name(o), "cactus" | "sand" | "red_sand" | "suspicious_sand")),
            "invalid cactus ground",
        );
        for (_, d) in SIDES {
            let result = c.at(d).and_then(|o| {
                if matches!(name(o), "lava" | "cactus") {
                    Some(false)
                } else if o.is_air() || matches!(name(o), "water" | "torch" | "redstone_wire") {
                    Some(true)
                } else if c.scene.support(add(c.cell.point, d), UP, Support::Full) == Some(true) {
                    Some(false)
                } else {
                    None
                }
            });
            c.require(
                "cactus.neighbor",
                result,
                "cactus touches a solid block or lava",
            );
        }
        c.require(
            "cactus.above",
            c.at(UP).map(|o| !matches!(name(o), "water" | "lava")),
            "cactus cannot have liquid directly above it",
        );
    } else if soil_plant(n) {
        c.require(
            "plant.soil",
            c.at(DOWN).map(|o| soil(name(o)) || name(o) == "farmland"),
            "plant requires suitable soil",
        );
    }
    if matches!(n, "attached_melon_stem" | "attached_pumpkin_stem") {
        let fruit = if n == "attached_melon_stem" {
            "melon"
        } else {
            "pumpkin"
        };
        c.require(
            "plant.fruit",
            c.at(facing(b)).map(|o| name(o) == fruit),
            "attached stem points to missing fruit",
        );
    }
}

// FallingBlock.isFree tests air, fire and fluid, not arbitrary non-solid blocks.
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/FallingBlock.java
fn check_gravity(c: &mut Check<'_, '_>, b: &Block) {
    let n = name(b);
    if !falls(n) {
        return;
    }
    match c.at(DOWN) {
        None => c.require("gravity.support", None, ""),
        Some(o) if o.is_air() || matches!(name(o), "fire" | "soul_fire" | "water" | "lava") => {
            c.emit(
                "gravity.unstable",
                "block can fall or change when updated",
                Severity::Warning,
            );
        }
        _ => (),
    }
}

fn connection_exception(n: &str) -> bool {
    n.ends_with("_leaves")
        || n.ends_with("shulker_box")
        || matches!(
            n,
            "barrier" | "carved_pumpkin" | "jack_o_lantern" | "melon" | "pumpkin"
        )
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/FenceBlock.java
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/IronBarsBlock.java
fn check_connections(c: &mut Check<'_, '_>, b: &Block) {
    let n = name(b);
    let fence = n.ends_with("_fence");
    let pane = n == "iron_bars" || n.ends_with("glass_pane");
    let wall = n.ends_with("_wall");
    if !(fence || pane || wall) {
        return;
    }
    for (side, d) in SIDES {
        let result = c.at(d).and_then(|o| {
            let other = name(o);
            let matching = if fence {
                other.ends_with("_fence")
                    && (n == "nether_brick_fence") == (other == "nether_brick_fence")
            } else if pane {
                other == "iron_bars" || other.ends_with("glass_pane")
            } else {
                other.ends_with("_wall")
            };
            let gate = !pane
                && other.ends_with("_fence_gate")
                && facing(o)[0] != d[0]
                && facing(o)[2] != d[2];
            let special = (pane && (other == "glass" || other.ends_with("_stained_glass")))
                || (wall && (other == "iron_bars" || other.ends_with("glass_pane")));
            let expected = if matching || gate || special {
                true
            } else if connection_exception(other) {
                false
            } else {
                c.scene
                    .support(add(c.cell.point, d), neg(d), Support::Full)?
            };
            let actual = !matches!(prop(b, side), "false" | "none");
            Some(expected == actual)
        });
        c.require(
            "connection.side",
            result,
            "side connection disagrees with its neighbor",
        );
    }
}

// WallBlock.updateShape uses the lower collision face of the block above.
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/WallBlock.java
fn check_wall_height(c: &mut Check<'_, '_>, b: &Block) {
    if !name(b).ends_with("_wall") {
        return;
    }
    if matches!(prop(b, "north"), "true" | "false") {
        c.require("wall.height", None, "");
        return; // Pre-1.16 wall geometry.
    }
    let result = (|| {
        let above = c.at(UP)?;
        let cover = c.scene.bottom_cover(add(c.cell.point, UP))?;
        let mut connected = [false; 4];
        let mut tall = [false; 4];
        for (i, (side, _)) in SIDES.iter().enumerate() {
            connected[i] = prop(b, side) != "none";
            tall[i] = connected[i] && cover[i];
            let expected = if !connected[i] {
                "none"
            } else if tall[i] {
                "tall"
            } else {
                "low"
            };
            if prop(b, side) != expected {
                return Some(false);
            }
        }
        let asymmetric = connected.iter().all(|v| !v)
            || connected[0] != connected[2]
            || connected[1] != connected[3];
        let straight_tall = (tall[0] && tall[2]) || (tall[1] && tall[3]);
        let forced = matches!(
            name(above),
            "torch" | "soul_torch" | "redstone_torch" | "tripwire"
        );
        let post = (name(above).ends_with("_wall") && prop(above, "up") == "true")
            || asymmetric
            || (!straight_tall && (forced || cover[4]));
        Some((prop(b, "up") == "true") == post)
    })();
    c.require(
        "wall.height",
        result,
        "wall arms or post disagree with the block above",
    );
}

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/StairBlock.java
fn check_stairs(c: &mut Check<'_, '_>, b: &Block) {
    if !name(b).ends_with("_stairs") {
        return;
    }
    let expected = (|| {
        let f = facing(b);
        for (d, outer) in [(f, true), (neg(f), false)] {
            let other = c.at(d)?;
            if name(other).ends_with("_stairs") && prop(other, "half") == prop(b, "half") {
                let of = facing(other);
                if of[0] * f[0] + of[2] * f[2] == 0 {
                    let side = c.at(if outer { neg(of) } else { of })?;
                    if !(name(side).ends_with("_stairs")
                        && facing(side) == f
                        && prop(side, "half") == prop(b, "half"))
                    {
                        let left = of == neg(clockwise(f));
                        return Some(match (outer, left) {
                            (true, true) => "outer_left",
                            (true, false) => "outer_right",
                            (false, true) => "inner_left",
                            (false, false) => "inner_right",
                        });
                    }
                }
            }
        }
        Some("straight")
    })();
    c.require(
        "stairs.shape",
        expected.map(|s| s == prop(b, "shape")),
        "stair corner shape disagrees with neighboring stairs",
    );
}

// Redstone dots and crosses can be selected by the player. Do not require every
// visible arm to have a recipient or infer powered state without running ticks.
// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/RedStoneWireBlock.java
fn check_redstone(c: &mut Check<'_, '_>, b: &Block) {
    if name(b) != "redstone_wire" {
        return;
    }
    for (side, d) in SIDES {
        if prop(b, side) == "up" {
            c.require(
                "redstone.up",
                c.at(add(d, UP)).map(|o| name(o) == "redstone_wire"),
                "upward connection has no wire above its neighbor",
            );
            c.needs_support(d, neg(d), Support::Full);
        } else if prop(b, side) == "none" && c.at(d).is_some_and(|o| name(o) == "redstone_wire") {
            c.emit(
                "redstone.connection",
                "adjacent redstone wire is disconnected",
                Severity::Error,
            );
        }
    }
}
