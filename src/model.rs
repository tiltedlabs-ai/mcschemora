use crate::{Result, registry};
use fastnbt::Value;
use std::collections::{BTreeMap, BTreeSet};

pub type Pos = [i32; 3];
pub type Compound = std::collections::HashMap<String, Value>;
pub const MAX_VOLUME: usize = 16_777_216;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Block {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}
impl Block {
    pub fn new(name: &str, properties: BTreeMap<String, String>) -> Result<Self> {
        let name = registry::namespace(name);
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
    pub fn air() -> Self {
        Self {
            name: "minecraft:air".into(),
            properties: BTreeMap::new(),
        }
    }
    pub fn is_air(&self) -> bool {
        matches!(
            self.name.as_str(),
            "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
        )
    }
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Bounds {
    pub start: Pos,
    pub size: Pos,
}
impl Bounds {
    pub fn new(start: Pos, size: Pos) -> Result<Self> {
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
    pub fn center(&self) -> [f64; 3] {
        std::array::from_fn(|i| self.start[i] as f64 + self.size[i] as f64 / 2.)
    }
    pub fn contains(&self, p: Pos) -> bool {
        (0..3).all(|i| {
            p[i] >= self.start[i]
                && i64::from(p[i]) < i64::from(self.start[i]) + i64::from(self.size[i])
        })
    }
    pub fn contains_entity(&self, p: [f64; 3]) -> bool {
        (0..3).all(|i| {
            p[i] >= self.start[i] as f64 && p[i] < self.start[i] as f64 + self.size[i] as f64
        })
    }
    pub fn positions(&self) -> impl Iterator<Item = Pos> + '_ {
        (self.start[1]..self.start[1] + self.size[1]).flat_map(move |y| {
            (self.start[2]..self.start[2] + self.size[2]).flat_map(move |z| {
                (self.start[0]..self.start[0] + self.size[0]).map(move |x| [x, y, z])
            })
        })
    }
    pub fn around(mut points: impl Iterator<Item = Pos>) -> Result<Self> {
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
#[derive(Clone, Debug)]
pub struct Entity {
    pub reference: u64,
    pub position_float: bool,
    pub position: [f64; 3],
    pub data: Compound,
}
/// Retained native fields, separate from editable blocks and entities.
#[derive(Clone, Debug, Default)]
pub struct RetainedData {
    pub spatial: Compound,
    pub bedrock: Option<BedrockData>,
}
impl RetainedData {
    pub fn is_empty(&self) -> bool {
        self.spatial.is_empty() && self.bedrock.is_none()
    }
}
#[derive(Clone, Debug)]
pub struct BedrockData {
    pub palette: BTreeMap<Block, Compound>,
    pub order: Vec<Block>,
    pub secondary: Vec<i32>,
    pub size: Pos,
    /// Only fields other than block_entity_data; Region owns all block entities.
    pub position_data: BTreeMap<Pos, Compound>,
}

#[derive(Clone, Debug, Default)]
pub struct Region {
    pub origin: Pos,
    pub bounds: Bounds,
    pub blocks: BTreeMap<Pos, Block>,
    pub block_entities: BTreeMap<Pos, Compound>,
    pub entities: Vec<Entity>,
    pub retained: RetainedData,
    pub present: Option<BTreeSet<Pos>>,
}
impl Region {
    pub fn new(origin: Pos) -> Self {
        Self {
            origin,
            ..Self::default()
        }
    }
    pub fn get(&self, p: Pos) -> Block {
        self.blocks.get(&p).cloned().unwrap_or_else(Block::air)
    }
    pub fn expanded(&self, positions: impl Iterator<Item = Pos>) -> Result<Bounds> {
        let mut ends = Vec::new();
        if self.bounds.size.iter().all(|&v| v > 0) {
            ends.push(self.bounds.start);
            ends.push(std::array::from_fn(|i| {
                self.bounds.start[i] + self.bounds.size[i] - 1
            }));
        }
        Bounds::around(ends.into_iter().chain(positions))
    }
    pub fn check_bounds(&self, bounds: Bounds) -> Result<()> {
        if !self.retained.is_empty()
            && (bounds.start != self.bounds.start || bounds.size != self.bounds.size)
        {
            return Err("Cannot resize a region with retained spatial data; export a supported conversion first".into());
        }
        Ok(())
    }
    pub fn write(
        &mut self,
        edits: Vec<(Pos, Block, Option<Compound>)>,
        grow: Option<Bounds>,
    ) -> Result<()> {
        let bounds = match grow {
            Some(bounds) => bounds,
            None => self.expanded(edits.iter().map(|(p, _, _)| *p))?,
        };
        self.check_bounds(bounds)?;
        for (p, b, nbt) in edits {
            if let Some(present) = &mut self.present {
                present.insert(p);
            }
            if self.get(p).name != b.name {
                self.block_entities.remove(&p);
            }
            if b == Block::air() {
                self.blocks.remove(&p);
                self.block_entities.remove(&p);
            } else {
                self.blocks.insert(p, b);
            }
            if let Some(data) = nbt {
                self.block_entities.insert(p, data);
            }
        }
        self.bounds = bounds;
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Document {
    pub data: std::sync::Arc<registry::MinecraftData>,
    pub catalog: Option<std::sync::Arc<registry::Registry>>,
    pub edition: String,
    pub version: String,
    pub data_version: i32,
    pub regions: BTreeMap<String, Region>,
    pub metadata: Compound,
    pub source_format: Option<String>,
    pub notices: Vec<String>,
    pub next_entity: u64,
}
impl Document {
    pub fn new(
        edition: &str,
        version: &str,
        data: std::sync::Arc<registry::MinecraftData>,
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
    pub(crate) fn imported(data: std::sync::Arc<registry::MinecraftData>) -> Self {
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
            next_entity: 1,
        }
    }
    pub fn registry(&self) -> Result<&registry::Registry> {
        self.catalog.as_deref().ok_or_else(|| {
            format!(
                "No authoring catalog for {} {}; supply a matching minecraft-data checkout",
                self.edition, self.version
            )
        })
    }
    pub fn region(&self, name: &str) -> Result<&Region> {
        self.regions
            .get(name)
            .ok_or_else(|| format!("Unknown region {name:?}"))
    }
    pub fn region_mut(&mut self, name: &str) -> Result<&mut Region> {
        self.regions
            .get_mut(name)
            .ok_or_else(|| format!("Unknown region {name:?}"))
    }
    pub fn validate(&self) -> Vec<String> {
        let mut issues = self.notices.clone();
        for (name, r) in &self.regions {
            for (p, b) in &r.blocks {
                if let Err(e) = self.registry().and_then(|catalog| catalog.resolve(b)) {
                    issues.push(format!("{name} {p:?}: {e}"));
                }
                if let Some(part) = b.properties.get("part")
                    && b.name.ends_with("_bed")
                {
                    let d = direction(
                        b.properties
                            .get("facing")
                            .map(String::as_str)
                            .unwrap_or("north"),
                    )
                    .unwrap_or([0, 0, -1]);
                    let sign = if part == "foot" { 1 } else { -1 };
                    let q = std::array::from_fn(|i| p[i].saturating_add(d[i] * sign));
                    let other = r.get(q);
                    if other.name != b.name
                        || other.properties.get("part") == Some(part)
                        || other.properties.get("facing") != b.properties.get("facing")
                    {
                        issues.push(format!("{name} {p:?}: incomplete bed"));
                    }
                }
                if b.name.ends_with("_door") && b.properties.contains_key("half") {
                    let half = &b.properties["half"];
                    let mut q = *p;
                    q[1] = q[1].saturating_add(if half == "lower" { 1 } else { -1 });
                    let other = r.get(q);
                    if other.name != b.name || other.properties.get("half") == Some(half) {
                        issues.push(format!("{name} {p:?}: incomplete door"));
                    }
                }
            }
        }
        issues
    }
}
#[derive(Clone, Debug)]
pub struct Selection {
    pub bounds: Bounds,
    pub cells: Option<BTreeSet<Pos>>,
    pub entities: BTreeSet<u64>,
}
impl Selection {
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
    pub fn positions(&self) -> Vec<Pos> {
        self.cells
            .as_ref()
            .map(|s| s.iter().copied().collect())
            .unwrap_or_else(|| self.bounds.positions().collect())
    }
    pub fn contains(&self, p: Pos) -> bool {
        self.cells
            .as_ref()
            .map(|s| s.contains(&p))
            .unwrap_or_else(|| self.bounds.contains(p))
    }
    pub fn filter(
        &self,
        r: &Region,
        id: Option<&str>,
        props: &BTreeMap<String, String>,
    ) -> Result<Self> {
        let id = id.map(registry::namespace);
        let cells: BTreeSet<Pos> = self
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
    pub fn snapshot(&self, r: &Region, edition: &str, version: &str) -> Result<Fragment> {
        if !r.retained.is_empty() {
            return Err(
                "Copying retained biome, tick, or Bedrock layer data is not implemented".into(),
            );
        }
        let edits = self
            .positions()
            .into_iter()
            .map(|p| {
                (
                    std::array::from_fn(|i| p[i] - self.bounds.start[i]),
                    r.get(p),
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
#[derive(Clone, Debug)]
pub struct Fragment {
    pub edition: String,
    pub version: String,
    pub size: Pos,
    pub edits: Vec<(Pos, Block, Option<Compound>)>,
    pub entities: Vec<Entity>,
}
pub fn direction(s: &str) -> Result<Pos> {
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
pub fn direction_name(p: Pos) -> &'static str {
    match p {
        [1, 0, 0] => "east",
        [-1, 0, 0] => "west",
        [0, 1, 0] => "up",
        [0, -1, 0] => "down",
        [0, 0, 1] => "south",
        _ => "north",
    }
}
