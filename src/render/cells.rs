use super::SceneOptions;
use crate::{
    Result,
    model::{Block, Position, Region, Schematic},
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
        if regions.len() > 1 {
            entries.sort_unstable_by_key(|cell| cell.position);
            for pair in entries.windows(2) {
                if pair[0].position == pair[1].position {
                    return Err(format!(
                        "Selected regions overlap at {:?}",
                        pair[0].position
                    ));
                }
            }
        }
        Ok(Self { regions, entries })
    }

    pub(super) fn block(&self, cell: &Cell) -> &'a Block {
        self.regions[cell.region]
            .1
            .blocks
            .palette_entry(cell.palette)
    }

    pub(super) fn get(&self, position: &Position) -> Option<&'a Block> {
        self.entries
            .binary_search_by_key(position, |cell| cell.position)
            .ok()
            .map(|index| self.block(&self.entries[index]))
    }
}
