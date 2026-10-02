//! Schematic values, local coordinate bounds, selections, and independent fragments.

mod block_storage;
pub use block_storage::BlockStorage;

use crate::{Result, catalog};
use fastnbt::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Integer X, Y, and Z cell coordinates.
pub type Position = [i32; 3];
/// An NBT compound preserving Minecraft tag types.
pub type Compound = std::collections::HashMap<String, Value>;
/// Maximum number of cells allowed in a bounded volume.
pub const MAX_VOLUME: usize = 16_777_216;

/// A block identifier and properties; catalog resolution validates and fills defaults.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Block {
    /// Namespaced Minecraft identifier.
    pub name: String,
    /// Minecraft property names mapped to string values.
    pub properties: BTreeMap<String, String>,
}
impl Block {
    /// Normalizes and checks an identifier without validating catalog properties.
    pub fn new(name: &str, properties: BTreeMap<String, String>) -> Result<Self> {
        let name = catalog::namespace(name);
        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_:./-".contains(c))
            || name.split(':').count() != 2
            || name.split(':').any(str::is_empty)
        {
            return Err(format!("Invalid block identifier {name:?}"));
        }
        Ok(Self { name, properties })
    }
    /// Creates an ordinary minecraft:air block without properties.
    pub fn air() -> Self {
        Self {
            name: "minecraft:air".into(),
            properties: BTreeMap::new(),
        }
    }
    /// Whether the identifier is air, cave_air, or void_air.
    pub fn is_air(&self) -> bool {
        matches!(
            self.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }
    /// Parses identifier[property=value,...], rejecting malformed or duplicate properties.
    pub fn parse(s: &str) -> Result<Self> {
        let (name, properties) = if let Some((n, tail)) = s.split_once('[') {
            let inner = tail.strip_suffix(']').ok_or("Block state is missing ]")?;
            let mut props = BTreeMap::new();
            if !inner.is_empty() {
                for pair in inner.split(',') {
                    let (k, v) = pair.split_once('=').ok_or("Expected property=value")?;
                    if props.insert(k.into(), v.into()).is_some() {
                        return Err(format!("Duplicate property {k}"));
                    }
                }
            }
            (n, props)
        } else {
            (s, BTreeMap::new())
        };
        Self::new(name, properties)
    }
    /// Returns the full state string with properties in sorted order.
    pub fn text(&self) -> String {
        let mut text = self.name.clone();
        if !self.properties.is_empty() {
            let properties = self
                .properties
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",");
            text.push_str(&format!("[{properties}]"));
        }
        text
    }
}

/// A box with an inclusive start and exclusive upper bounds.
#[derive(Clone, Copy, Debug, Default)]
pub struct Bounds {
    /// Minimum cell coordinates.
    pub start: Position,
    /// Nonnegative cell counts along X, Y, and Z.
    pub size: Position,
}
impl Bounds {
    /// Creates bounds, rejecting negative sizes, coordinate overflow, or excessive volume.
    pub fn new(start: Position, size: Position) -> Result<Self> {
        if size.iter().any(|&n| n < 0) {
            return Err("Size cannot be negative".into());
        }
        for i in 0..3 {
            start[i].checked_add(size[i]).ok_or("Coordinate overflow")?;
        }
        let b = Self { start, size };
        b.volume()?;
        Ok(b)
    }
    /// Returns the cell count, rejecting overflow or a value above MAX_VOLUME.
    pub fn volume(&self) -> Result<usize> {
        let n = self
            .size
            .iter()
            .try_fold(1usize, |n, &s| n.checked_mul(s as usize))
            .ok_or("Volume overflow")?;
        if n > MAX_VOLUME {
            return Err(format!(
                "Volume {n} exceeds the MVP limit of {MAX_VOLUME} cells"
            ));
        }
        Ok(n)
    }
    /// Returns the geometric center in cell-boundary coordinates.
    pub fn center(&self) -> [f64; 3] {
        std::array::from_fn(|i| self.start[i] as f64 + self.size[i] as f64 / 2.)
    }
    /// Whether an integer cell lies inside the box.
    pub fn contains(&self, p: Position) -> bool {
        (0..3).all(|i| {
            p[i] >= self.start[i]
                && i64::from(p[i]) < i64::from(self.start[i]) + i64::from(self.size[i])
        })
    }
    /// Whether a floating-point entity position lies inside the box.
    pub fn contains_entity(&self, p: [f64; 3]) -> bool {
        (0..3).all(|i| {
            p[i] >= self.start[i] as f64 && p[i] < self.start[i] as f64 + self.size[i] as f64
        })
    }
    /// Iterates cells with X varying fastest, followed by Z and Y.
    pub fn positions(&self) -> impl Iterator<Item = Position> + '_ {
        (self.start[1]..self.start[1] + self.size[1]).flat_map(move |y| {
            (self.start[2]..self.start[2] + self.size[2]).flat_map(move |z| {
                (self.start[0]..self.start[0] + self.size[0]).map(move |x| [x, y, z])
            })
        })
    }
    /// Encloses all supplied cells; an empty iterator yields empty bounds.
    pub fn around(mut points: impl Iterator<Item = Position>) -> Result<Self> {
        let Some(first) = points.next() else {
            return Ok(Self::default());
        };
        let (mut min, mut max) = (first, first);
        for p in points {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let mut size = [0; 3];
        for i in 0..3 {
            size[i] = max[i]
                .checked_sub(min[i])
                .and_then(|v| v.checked_add(1))
                .ok_or("Bounds overflow")?;
        }
        Self::new(min, size)
    }
}
/// A free entity with a document-local reference and region-local position.
#[derive(Clone, Debug)]
pub struct Entity {
    /// Schematic-local reference used for entity operations.
    pub reference: u64,
    /// Whether imported position tags used floats rather than doubles.
    pub position_float: bool,
    /// Floating-point X, Y, and Z coordinates relative to the region origin.
    pub position: [f64; 3],
    /// Full entity NBT compound, including its identifier.
    pub data: Compound,
}
/// Native format fields retained separately from editable blocks and entities.
#[derive(Clone, Debug, Default)]
pub struct RetainedData {
    /// Format-specific spatial data, such as biome palettes and scheduled ticks.
    pub spatial: Compound,
    /// Bedrock palette and secondary-layer data, when present.
    pub bedrock: Option<BedrockData>,
}
impl RetainedData {
    /// Whether there is no retained spatial or Bedrock data.
    pub fn is_empty(&self) -> bool {
        self.spatial.is_empty() && self.bedrock.is_none()
    }
}
/// Native Bedrock fields needed to preserve a mcstructure file on export.
#[derive(Clone, Debug)]
pub struct BedrockData {
    /// Original typed NBT entries for block palette values.
    pub palette: BTreeMap<Block, Compound>,
    /// Original block palette order.
    pub order: Vec<Block>,
    /// Secondary-layer palette indices in source cell order.
    pub secondary: Vec<i32>,
    /// Dimensions associated with the retained source data.
    pub size: Position,
    /// Position fields other than block_entity_data; Region owns block entities.
    pub position_data: BTreeMap<Position, Compound>,
}

/// A named document area whose cells and entities use local coordinates.
#[derive(Clone, Debug, Default)]
pub struct Region {
    /// World coordinates corresponding to local [0, 0, 0].
    pub origin: Position,
    /// Stored extent in local cell coordinates.
    pub bounds: Bounds,
    /// Sparse block storage; absent entries read as air through get().
    pub blocks: BlockStorage,
    /// Block-entity compounds keyed by local cell coordinates.
    pub block_entities: BTreeMap<Position, Compound>,
    /// Free entities with local floating-point positions.
    pub entities: Vec<Entity>,
    /// Native format data that may restrict resizing or transforms.
    pub retained: RetainedData,
    /// Explicit placement mask; None means every cell in bounds is present.
    pub present: Option<BTreeSet<Position>>,
}
impl Region {
    /// Creates an empty region at the given world origin.
    pub fn new(origin: Position) -> Self {
        Self {
            origin,
            ..Self::default()
        }
    }
    /// Returns the block at a local position, or ordinary air for an absent entry.
    pub fn get(&self, p: Position) -> Block {
        self.blocks.get(&p).cloned().unwrap_or_else(Block::air)
    }
    /// Computes bounds enclosing the existing extent and supplied local cells.
    pub fn expanded(&self, positions: impl Iterator<Item = Position>) -> Result<Bounds> {
        let mut ends = Vec::new();
        if self.bounds.size.iter().all(|&v| v > 0) {
            ends.push(self.bounds.start);
            ends.push(std::array::from_fn(|i| {
                self.bounds.start[i] + self.bounds.size[i] - 1
            }));
        }
        Bounds::around(ends.into_iter().chain(positions))
    }
    /// Rejects a bounds change when retained spatial data prevents resizing.
    pub fn check_bounds(&self, bounds: Bounds) -> Result<()> {
        if !self.retained.is_empty()
            && (bounds.start != self.bounds.start || bounds.size != self.bounds.size)
        {
            return Err("Cannot resize a region with retained spatial data; export a supported conversion first".into());
        }
        Ok(())
    }
    /// Writes resolved cells and optional attached NBT after checking the target bounds.
    ///
    /// Use Schematic::set_blocks for catalog validation. None for grow computes expanded bounds;
    /// a supplied bound is used as-is. Later writes to a position take precedence.
    pub fn write<B: std::borrow::Borrow<Block>>(
        &mut self,
        edits: Vec<(Position, B, Option<Compound>)>,
        grow: Option<Bounds>,
    ) -> Result<()> {
        let bounds = match grow {
            Some(bounds) => bounds,
            None => self.expanded(edits.iter().map(|(p, _, _)| *p))?,
        };
        self.check_bounds(bounds)?;
        for (p, b, nbt) in edits {
            let b = b.borrow();
            let id = self.block_id(b);
            self.write_cell(p, id);
            if let Some(data) = nbt {
                self.block_entities.insert(p, data);
            }
        }
        self.bounds = bounds;
        Ok(())
    }
    fn block_id(&mut self, block: &Block) -> Option<u32> {
        if block.name == "minecraft:air" && block.properties.is_empty() {
            None
        } else {
            Some(self.blocks.intern(block))
        }
    }
    fn write_cell(&mut self, position: Position, id: Option<u32>) {
        if let Some(present) = &mut self.present {
            present.insert(position);
        }
        if let Some(id) = id {
            if self.blocks.replace_id(position, id) {
                self.block_entities.remove(&position);
            }
        } else {
            self.blocks.remove(&position);
            self.block_entities.remove(&position);
        }
    }
    pub(crate) fn write_indexed(
        &mut self,
        palette: &[Block],
        cells: Vec<(Position, usize)>,
    ) -> Result<()> {
        let bounds = self.expanded(cells.iter().map(|(p, _)| *p))?;
        self.check_bounds(bounds)?;
        let ids: Vec<_> = palette.iter().map(|block| self.block_id(block)).collect();
        for (position, index) in cells {
            self.write_cell(position, ids[index]);
        }
        self.bounds = bounds;
        Ok(())
    }
}
/// A versioned Minecraft document containing named regions and metadata.
///
/// Use catalog-checked editing methods to preserve block and bounds invariants.
#[derive(Clone, Debug)]
pub struct Schematic {
    /// Shared catalog and asset provider.
    pub data: std::sync::Arc<catalog::MinecraftData>,
    /// Loaded authoring registry, absent if the imported version has no catalog.
    pub catalog: Option<std::sync::Arc<catalog::Registry>>,
    /// Minecraft edition, such as java or bedrock.
    pub edition: String,
    /// Minecraft version string associated with the document.
    pub version: String,
    /// Numeric Minecraft data version.
    pub data_version: i32,
    /// Regions indexed by their unique names.
    pub regions: BTreeMap<String, Region>,
    /// Typed document metadata compound.
    pub metadata: Compound,
    /// Original import codec, if the document was decoded from a file.
    pub source_format: Option<String>,
    /// Notices retained from format decoding and document operations.
    pub notices: Vec<String>,
    /// Assumptions or omissions reported during import.
    pub import_diagnostics: Vec<String>,
    /// Next document-local entity reference allocated by editing operations.
    pub next_entity: u64,
}
impl Schematic {
    /// Creates a Java document with a main region using an already loaded catalog.
    ///
    /// Call MinecraftData::load first. New Bedrock authoring is unsupported.
    pub fn new(
        edition: &str,
        version: &str,
        data: std::sync::Arc<catalog::MinecraftData>,
    ) -> Result<Self> {
        if edition != "java" {
            return Err("New authoring currently supports Java Edition".into());
        }
        let catalog = data.registry(version)?;
        let mut doc = Self::imported(data);
        doc.version = catalog.version.clone();
        doc.data_version = catalog.data_version;
        doc.catalog = Some(catalog);
        doc.regions.insert("main".into(), Region::new([0; 3]));
        Ok(doc)
    }
    pub(crate) fn imported(data: std::sync::Arc<catalog::MinecraftData>) -> Self {
        Self {
            data,
            catalog: None,
            edition: "java".into(),
            version: "unknown".into(),
            data_version: 0,
            regions: BTreeMap::new(),
            metadata: Compound::new(),
            source_format: None,
            notices: vec![],
            import_diagnostics: vec![],
            next_entity: 1,
        }
    }
    /// Returns the authoring catalog, or an error if none is available.
    pub fn registry(&self) -> Result<&catalog::Registry> {
        self.catalog.as_deref().ok_or_else(|| {
            format!(
                "No authoring catalog for {} {}; no catalog is available in the pinned minecraft-data snapshot",
                self.edition, self.version
            )
        })
    }
    /// Borrows a named region, or returns an error if it does not exist.
    pub fn region(&self, name: &str) -> Result<&Region> {
        self.regions
            .get(name)
            .ok_or_else(|| format!("Unknown region {name:?}"))
    }
    /// Mutably borrows a named region, or returns an error if it does not exist.
    pub fn region_mut(&mut self, name: &str) -> Result<&mut Region> {
        self.regions
            .get_mut(name)
            .ok_or_else(|| format!("Unknown region {name:?}"))
    }
    /// Checks game rules without changing the document or simulating world ticks.
    pub fn validate(&self) -> crate::validate::Report {
        crate::validate::validate(self)
    }
}
/// Fixed membership of local cells and document-local entity references.
#[derive(Clone, Debug)]
pub struct Selection {
    /// Local bounding box for the selected cells.
    pub bounds: Bounds,
    /// Explicit cell membership; None selects every cell in bounds.
    pub cells: Option<BTreeSet<Position>>,
    /// References of selected free entities.
    pub entities: BTreeSet<u64>,
}
impl Selection {
    /// Selects all cells and the currently enclosed free entities in a local box.
    pub fn new(r: &Region, bounds: Bounds) -> Self {
        Self {
            bounds,
            cells: None,
            entities: r
                .entities
                .iter()
                .filter(|e| bounds.contains_entity(e.position))
                .map(|e| e.reference)
                .collect(),
        }
    }
    /// Returns the selected local cell coordinates, including air cells.
    pub fn positions(&self) -> Vec<Position> {
        self.cells
            .as_ref()
            .map(|s| s.iter().copied().collect())
            .unwrap_or_else(|| self.bounds.positions().collect())
    }
    /// Whether a local cell is a member of this selection.
    pub fn contains(&self, p: Position) -> bool {
        self.cells
            .as_ref()
            .map(|s| s.contains(&p))
            .unwrap_or_else(|| self.bounds.contains(p))
    }
    /// Returns fixed cell membership matching an ID and properties; excludes entities.
    pub fn filter(
        &self,
        r: &Region,
        id: Option<&str>,
        props: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let id = id.map(catalog::namespace);
        let cells: BTreeSet<Position> = self
            .positions()
            .into_iter()
            .filter(|&p| {
                let b = r.get(p);
                id.as_ref().is_none_or(|id| id == &b.name)
                    && props.iter().all(|(k, v)| b.properties.get(k) == Some(v))
            })
            .collect();
        Ok(Self {
            bounds: Bounds::around(cells.iter().copied())?,
            cells: Some(cells),
            entities: BTreeSet::new(),
        })
    }
    /// Copies selected content relative to the minimum corner without changing the source.
    ///
    /// Includes selected air and attached NBT. Retained format data prevents copying.
    pub fn snapshot(&self, r: &Region, edition: &str, version: &str) -> Result<Fragment> {
        if !r.retained.is_empty() {
            return Err(
                "Copying retained biome, tick, or Bedrock layer data is not implemented".into(),
            );
        }
        let air = std::sync::Arc::new(Block::air());
        let edits = self
            .positions()
            .into_iter()
            .map(|p| {
                (
                    std::array::from_fn(|i| p[i] - self.bounds.start[i]),
                    r.blocks.shared(&p).unwrap_or_else(|| air.clone()),
                    r.block_entities.get(&p).cloned(),
                )
            })
            .collect();
        let entities = r
            .entities
            .iter()
            .filter(|e| self.entities.contains(&e.reference))
            .map(|e| {
                let mut e = e.clone();
                for i in 0..3 {
                    e.position[i] -= self.bounds.start[i] as f64;
                }
                e
            })
            .collect();
        Ok(Fragment {
            edition: edition.into(),
            version: version.into(),
            size: self.bounds.size,
            edits,
            entities,
        })
    }
}
/// An independent selection snapshot for pasting into a matching edition and version.
#[derive(Clone, Debug)]
pub struct Fragment {
    /// Minecraft edition required by the paste destination.
    pub edition: String,
    /// Minecraft version required by the paste destination.
    pub version: String,
    /// Copied bounding-box dimensions.
    pub size: Position,
    /// Relative cell positions, shared block values, and optional attached NBT.
    pub edits: Vec<(Position, std::sync::Arc<Block>, Option<Compound>)>,
    /// Copied free entities with positions relative to the fragment origin.
    pub entities: Vec<Entity>,
}
/// Converts north, south, east, west, up, or down to a unit coordinate vector.
pub fn direction(s: &str) -> Result<Position> {
    match s {
        "east" => Ok([1, 0, 0]),
        "west" => Ok([-1, 0, 0]),
        "up" => Ok([0, 1, 0]),
        "down" => Ok([0, -1, 0]),
        "south" => Ok([0, 0, 1]),
        "north" => Ok([0, 0, -1]),
        _ => Err(format!("Unknown direction {s}")),
    }
}
/// Names a cardinal unit vector; unrecognized vectors fall back to north.
pub fn direction_name(p: Position) -> &'static str {
    match p {
        [1, 0, 0] => "east",
        [-1, 0, 0] => "west",
        [0, 1, 0] => "up",
        [0, -1, 0] => "down",
        [0, 0, 1] => "south",
        _ => "north",
    }
}
