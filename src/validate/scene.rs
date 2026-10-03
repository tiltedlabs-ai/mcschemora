use super::rules::{self, Rule};
use super::shapes::{Shapes, Support, face_index, face_rectangles, support, wall_cover};
use super::{DOWN, Point, Report, add, global_position, name};
use crate::{
    catalog::Registry,
    model::{Block, Position, Region, Schematic},
};
use std::collections::HashMap;

pub(super) struct State {
    pub(super) block: Block,
    pub(super) valid: bool,
    pub(super) rules: Vec<Rule>,
    support: [[Option<bool>; 3]; 6],
    bottom_cover: Option<[bool; 5]>,
}

impl State {
    pub(super) fn new(
        block: Block,
        valid: bool,
        shapes: Option<&Shapes>,
        registry: &Registry,
    ) -> Self {
        let mut state = Self {
            rules: if valid {
                rules::for_block(&block)
            } else {
                Vec::new()
            },
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

#[derive(Clone, Copy)]
pub(super) struct Cell<'a> {
    pub(super) region: &'a str,
    pub(super) local: Position,
    pub(super) point: Point,
    pub(super) state: usize,
}

pub(super) struct Scene<'a> {
    regions: Vec<&'a Region>,
    last_region: std::cell::Cell<Option<usize>>,
    pub(super) states: Vec<State>,
    pub(super) cells: Vec<Cell<'a>>,
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
    fn new(schematic: &Schematic, count: usize) -> Self {
        if schematic.regions.len() == 1 {
            let region = schematic.regions.values().next().unwrap();
            if let Ok(volume) = region.bounds.volume()
                && volume > 0
                && volume <= count.saturating_mul(2)
            {
                return Self::Dense {
                    start: global_position(region.origin, region.bounds.start),
                    size: region.bounds.size.map(i64::from),
                    states: vec![usize::MAX; volume],
                };
            }
        }
        Self::Sparse(HashMap::with_capacity(count))
    }

    pub(super) fn get(&self, point: Point) -> Option<usize> {
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
    pub(super) fn replace(&mut self, cell: usize, state: usize) {
        self.cells[cell].state = state;
        self.index.insert(self.cells[cell].point, state);
    }

    pub(super) fn state_at(&self, point: Point) -> Option<&State> {
        self.index.get(point).map(|state| &self.states[state])
    }

    pub(super) fn get(&self, p: Point) -> Option<&Block> {
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
    pub(super) fn support(&self, p: Point, face: Point, kind: Support) -> Option<bool> {
        if let Some(state) = self.state_at(p) {
            if !state.valid {
                return None;
            }
            state.support[face_index(face)][kind as usize]
        } else {
            self.get(p).map(|_| false)
        }
    }
    pub(super) fn bottom_cover(&self, p: Point) -> Option<[bool; 5]> {
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

pub(super) fn check_region_bounds(schematic: &Schematic, report: &mut Report) {
    let mut regions: Vec<_> = schematic
        .regions
        .iter()
        .filter(|(_, region)| region.bounds.size.iter().all(|&size| size > 0))
        .map(|(name, region)| {
            let start = global_position(region.origin, region.bounds.start);
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
pub(super) struct StatePalette<'a> {
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

pub(super) fn build_scene<'a>(
    schematic: &'a Schematic,
    registry: &Registry,
    shapes: Option<&Shapes>,
    report: &mut Report,
) -> Scene<'a> {
    let count = schematic.regions.values().map(|r| r.blocks.len()).sum();
    let mut scene = Scene {
        regions: schematic.regions.values().collect(),
        last_region: std::cell::Cell::new(None),
        states: Vec::new(),
        cells: Vec::new(),
        index: BlockIndex::Sparse(HashMap::new()),
        air: Block::air(),
    };
    let mut palette = StatePalette::default();
    let mut state_ids = Vec::with_capacity(count);
    for (region_name, region) in &schematic.regions {
        for (&local, raw) in region.blocks.iter() {
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
            let point = global_position(region.origin, local);
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
    if !scene.cells.is_empty()
        || schematic
            .regions
            .values()
            .any(|r| !r.block_entities.is_empty())
    {
        scene.index = BlockIndex::new(schematic, count);
        // Preserve the first scan's IDs to avoid hashing every state twice.
        let mut state_ids = state_ids.into_iter();
        for region in schematic.regions.values() {
            for &local in region.blocks.keys() {
                let state = state_ids.next().unwrap();
                scene
                    .index
                    .insert(global_position(region.origin, local), state);
            }
        }
    }
    scene
}
