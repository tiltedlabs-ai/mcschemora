//! semantic checks on schematics to make sure they're valid
//! 
use crate::{
    model::{Block, Compound, Document, Pos, Region, direction},
    registry::Registry,
};
use fastnbt::Value;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub unknown: Vec<String>,
}

type Point = [i64; 3];
const UP: Point = [0, 1, 0];
const DOWN: Point = [0, -1, 0];
const SIDES: [(&str, Point); 4] = [
    ("north", [0, 0, -1]),
    ("east", [1, 0, 0]),
    ("south", [0, 0, 1]),
    ("west", [-1, 0, 0]),
];

fn add(p: Point, d: Point) -> Point {
    std::array::from_fn(|i| p[i] + d[i])
}
fn neg(d: Point) -> Point {
    d.map(|n| -n)
}
fn world(origin: Pos, p: Pos) -> Point {
    std::array::from_fn(|i| i64::from(origin[i]) + i64::from(p[i]))
}
fn name(b: &Block) -> &str {
    b.name.strip_prefix("minecraft:").unwrap_or(&b.name)
}
fn prop<'a>(b: &'a Block, key: &str) -> &'a str {
    b.properties.get(key).map(String::as_str).unwrap_or("")
}
fn facing(b: &Block) -> Point {
    direction(prop(b, "facing"))
        .unwrap_or([0, 0, -1])
        .map(i64::from)
}
fn clockwise(d: Point) -> Point {
    [-d[2], 0, d[0]]
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ShapeIds {
    One(u32),
    States(Vec<u32>),
}

/// https://github.com/PrismarineJS/minecraft-data/blob/master/doc/blockCollisionShapes.md
#[derive(Debug, Deserialize)]
pub(crate) struct Shapes {
    blocks: HashMap<String, ShapeIds>,
    shapes: HashMap<u32, Vec<[f64; 6]>>,
}

#[derive(Clone, Copy)]
enum Support {
    Full,
    Center,
    Rigid,
}

impl Shapes {
    fn boxes(&self, b: &Block, registry: &Registry) -> Option<&[[f64; 6]]> {
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
fn face_rectangles(boxes: &[[f64; 6]], face: Point) -> Vec<[f64; 4]> {
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

type Rule = fn(&mut Check<'_, '_>, &Block);

/// Choose checks once per distinct state, instead of testing every rule at
/// every coordinate. Individual checks retain their guards for clarity.
fn rules_for(block: &Block) -> Vec<Rule> {
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

struct State {
    block: Block,
    valid: bool,
    rules: Vec<Rule>,
    support: [[Option<bool>; 3]; 6],
    bottom_cover: Option<[bool; 5]>,
}

impl State {
    fn new(block: Block, valid: bool, shapes: Option<&Shapes>, registry: &Registry) -> Self {
        let mut state = Self {
            rules: if valid { rules_for(&block) } else { Vec::new() },
            block,
            valid,
            support: [[None; 3]; 6],
            bottom_cover: None,
        };
        if !valid {
            return state;
        }

        let boxes = shapes.and_then(|shapes| shapes.boxes(&state.block, registry));
        for axis in 0..3 {
            for sign in 0..2 {
                let mut face = [0; 3];
                face[axis] = if sign == 0 { -1 } else { 1 };
                let rects = boxes.map(|boxes| face_rectangles(boxes, face));
                state.support[axis * 2 + sign] = [Support::Full, Support::Center, Support::Rigid]
                    .map(|kind| support(&state.block, rects.as_deref(), face, kind));
                if face == DOWN {
                    state.bottom_cover = rects.as_deref().map(wall_cover);
                }
            }
        }
        state
    }
}

/// North, east, south, west arm coverage, followed by the center post.
fn wall_cover(rects: &[[f64; 4]]) -> [bool; 5] {
    [
        [7. / 16., 0., 9. / 16., 9. / 16.],
        [7. / 16., 7. / 16., 1., 9. / 16.],
        [7. / 16., 7. / 16., 9. / 16., 1.],
        [0., 7. / 16., 9. / 16., 9. / 16.],
        [7. / 16., 7. / 16., 9. / 16., 9. / 16.],
    ]
    .map(|target| covered(rects, target))
}

fn face_index(face: Point) -> usize {
    let axis = face.iter().position(|&n| n != 0).unwrap();
    axis * 2 + usize::from(face[axis] > 0)
}

fn support(b: &Block, rects: Option<&[[f64; 4]]>, face: Point, kind: Support) -> Option<bool> {
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

#[derive(Clone, Copy)]
struct Cell<'a> {
    region: &'a str,
    local: Pos,
    point: Point,
    state: usize,
}

struct Scene<'a> {
    regions: Vec<&'a Region>,
    last_region: std::cell::Cell<Option<usize>>,
    states: Vec<State>,
    cells: Vec<Cell<'a>>,
    index: BlockIndex,
    air: Block,
}

/// Dense regions use a compact array. Widely separated or mostly empty regions
/// keep the sparse hash index, so their empty bounding volume costs no memory.
enum BlockIndex {
    Sparse(HashMap<Point, usize>),
    Dense {
        start: Point,
        size: Point,
        states: Vec<usize>,
    },
}

impl BlockIndex {
    fn new(doc: &Document, count: usize) -> Self {
        if doc.regions.len() == 1 {
            let region = doc.regions.values().next().unwrap();
            if let Ok(volume) = region.bounds.volume()
                && volume > 0
                && volume <= count.saturating_mul(2)
            {
                return Self::Dense {
                    start: world(region.origin, region.bounds.start),
                    size: region.bounds.size.map(i64::from),
                    states: vec![usize::MAX; volume],
                };
            }
        }
        Self::Sparse(HashMap::with_capacity(count))
    }

    fn get(&self, point: Point) -> Option<usize> {
        match self {
            Self::Sparse(states) => states.get(&point).copied(),
            Self::Dense {
                start,
                size,
                states,
            } => {
                let state = states[Self::offset(point, *start, *size)?];
                (state != usize::MAX).then_some(state)
            }
        }
    }

    fn insert(&mut self, point: Point, state: usize) {
        match self {
            Self::Sparse(states) => {
                states.insert(point, state);
            }
            Self::Dense {
                start,
                size,
                states,
            } => {
                if let Some(offset) = Self::offset(point, *start, *size) {
                    states[offset] = state;
                }
            }
        }
    }

    fn offset(point: Point, start: Point, size: Point) -> Option<usize> {
        let local: Point = std::array::from_fn(|i| point[i] - start[i]);
        if (0..3).any(|i| local[i] < 0 || local[i] >= size[i]) {
            return None;
        }
        Some(((local[0] * size[1] + local[1]) * size[2] + local[2]) as usize)
    }
}

impl Scene<'_> {
    fn state_at(&self, point: Point) -> Option<&State> {
        self.index.get(point).map(|state| &self.states[state])
    }

    fn get(&self, p: Point) -> Option<&Block> {
        if let Some(state) = self.state_at(p) {
            return state.valid.then_some(&state.block);
        }
        // Neighbor reads usually stay in the same region. 
        if self
            .last_region
            .get()
            .is_some_and(|i| contains_known(self.regions[i], p))
        {
            return Some(&self.air);
        }
        let region = self
            .regions
            .iter()
            .position(|region| contains_known(region, p))?;
        self.last_region.set(Some(region));
        Some(&self.air)
    }
    fn support(&self, p: Point, face: Point, kind: Support) -> Option<bool> {
        if let Some(state) = self.state_at(p) {
            if !state.valid {
                return None;
            }
            state.support[face_index(face)][kind as usize]
        } else {
            self.get(p).map(|_| false)
        }
    }
    fn bottom_cover(&self, p: Point) -> Option<[bool; 5]> {
        match self.state_at(p) {
            Some(state) => state.bottom_cover,
            None => self.get(p).map(|_| [false; 5]),
        }
    }
}

fn contains_known(region: &Region, point: Point) -> bool {
    let local = std::array::from_fn::<_, 3, _>(|i| {
        i32::try_from(point[i] - i64::from(region.origin[i])).ok()
    });
    let [Some(x), Some(y), Some(z)] = local else {
        return false;
    };
    let local = [x, y, z];
    region.bounds.contains(local)
        && region
            .present
            .as_ref()
            .is_none_or(|cells| cells.contains(&local))
}

#[derive(Clone, Copy)]
enum Severity {
    Error,
    Warning,
    Unknown,
}

struct Check<'a, 'b> {
    scene: &'a Scene<'b>,
    cell: Cell<'b>,
    report: &'a mut Report,
    missing: Vec<&'static str>,
}

impl Check<'_, '_> {
    fn emit(&mut self, rule: &str, message: &str, severity: Severity) {
        let text = format!(
            "{} {:?}: {rule}: {message}",
            self.cell.region, self.cell.local
        );
        match severity {
            Severity::Error => self.report.errors.push(text),
            Severity::Warning => self.report.warnings.push(text),
            Severity::Unknown => self.report.unknown.push(text),
        }
    }
    fn require(&mut self, rule: &'static str, result: Option<bool>, message: &str) {
        match result {
            Some(true) => (),
            Some(false) => self.emit(rule, message, Severity::Error),
            None if !self.missing.contains(&rule) => {
                self.missing.push(rule);
                self.emit(
                    rule,
                    "needs surrounding blocks or support data",
                    Severity::Unknown,
                )
            }
            None => (),
        }
    }
    fn at(&self, d: Point) -> Option<&Block> {
        self.scene.get(add(self.cell.point, d))
    }
    fn needs_support(&mut self, offset: Point, face: Point, kind: Support) {
        let result = self.scene.support(add(self.cell.point, offset), face, kind);
        self.require("support", result, "required supporting face is missing");
    }
}

pub fn validate(doc: &Document) -> Report {
    let mut report = Report {
        warnings: doc.notices.clone(),
        ..Report::default()
    };
    if doc.edition != "java" {
        report
            .unknown
            .push("edition: game-rule validation currently requires Java Edition".into());
        return report;
    }
    let registry = match doc.registry() {
        Ok(registry) => registry,
        Err(e) => {
            report.unknown.push(e);
            return report;
        }
    };
    check_region_bounds(doc, &mut report);
    if !report.errors.is_empty() {
        return report;
    }
    let shapes = registry.validation_shapes.get_or_init(|| {
        serde_json::from_value(doc.data.collision_shapes(&doc.version)?)
            .map_err(|e| format!("Invalid collision shapes: {e}"))
    });
    if let Err(e) = shapes {
        report.unknown.push(format!("support.catalog: {e}"));
    }
    let scene = build_scene(doc, registry, shapes.as_ref().ok(), &mut report);
    let mut portals = HashSet::new();
    for &cell in &scene.cells {
        let state = &scene.states[cell.state];
        if !state.valid {
            continue;
        }
        let b = &state.block;
        let mut check = Check {
            scene: &scene,
            cell,
            report: &mut report,
            missing: Vec::new(),
        };
        for rule in &state.rules {
            rule(&mut check, b);
        }
        if name(b) == "moving_piston"
            && !doc.regions[cell.region]
                .block_entities
                .contains_key(&cell.local)
        {
            check.emit(
                "piston.data",
                "moving piston requires its block entity data",
                Severity::Error,
            );
        }
        if matches!(name(b), "nether_portal" | "end_portal") && !portals.contains(&cell.point) {
            check_portal(&mut check, b, &mut portals);
        }
    }
    for (region_name, region) in &doc.regions {
        for (&local, data) in &region.block_entities {
            check_block_entity(
                &scene,
                world(region.origin, local),
                data,
                region_name,
                local,
                doc,
                &mut report,
            );
        }
    }
    report
}

fn check_region_bounds(doc: &Document, report: &mut Report) {
    let mut regions: Vec<_> = doc
        .regions
        .iter()
        .filter(|(_, region)| region.bounds.size.iter().all(|&size| size > 0))
        .map(|(name, region)| {
            let start = world(region.origin, region.bounds.start);
            let end = add(start, region.bounds.size.map(i64::from));
            (name, start, end)
        })
        .collect();
    regions.sort_by_key(|(_, start, _)| start[0]);
    for (i, (name, start, end)) in regions.iter().enumerate() {
        for (other, next_start, next_end) in &regions[i + 1..] {
            // Sorted X ranges let us stop before testing unrelated regions.
            if next_start[0] >= end[0] {
                break;
            }
            let overlaps =
                (1..3).all(|axis| start[axis] < next_end[axis] && next_start[axis] < end[axis]);
            if overlaps {
                report.errors.push(format!(
                    "region.overlap: {name} and {other} have overlapping bounds"
                ));
            }
        }
    }
}

/// Neighboring cells often repeat a state. Compare with the last state before
/// hashing its name and properties again. The cache lasts for one validation.
#[derive(Default)]
struct StatePalette<'a> {
    ids: HashMap<&'a Block, usize>,
    previous: Option<(&'a Block, usize)>,
}

impl<'a> StatePalette<'a> {
    fn get_or_insert(&mut self, block: &'a Block, create: impl FnOnce() -> usize) -> usize {
        if let Some((previous, id)) = self.previous
            && previous == block
        {
            return id;
        }
        let id = *self.ids.entry(block).or_insert_with(create);
        self.previous = Some((block, id));
        id
    }
}

fn build_scene<'a>(
    doc: &'a Document,
    registry: &Registry,
    shapes: Option<&Shapes>,
    report: &mut Report,
) -> Scene<'a> {
    let count = doc.regions.values().map(|r| r.blocks.len()).sum();
    let mut scene = Scene {
        regions: doc.regions.values().collect(),
        last_region: std::cell::Cell::new(None),
        states: Vec::new(),
        cells: Vec::new(),
        index: BlockIndex::Sparse(HashMap::new()),
        air: Block::air(),
    };
    let mut palette = StatePalette::default();
    let mut state_ids = Vec::with_capacity(count);
    for (region_name, region) in &doc.regions {
        for (&local, raw) in &region.blocks {
            let state = palette.get_or_insert(raw, || {
                let resolved = registry.resolve(raw);
                let valid = resolved.is_ok();
                let block = resolved.unwrap_or_else(|e| {
                    report
                        .errors
                        .push(format!("{region_name} {local:?}: block.state: {e}"));
                    raw.clone()
                });
                let index = scene.states.len();
                scene
                    .states
                    .push(State::new(block, valid, shapes, registry));
                index
            });
            state_ids.push(state);
            if !region.bounds.contains(local) {
                report.errors.push(format!(
                    "{region_name} {local:?}: region.bounds: stored block is outside its region"
                ));
                continue;
            }
            let state_info = &scene.states[state];
            if state_info.rules.is_empty()
                && !matches!(name(&state_info.block), "nether_portal" | "end_portal")
            {
                continue;
            }
            let point = world(region.origin, local);
            scene.cells.push(Cell {
                region: region_name,
                local,
                point,
                state,
            });
        }
    }
    // Only spatial rules and block entities need neighbor lookups. A scene of
    // plain building blocks still has every distinct state checked above.
    if !scene.cells.is_empty() || doc.regions.values().any(|r| !r.block_entities.is_empty()) {
        scene.index = BlockIndex::new(doc, count);
        // Preserve the first scan's IDs to avoid hashing every state twice.
        let mut state_ids = state_ids.into_iter();
        for region in doc.regions.values() {
            for &local in region.blocks.keys() {
                let state = state_ids.next().unwrap();
                scene.index.insert(world(region.origin, local), state);
            }
        }
    }
    scene
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

// https://github.com/mahtomedi/minecraft/blob/main/src/main/java/net/minecraft/world/level/block/entity/BlockEntityType.java
pub(crate) fn block_entity_id(b: &Block) -> Option<&str> {
    let n = name(b);
    Some(match n {
        "moving_piston" => "piston",
        "soul_campfire" => "campfire",
        "chain_command_block" | "repeating_command_block" => "command_block",
        n if n.ends_with("_hanging_sign") => "hanging_sign",
        n if n.ends_with("_sign") || n == "sign" => "sign",
        n if n.ends_with("_bed") => "bed",
        n if n.ends_with("_banner") => "banner",
        n if n.ends_with("shulker_box") => "shulker_box",
        "skeleton_skull"
        | "skeleton_wall_skull"
        | "wither_skeleton_skull"
        | "wither_skeleton_wall_skull"
        | "player_head"
        | "player_wall_head"
        | "zombie_head"
        | "zombie_wall_head"
        | "creeper_head"
        | "creeper_wall_head"
        | "dragon_head"
        | "dragon_wall_head"
        | "piglin_head"
        | "piglin_wall_head" => "skull",
        "chest"
        | "trapped_chest"
        | "furnace"
        | "blast_furnace"
        | "smoker"
        | "hopper"
        | "dispenser"
        | "dropper"
        | "barrel"
        | "beacon"
        | "spawner"
        | "lectern"
        | "brewing_stand"
        | "crafter"
        | "campfire"
        | "command_block"
        | "comparator"
        | "daylight_detector"
        | "enchanting_table"
        | "end_portal"
        | "end_gateway"
        | "ender_chest"
        | "jigsaw"
        | "jukebox"
        | "structure_block"
        | "conduit"
        | "bell"
        | "beehive"
        | "sculk_sensor"
        | "calibrated_sculk_sensor"
        | "sculk_catalyst"
        | "sculk_shrieker"
        | "chiseled_bookshelf"
        | "decorated_pot"
        | "trial_spawner"
        | "vault" => n,
        "bee_nest" => "beehive",
        "suspicious_sand" | "suspicious_gravel" => "brushable_block",
        _ => return None,
    })
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

// Portal components are traversed once. A malformed component never causes an
// unbounded rectangular volume scan: Nether openings are at most 21 by 21.
fn check_portal(c: &mut Check<'_, '_>, b: &Block, visited: &mut HashSet<Point>) {
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
                    && state.block.name == b.name
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

// https://minecraft.wiki/w/Block_entity
// Inventory slot IDs are local to each block, including each double-chest half.
fn check_block_entity(
    scene: &Scene<'_>,
    p: Point,
    data: &Compound,
    region: &str,
    local: Pos,
    doc: &Document,
    report: &mut Report,
) {
    let Some(block) = scene.get(p) else {
        report.unknown.push(format!(
            "{region} {local:?}: block_entity: owning block is unknown"
        ));
        return;
    };
    let mut error = |message: &str| {
        report
            .errors
            .push(format!("{region} {local:?}: block_entity: {message}"))
    };
    match data.get("id") {
        Some(Value::String(id))
            if block_entity_id(block) == Some(id.strip_prefix("minecraft:").unwrap_or(id)) => {}
        _ => error("missing ID or ID does not match the block"),
    }
    let capacity = match name(block) {
        "chest" | "trapped_chest" | "barrel" => Some(27),
        "hopper" | "brewing_stand" => Some(5),
        "furnace" | "blast_furnace" | "smoker" => Some(3),
        "dispenser" | "dropper" | "crafter" => Some(9),
        "campfire" | "soul_campfire" => Some(4),
        "chiseled_bookshelf" => Some(6),
        n if n.ends_with("shulker_box") => Some(27),
        _ => None,
    };
    if let Some(items) = data.get("Items") {
        let Value::List(items) = items else {
            error("Items must be a list");
            return;
        };
        let mut slots = HashSet::new();
        for item in items {
            let Value::Compound(item) = item else {
                error("inventory entries must be compounds");
                continue;
            };
            match item.get("Slot") {
                Some(Value::Byte(slot))
                    if *slot >= 0 && capacity.is_none_or(|n| i32::from(*slot) < n) =>
                {
                    if !slots.insert(*slot) {
                        error("duplicate inventory slot");
                    }
                }
                _ => error("invalid inventory slot"),
            }
            match item.get("id") {
                Some(Value::String(id)) if doc.registry().is_ok_and(|r| r.item(id).is_ok()) => (),
                _ => error("unknown or missing inventory item ID"),
            }
            let valid_count = if doc.data_version >= 3837 {
                item.get("count").is_none()
                    || matches!(item.get("count"),Some(Value::Int(n)) if *n > 0)
            } else {
                matches!(item.get("Count"),Some(Value::Byte(n)) if *n > 0)
            };
            if !valid_count {
                error("item count has the wrong type, name, or value for this version");
            }
        }
    }
}
