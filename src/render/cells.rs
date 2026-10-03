use super::SceneOptions;
use crate::{
    Result,
    model::{Block, Compound, Position, Region, Schematic},
};

pub(super) struct Cell {
    pub(super) position: Position,
    pub(super) region: usize,
    pub(super) palette: u32,
}

pub(super) struct Cells<'a> {
    pub(super) regions: Vec<(&'a String, &'a Region)>,
    pub(super) entries: Vec<Cell>,
}

impl<'a> Cells<'a> {
    pub(super) fn new(schematic: &'a Schematic, options: &SceneOptions) -> Result<Self> {
        let regions: Vec<_> = schematic
            .regions
            .iter()
            .filter(|(name, _)| {
                options
                    .region
                    .as_ref()
                    .is_none_or(|selected| selected == *name)
            })
            .collect();
        let mut entries = Vec::new();
        for (index, (_, region)) in regions.iter().enumerate() {
            for (local, palette) in region.blocks.indexed_iter() {
                if region.blocks.palette_entry(palette).is_air() {
                    continue;
                }
                let mut position = [0; 3];
                for axis in 0..3 {
                    position[axis] = region.origin[axis]
                        .checked_add(local[axis])
                        .ok_or("Geometry coordinate overflow")?;
                }
                if options.contains(position.map(f64::from)) {
                    entries.push(Cell {
                        position,
                        region: index,
                        palette,
                    });
                }
            }
        }
        let mut cells = Self { regions, entries };
        if cells.regions.len() > 1 {
            cells
                .entries
                .sort_unstable_by_key(|cell| (cell.position, cell.region));
            for pair in cells.entries.windows(2) {
                if pair[0].position == pair[1].position {
                    let first = cells.block(&pair[0]);
                    let second = cells.block(&pair[1]);
                    let conflict = if first != second {
                        format!("{} versus {}", first.text(), second.text())
                    } else if cells.block_entity(&pair[0]) != cells.block_entity(&pair[1]) {
                        "different attached block data".into()
                    } else {
                        continue;
                    };
                    return Err(format!(
                        "Conflicting blocks at {:?} in regions {:?} and {:?}: {conflict}",
                        pair[0].position,
                        cells.regions[pair[0].region].0,
                        cells.regions[pair[1].region].0,
                    ));
                }
            }
            cells.entries.dedup_by_key(|cell| cell.position);
        }
        Ok(cells)
    }

    pub(super) fn block(&self, cell: &Cell) -> &'a Block {
        self.regions[cell.region]
            .1
            .blocks
            .palette_entry(cell.palette)
    }

    pub(super) fn block_entity(&self, cell: &Cell) -> Option<&'a Compound> {
        let region = self.regions[cell.region].1;
        let local = std::array::from_fn(|i| cell.position[i] - region.origin[i]);
        region.block_entities.get(&local)
    }

    pub(super) fn get(&self, position: &Position) -> Option<&'a Block> {
        self.entries
            .binary_search_by_key(position, |cell| cell.position)
            .ok()
            .map(|index| self.block(&self.entries[index]))
    }
}
