use super::{Block, Pos};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[derive(Clone, Debug, Default)]
pub struct BlockStorage {
    palette: Vec<Arc<Block>>,
    lookup: HashMap<Arc<Block>, u32>,
    uses: Vec<usize>,
    cells: BTreeMap<Pos, u32>,
}

impl BlockStorage {
    pub fn len(&self) -> usize {
        self.cells.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
    pub fn get(&self, position: &Pos) -> Option<&Block> {
        self.cells
            .get(position)
            .map(|&id| self.palette[id as usize].as_ref())
    }
    pub(crate) fn shared(&self, position: &Pos) -> Option<Arc<Block>> {
        self.cells
            .get(position)
            .map(|&id| self.palette[id as usize].clone())
    }
    pub fn contains_key(&self, position: &Pos) -> bool {
        self.cells.contains_key(position)
    }
    pub fn keys(&self) -> impl Iterator<Item = &Pos> {
        self.cells.keys()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&Pos, &Block)> {
        self.cells
            .iter()
            .map(|(p, &id)| (p, self.palette[id as usize].as_ref()))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn indexed_iter(&self) -> impl Iterator<Item = (&Pos, u32)> {
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
    pub fn values(&self) -> impl Iterator<Item = &Block> {
        self.cells
            .values()
            .map(|&id| self.palette[id as usize].as_ref())
    }
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
    pub(crate) fn set_id(&mut self, position: Pos, id: u32) {
        if let Some(old) = self.cells.insert(position, id) {
            self.uses[old as usize] -= 1;
        }
        self.uses[id as usize] += 1;
    }
    pub fn set(&mut self, position: Pos, block: &Block) {
        let id = self.intern(block);
        self.set_id(position, id);
    }
    pub fn remove(&mut self, position: &Pos) {
        if let Some(id) = self.cells.remove(position) {
            self.uses[id as usize] -= 1;
        }
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn fill(
        &mut self,
        positions: impl Iterator<Item = Pos>,
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
