use super::{Block, Position};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

/// A sparse coordinate map sharing identical block values through a palette.
///
/// Direct writes do not validate catalogs or update region bounds and attached data.
#[derive(Clone, Debug, Default)]
pub struct BlockStorage {
    palette: Vec<Arc<Block>>,
    lookup: HashMap<Arc<Block>, u32>,
    uses: Vec<usize>,
    cells: BTreeMap<Position, u32>,
}

impl BlockStorage {
    /// Returns the number of stored cells.
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    /// Whether no cells are stored.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    /// Borrows a stored block, or returns None for an absent cell.
    pub fn get(&self, position: &Position) -> Option<&Block> {
        self.cells
            .get(position)
            .map(|&id| self.palette[id as usize].as_ref())
    }
    pub(crate) fn shared(&self, position: &Position) -> Option<Arc<Block>> {
        self.cells
            .get(position)
            .map(|&id| self.palette[id as usize].clone())
    }
    /// Whether a cell has a stored block entry.
    pub fn contains_key(&self, position: &Position) -> bool {
        self.cells.contains_key(position)
    }
    /// Iterates stored positions in lexicographic X, Y, Z order.
    pub fn keys(&self) -> impl Iterator<Item = &Position> {
        self.cells.keys()
    }
    /// Iterates stored positions and blocks in lexicographic coordinate order.
    pub fn iter(&self) -> impl Iterator<Item = (&Position, &Block)> {
        self.cells
            .iter()
            .map(|(p, &id)| (p, self.palette[id as usize].as_ref()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn indexed_iter(&self) -> impl Iterator<Item = (&Position, u32)> {
        self.cells.iter().map(|(position, &id)| (position, id))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn palette_len(&self) -> usize {
        self.palette.len()
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn palette_entry(&self, id: u32) -> &Block {
        &self.palette[id as usize]
    }
    /// Iterates block values once per stored cell, including repeated states.
    pub fn values(&self) -> impl Iterator<Item = &Block> {
        self.cells
            .values()
            .map(|&id| self.palette[id as usize].as_ref())
    }
    /// Iterates each distinct block value currently referenced by stored cells.
    pub fn states(&self) -> impl Iterator<Item = &Block> {
        self.palette
            .iter()
            .zip(&self.uses)
            .filter(|(_, count)| **count > 0)
            .map(|(block, _)| block.as_ref())
    }
    pub(crate) fn intern(&mut self, block: &Block) -> u32 {
        if let Some(&id) = self.lookup.get(block) {
            return id;
        }
        let id = u32::try_from(self.palette.len()).expect("Block palette exceeds u32 capacity");
        let block = Arc::new(block.clone());
        self.lookup.insert(block.clone(), id);
        self.palette.push(block);
        self.uses.push(0);
        id
    }
    pub(crate) fn set_id(&mut self, position: Position, id: u32) {
        self.replace_id(position, id);
    }
    pub(crate) fn replace_id(&mut self, position: Position, id: u32) -> bool {
        let old = self.cells.insert(position, id);
        if let Some(old) = old {
            self.uses[old as usize] -= 1;
        }
        self.uses[id as usize] += 1;
        old.is_none_or(|old| self.palette[old as usize].id != self.palette[id as usize].id)
    }
    /// Stores a block at a position, replacing any previous value without validation.
    pub fn set(&mut self, position: Position, block: &Block) {
        let id = self.intern(block);
        self.set_id(position, id);
    }
    /// Removes a stored cell; an absent position has no effect.
    pub fn remove(&mut self, position: &Position) {
        if let Some(id) = self.cells.remove(position) {
            self.uses[id as usize] -= 1;
        }
    }
    /// Removes all cells and palette values.
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn fill(
        &mut self,
        positions: impl Iterator<Item = Position>,
        block: &Block,
        replace: bool,
    ) {
        if replace {
            self.clear();
            let id = self.intern(block);
            self.cells = positions.map(|p| (p, id)).collect();
            self.uses[id as usize] = self.cells.len();
        } else {
            let id = self.intern(block);
            for position in positions {
                self.set_id(position, id);
            }
        }
    }
}
