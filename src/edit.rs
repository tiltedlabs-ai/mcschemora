//! Catalog-checked document edits and operations on attached entity data.

use crate::{Result, helpers, model::*, nbt, transform::Transform};
use std::{borrow::Borrow, collections::BTreeMap};

pub(crate) fn check_position(p: [f64; 3]) -> Result<()> {
    if p.iter()
        .any(|v| !v.is_finite() || *v < i32::MIN as f64 || *v >= i32::MAX as f64)
    {
        return Err("Entity position must be finite and inside coordinate limits".into());
    }
    Ok(())
}

impl Schematic {
    /// Adds an empty region at a world origin; its name must be nonempty and unique.
    pub fn add_region(&mut self, name: &str, origin: Position) -> Result<()> {
        if name.is_empty() || self.regions.contains_key(name) {
            return Err("Region name must be nonempty and unique".into());
        }
        self.regions.insert(name.into(), Region::new(origin));
        Ok(())
    }
    /// Resolves block states and commits local cell writes atomically.
    ///
    /// Later writes to the same position take precedence. Bounds expand when permitted.
    pub fn set_blocks<B: Borrow<Block>>(
        &mut self,
        name: &str,
        blocks: impl IntoIterator<Item = (Position, B)>,
    ) -> Result<()> {
        let catalog = self.registry()?;
        let mut resolved: BTreeMap<Block, std::sync::Arc<Block>> = BTreeMap::new();
        let edits = blocks
            .into_iter()
            .map(|(p, b)| {
                let b = b.borrow();
                let block = if let Some(block) = resolved.get(b) {
                    block.clone()
                } else {
                    let block = std::sync::Arc::new(
                        catalog
                            .resolve(b)
                            .map_err(|e| format!("{name} {p:?}: {e}"))?,
                    );
                    resolved.insert(b.clone(), block.clone());
                    block
                };
                Ok((p, block, None))
            })
            .collect::<Result<_>>()?;
        self.region_mut(name)?.write(edits, None)
    }
    /// Validates a palette and commits indexed local cell writes atomically.
    pub fn set_indexed_blocks(
        &mut self,
        name: &str,
        palette: &[Block],
        cells: Vec<(Position, usize)>,
    ) -> Result<()> {
        if cells.iter().any(|(_, index)| *index >= palette.len()) {
            return Err("Block palette index out of range".into());
        }
        let catalog = self.registry()?;
        let palette = palette
            .iter()
            .enumerate()
            .map(|(index, block)| {
                catalog.resolve(block).map_err(|error| {
                    match cells.iter().find(|(_, id)| *id == index) {
                        Some((position, _)) => format!("{name} {position:?}: {error}"),
                        None => format!("{name}: {error}"),
                    }
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.region_mut(name)?.write_indexed(&palette, cells)
    }
    /// Fills selected cells with a resolved block without changing free entities.
    pub fn fill(&mut self, name: &str, selection: &Selection, block: &Block) -> Result<()> {
        let block = self.registry()?.resolve(block)?;
        self.region_mut(name)?.fill(selection, &block)
    }
    /// Validates and replaces supplied properties at local cell positions atomically.
    pub fn patch(
        &mut self,
        name: &str,
        positions: impl IntoIterator<Item = Position>,
        properties: &BTreeMap<String, String>,
    ) -> Result<()> {
        let r = self.region(name)?;
        let blocks: Vec<_> = positions
            .into_iter()
            .map(|p| {
                let mut b = r.get(p);
                b.properties.extend(properties.clone());
                (p, b)
            })
            .collect();
        self.set_blocks(name, blocks)
    }
    /// Clears selected blocks, attached data, and selected free entities.
    pub fn delete(&mut self, name: &str, selection: &Selection) -> Result<()> {
        if self.edition == "bedrock" {
            return Err("Bedrock layered editing is not implemented".into());
        }
        self.region_mut(name)?.clear(selection);
        Ok(())
    }
    /// Validates and commits a placement anchored at a local cell position.
    ///
    /// Occupied targets require replace=true. Beds anchor at the foot and doors at the lower half.
    pub fn place(
        &mut self,
        name: &str,
        placement: &helpers::Placement,
        at: Position,
        replace: bool,
    ) -> Result<()> {
        let edits = helpers::resolve_placement(self.registry()?, placement, at)?;
        let r = self.region(name)?;
        if !replace {
            for (p, _, _) in &edits {
                if !r.get(*p).is_air() {
                    return Err(format!("{name} {p:?}: occupied; use replace=True"));
                }
            }
        }
        self.region_mut(name)?.write(edits, None)
    }
    /// Pastes a fragment at a local anchor, replacing cells and preserving unrelated entities.
    ///
    /// Edition and version must match. Copied entities receive fresh references.
    pub fn paste(&mut self, name: &str, at: Position, fragment: &Fragment) -> Result<()> {
        if self.edition != fragment.edition || self.version != fragment.version {
            return Err("Fragments require matching editions and versions; use explicit file conversion for mappings".into());
        }
        let t = Transform::move_by(at);
        let edits = fragment
            .edits
            .iter()
            .map(|(p, b, n)| Ok((t.cell(*p)?, b.clone(), n.clone())))
            .collect::<Result<Vec<_>>>()?;
        let r = self.region(name)?;
        let bounds = r.expanded(edits.iter().map(|(p, _, _)| *p))?;
        r.check_bounds(bounds)?;
        let mut next = self.next_entity;
        let entities = t.entities(fragment.entities.iter(), &mut next, true)?;
        let r = self.region_mut(name)?;
        for (p, _, _) in &edits {
            r.block_entities.remove(p);
        }
        r.write(edits, Some(bounds))?;
        r.entities.extend(entities);
        self.next_entity = next;
        Ok(())
    }
    /// Adds entity NBT at a local floating-point position and returns a fresh reference.
    ///
    /// The compound must include id. Position and any required bounds expansion are checked.
    pub fn add_entity(&mut self, name: &str, at: [f64; 3], data: Compound) -> Result<u64> {
        check_position(at)?;
        nbt::string(nbt::get(&data, "id")?)?;
        let reference = self.next_entity;
        let next = reference
            .checked_add(1)
            .ok_or("Entity reference overflow")?;
        let r = self.region_mut(name)?;
        let bounds = r.expanded(std::iter::once(at.map(|v| v.floor() as i32)))?;
        r.check_bounds(bounds)?;
        r.entities.push(Entity {
            reference,
            position_float: false,
            position: at,
            data,
        });
        r.bounds = bounds;
        self.next_entity = next;
        Ok(reference)
    }
}

impl Region {
    fn fill(&mut self, selection: &Selection, block: &Block) -> Result<()> {
        let area = match &selection.cells {
            Some(cells) => Bounds::around(cells.iter().copied())?,
            None => Bounds::new(selection.bounds.start, selection.bounds.size)?,
        };
        if area.size.contains(&0) {
            return Ok(());
        }
        let last = std::array::from_fn(|i| area.start[i] + area.size[i] - 1);
        let bounds = self.expanded([area.start, last].into_iter())?;
        self.check_bounds(bounds)?;

        let air = block == &Block::air();
        self.block_entities.retain(|p, _| {
            !selection.contains(*p)
                || (!air && self.blocks.get(p).is_some_and(|old| old.name == block.name))
        });
        if let Some(present) = &mut self.present {
            present.extend(fill_positions(selection));
        }

        let replaces_all = selection.cells.is_none()
            && (self.blocks.is_empty()
                || (area.contains(self.bounds.start)
                    && area.contains(std::array::from_fn(|i| {
                        self.bounds.start[i] + self.bounds.size[i] - 1
                    }))));
        if replaces_all {
            if air {
                self.blocks.clear();
            } else {
                self.blocks.fill(fill_positions(selection), block, true);
            }
        } else if air {
            for p in fill_positions(selection) {
                self.blocks.remove(&p);
            }
        } else {
            self.blocks.fill(fill_positions(selection), block, false);
        }
        self.bounds = bounds;
        Ok(())
    }

    pub(crate) fn clear(&mut self, selection: &Selection) {
        for p in selection.positions() {
            if let Some(present) = &mut self.present {
                present.insert(p);
            }
            self.blocks.remove(&p);
            self.block_entities.remove(&p);
        }
        self.entities
            .retain(|e| !selection.entities.contains(&e.reference));
    }

    fn entity_index(&self, reference: u64) -> Result<usize> {
        self.entities
            .iter()
            .position(|e| e.reference == reference)
            .ok_or_else(|| "Unknown entity reference".into())
    }
    /// Borrows an entity by its document-local reference within this region.
    pub fn entity(&self, reference: u64) -> Result<&Entity> {
        Ok(&self.entities[self.entity_index(reference)?])
    }
    /// Replaces supplied position and full NBT; omitted fields are preserved.
    ///
    /// Positions are local and finite. Replacement NBT must include id.
    pub fn update_entity(
        &mut self,
        reference: u64,
        position: Option<[f64; 3]>,
        data: Option<Compound>,
    ) -> Result<()> {
        if let Some(p) = position {
            check_position(p)?;
        }
        if let Some(data) = &data {
            nbt::string(nbt::get(data, "id")?)?;
        }
        let index = self.entity_index(reference)?;
        let bounds = position
            .map(|p| self.expanded(std::iter::once(p.map(|v| v.floor() as i32))))
            .transpose()?;
        if let Some(bounds) = bounds {
            self.check_bounds(bounds)?;
        }
        let e = &mut self.entities[index];
        if let Some(p) = position {
            e.position = p;
        }
        if let Some(data) = data {
            e.data = data;
        }
        if let Some(bounds) = bounds {
            self.bounds = bounds;
        }
        Ok(())
    }
    /// Removes an entity, or returns an error if its reference is unknown in this region.
    pub fn remove_entity(&mut self, reference: u64) -> Result<()> {
        let index = self.entity_index(reference)?;
        self.entities.remove(index);
        Ok(())
    }
    /// Sets NBT at a local cell after checking id against the owning block.
    pub fn set_block_entity(&mut self, at: Position, data: Compound) -> Result<()> {
        let id = nbt::string(nbt::get(&data, "id")?)?;
        if !helpers::compatible(id, &self.get(at)) {
            return Err(format!(
                "Block entity {id} is not supported for {} at {at:?}",
                self.get(at).name
            ));
        }
        self.block_entities.insert(at, data);
        Ok(())
    }
}

/// Stream cells in the map's [x, y, z] key order. Filtered selections already
/// have this order; rectangular selections need no position list or sorting.
fn fill_positions(selection: &Selection) -> Box<dyn Iterator<Item = Position> + '_> {
    if let Some(cells) = &selection.cells {
        Box::new(cells.iter().copied())
    } else {
        let Bounds { start, size } = selection.bounds;
        Box::new((start[0]..start[0] + size[0]).flat_map(move |x| {
            (start[1]..start[1] + size[1])
                .flat_map(move |y| (start[2]..start[2] + size[2]).map(move |z| [x, y, z]))
        }))
    }
}
