use crate::{
    Result,
    model::{Block, Compound, Schematic},
    nbt::{self, Tag},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default, Serialize)]
pub struct Inspection {
    pub edition: String,
    pub version: String,
    pub data_version: i32,
    pub regions: BTreeMap<String, [i32; 3]>,
    pub volume: usize,
    pub blocks: BTreeMap<String, u64>,
    pub items: BTreeMap<String, u64>,
    pub entities: BTreeMap<String, u64>,
    pub inventories: Vec<Inventory>,
    pub identifiers: BTreeSet<String>,
    pub payload_fields: BTreeSet<String>,
    pub retained_data: bool,
    pub entities_outside_bounds: bool,
    pub entity_count: u64,
    pub moving_entities: bool,
    pub max_stack_count: u64,
}

#[derive(Serialize)]
pub struct Inventory {
    pub path: String,
    pub item: String,
    pub count: u64,
}

fn identifier(value: &Tag) -> Result<String> {
    Ok(Block::new(nbt::string(value)?, BTreeMap::new())?.id)
}

fn count(value: &Tag) -> Result<u64> {
    let n = match value {
        Tag::Byte(n) => i64::from(*n),
        Tag::Short(n) => i64::from(*n),
        Tag::Int(n) => i64::from(*n),
        Tag::Long(n) => *n,
        _ => return Err("Item count must be an integer".into()),
    };
    if n <= 0 {
        return Err("Item count must be positive".into());
    }
    Ok(n as u64)
}

fn add(counts: &mut BTreeMap<String, u64>, id: String, count: u64) -> Result<()> {
    let total = counts.entry(id).or_default();
    *total = total
        .checked_add(count)
        .filter(|n| *n <= i64::MAX as u64)
        .ok_or("Resource count overflow")?;
    Ok(())
}

fn nonempty(value: &Tag) -> bool {
    match value {
        Tag::Byte(n) => *n != 0,
        Tag::Short(n) => *n != 0,
        Tag::Int(n) => *n != 0,
        Tag::Long(n) => *n != 0,
        Tag::Float(n) => *n != 0.,
        Tag::Double(n) => *n != 0.,
        Tag::String(s) => !s.is_empty(),
        Tag::List(v) => !v.is_empty(),
        Tag::Compound(v) => !v.is_empty(),
        Tag::ByteArray(v) => !v.is_empty(),
        Tag::IntArray(v) => !v.is_empty(),
        Tag::LongArray(v) => !v.is_empty(),
    }
}

fn numeric(value: &Tag) -> Result<f64> {
    match value {
        Tag::Byte(n) => Ok(f64::from(*n)),
        Tag::Short(n) => Ok(f64::from(*n)),
        Tag::Int(n) => Ok(f64::from(*n)),
        Tag::Long(n) => Ok(*n as f64),
        Tag::Float(n) => Ok(f64::from(*n)),
        Tag::Double(n) => Ok(*n),
        _ => Err("Expected a number".into()),
    }
}

struct Scanner {
    report: Inspection,
    remaining: usize,
}

impl Scanner {
    fn scan(
        &mut self,
        value: &Tag,
        path: &str,
        multiplier: u64,
        item: bool,
        depth: usize,
    ) -> Result<()> {
        if depth > 32 || self.remaining == 0 {
            return Err("Payload nesting or node limit exceeded".into());
        }
        self.remaining -= 1;
        if matches!(value, Tag::Float(n) if !n.is_finite())
            || matches!(value, Tag::Double(n) if !n.is_finite())
        {
            return Err(format!("Nonfinite payload number at {path}"));
        }
        match value {
            Tag::List(values) => {
                for (i, child) in values.iter().enumerate() {
                    self.scan(child, &format!("{path}/{i}"), multiplier, item, depth + 1)?;
                }
            }
            Tag::Compound(fields) => {
                let mut multiplier = multiplier;
                if item && !fields.is_empty() && !fields.contains_key("id") {
                    return Err(format!("Inventory entry has no item ID at {path}"));
                }
                if let Some(id) = fields.get("id")
                    && (item || fields.contains_key("Count") || fields.contains_key("count"))
                {
                    if fields.contains_key("Count") && fields.contains_key("count") {
                        return Err(format!("Ambiguous item count at {path}"));
                    }
                    let id = identifier(id)?;
                    let count = fields
                        .get("count")
                        .or_else(|| fields.get("Count"))
                        .map(count)
                        .transpose()?
                        .unwrap_or(1);
                    self.report.max_stack_count = self.report.max_stack_count.max(count);
                    multiplier = multiplier
                        .checked_mul(count)
                        .filter(|n| *n <= i64::MAX as u64)
                        .ok_or("Nested inventory count overflow")?;
                    add(&mut self.report.items, id.clone(), multiplier)?;
                    self.report.inventories.push(Inventory {
                        path: path.into(),
                        item: id,
                        count: multiplier,
                    });
                }
                for (key, child) in fields {
                    let key_name = key.strip_prefix("minecraft:").unwrap_or(key).to_lowercase();
                    if nonempty(child) {
                        self.report.payload_fields.insert(key_name.clone());
                    }
                    if matches!(key.as_str(), "id" | "Id") && matches!(child, Tag::String(_)) {
                        self.report.identifiers.insert(identifier(child)?);
                    }
                    if key_name == "motion" {
                        let motion = nbt::list(child)?;
                        if motion.len() != 3 {
                            return Err(format!("Invalid entity motion at {path}"));
                        }
                        for coordinate in motion {
                            self.report.moving_entities |= numeric(coordinate)? != 0.;
                        }
                    }
                    let child_path = format!("{path}/{key}");
                    match key_name.as_str() {
                        "items" | "handitems" | "armoritems" | "inventory" => {
                            nbt::list(child)?;
                            self.scan(child, &child_path, multiplier, true, depth + 1)?;
                        }
                        "equipment" => {
                            for (slot, stack) in nbt::compound(child)? {
                                self.scan(
                                    stack,
                                    &format!("{child_path}/{slot}"),
                                    multiplier,
                                    true,
                                    depth + 1,
                                )?;
                            }
                        }
                        "bundle_contents" | "charged_projectiles" => {
                            self.scan(child, &child_path, multiplier, true, depth + 1)?
                        }
                        name if name.ends_with("item") => {
                            self.scan(child, &child_path, multiplier, true, depth + 1)?
                        }
                        _ => self.scan(child, &child_path, multiplier, false, depth + 1)?,
                    }
                }
            }
            _ if item => return Err(format!("Invalid item stack at {path}")),
            Tag::String(s) if s.trim_start().starts_with(['{', '[']) => {
                let json: serde_json::Value = serde_json::from_str(s)
                    .map_err(|e| format!("Invalid embedded JSON at {path}: {e}"))?;
                let tag = json_tag(json)?;
                self.scan(&tag, &format!("{path}/text"), multiplier, false, depth + 1)?;
            }
            _ => (),
        }
        Ok(())
    }

    fn entity(&mut self, entity: &Compound, depth: usize) -> Result<()> {
        if depth > 32 {
            return Err("Entity passenger depth exceeded".into());
        }
        self.report.entity_count = self
            .report
            .entity_count
            .checked_add(1)
            .ok_or("Entity count overflow")?;
        let id = identifier(
            entity
                .get("id")
                .or_else(|| entity.get("identifier"))
                .ok_or("Entity has no ID")?,
        )?;
        add(&mut self.report.entities, id, 1)?;
        if let Some(passengers) = entity.get("Passengers") {
            for passenger in nbt::list(passengers)? {
                self.entity(nbt::compound(passenger)?, depth + 1)?;
            }
        }
        Ok(())
    }
}

fn json_tag(value: serde_json::Value) -> Result<Tag> {
    Ok(match value {
        serde_json::Value::Null => Tag::String(String::new()),
        serde_json::Value::Bool(value) => Tag::Byte(i8::from(value)),
        serde_json::Value::String(value) => Tag::String(value),
        serde_json::Value::Number(value) => {
            if let Some(n) = value.as_i64() {
                Tag::Long(n)
            } else {
                Tag::Double(value.as_f64().ok_or("Invalid JSON number")?)
            }
        }
        serde_json::Value::Array(values) => {
            Tag::List(values.into_iter().map(json_tag).collect::<Result<_>>()?)
        }
        serde_json::Value::Object(values) => Tag::Compound(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, json_tag(value)?)))
                .collect::<Result<_>>()?,
        ),
    })
}

pub fn inspect(schematic: &Schematic) -> Result<Inspection> {
    let mut scanner = Scanner {
        report: Inspection {
            edition: schematic.edition.clone(),
            version: schematic.version.clone(),
            data_version: schematic.data_version,
            ..Inspection::default()
        },
        remaining: 2_000_000,
    };
    for (name, region) in &schematic.regions {
        let volume = region.bounds.volume()?;
        scanner.report.volume = scanner
            .report
            .volume
            .checked_add(volume)
            .ok_or("Schematic volume overflow")?;
        scanner
            .report
            .regions
            .insert(name.clone(), region.bounds.size);
        scanner.report.retained_data |= !region.retained.is_empty();
        let mut air = region.present.as_ref().map_or(volume, BTreeSet::len) as u64;
        for (position, block) in region.blocks.iter() {
            if region
                .present
                .as_ref()
                .is_some_and(|cells| !cells.contains(position))
            {
                continue;
            }
            air = air
                .checked_sub(1)
                .ok_or("Block count exceeds region volume")?;
            add(&mut scanner.report.blocks, block.id.clone(), 1)?;
        }
        if air > 0 {
            add(&mut scanner.report.blocks, "minecraft:air".into(), air)?;
        }
        for (position, data) in &region.block_entities {
            scanner.scan(
                &Tag::Compound(data.clone()),
                &format!("{name}/block_entities/{position:?}"),
                1,
                false,
                0,
            )?;
        }
        for entity in &region.entities {
            scanner.report.entities_outside_bounds |=
                !region.bounds.contains_entity(entity.position);
            scanner.entity(&entity.data, 0)?;
            scanner.scan(
                &Tag::Compound(entity.data.clone()),
                &format!("{name}/entities/{}", entity.reference),
                1,
                false,
                0,
            )?;
        }
    }
    scanner
        .report
        .inventories
        .sort_by(|a, b| a.path.cmp(&b.path));
    Ok(scanner.report)
}

#[derive(Default, Serialize)]
pub struct Resources {
    pub blocks: BTreeMap<String, u64>,
    pub items: BTreeMap<String, u64>,
    pub entities: BTreeMap<String, u64>,
    pub volume: usize,
    pub entity_count: u64,
    pub placements: Vec<Inspection>,
}

impl Resources {
    pub fn new(initial_items: &[(String, u64)]) -> Result<Self> {
        let mut resources = Self::default();
        for (id, count) in initial_items {
            add(
                &mut resources.items,
                Block::new(id, BTreeMap::new())?.id,
                *count,
            )?;
        }
        Ok(resources)
    }

    pub fn include(&mut self, schematic: &Schematic) -> Result<()> {
        let report = inspect(schematic)?;
        self.volume = self
            .volume
            .checked_add(report.volume)
            .ok_or("Combined volume overflow")?;
        self.entity_count = self
            .entity_count
            .checked_add(report.entity_count)
            .ok_or("Combined entity count overflow")?;
        for (total, counts) in [
            (&mut self.blocks, &report.blocks),
            (&mut self.items, &report.items),
            (&mut self.entities, &report.entities),
        ] {
            for (id, count) in counts {
                add(total, id.clone(), *count)?;
            }
        }
        self.placements.push(report);
        Ok(())
    }
}
