//! Grid-aligned local transforms with block-state and entity orientation updates.

use crate::{Result, catalog, model::*};
use fastnbt::Value;
use std::collections::BTreeSet;

/// A grid transform applied to local cells, block states, and entities.
#[derive(Clone, Copy)]
pub struct Transform {
    /// Integer orientation matrix; supported operations use axis-aligned transforms.
    pub matrix: [[i32; 3]; 3],
    /// Floating-point pivot in local cell-boundary coordinates.
    pub pivot: [f64; 3],
    /// Integer displacement applied after the pivoted orientation transform.
    pub offset: Position,
}
impl Transform {
    /// Creates an integer translation without changing orientation.
    pub fn move_by(offset: Position) -> Self {
        Self {
            matrix: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
            pivot: [0.; 3],
            offset,
        }
    }
    /// Creates quarter turns about Y; positive steps turn north toward west.
    pub fn rotate(axis: &str, steps: i32, pivot: [f64; 3]) -> Result<Self> {
        if axis != "y" {
            return Err(
                "MVP rotations support axis='y'; vertical block-state mappings are not implemented"
                    .into(),
            );
        }
        let m = match steps.rem_euclid(4) {
            0 => [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
            1 => [[0, 0, 1], [0, 1, 0], [-1, 0, 0]],
            2 => [[-1, 0, 0], [0, 1, 0], [0, 0, -1]],
            _ => [[0, 0, -1], [0, 1, 0], [1, 0, 0]],
        };
        Ok(Self {
            matrix: m,
            pivot,
            offset: [0; 3],
        })
    }
    /// Creates a reflection about an X or Z plane at the supplied local coordinate.
    pub fn flip(axis: &str, center: f64) -> Result<Self> {
        let a = match axis {
            "x" => 0,
            "z" => 2,
            _ => return Err("MVP flips support x or z".into()),
        };
        let mut t = Self::move_by([0; 3]);
        t.matrix[a][a] = -1;
        t.pivot[a] = center;
        Ok(t)
    }
    /// Transforms a floating-point position, rejecting nonfinite or out-of-range results.
    pub fn point(&self, p: [f64; 3]) -> Result<[f64; 3]> {
        let q = std::array::from_fn(|i| {
            self.pivot[i]
                + self.offset[i] as f64
                + (0..3)
                    .map(|j| self.matrix[i][j] as f64 * (p[j] - self.pivot[j]))
                    .sum::<f64>()
        });
        if q.iter()
            .any(|v| !v.is_finite() || *v < i32::MIN as f64 || *v > i32::MAX as f64)
        {
            return Err("Transform coordinate out of range".into());
        }
        Ok(q)
    }
    /// Transforms a cell by its center, rejecting results off the integer grid.
    pub fn cell(&self, p: Position) -> Result<Position> {
        let q = self.point(p.map(|n| n as f64 + 0.5))?;
        let q = q.map(|v| v - 0.5);
        if q.iter().any(|v| (*v - v.round()).abs() > 1e-8) {
            return Err("Pivot puts blocks off the integer grid; use a grid-aligned pivot".into());
        }
        Ok(q.map(|v| v.round() as i32))
    }
    fn vector(&self, p: Position) -> Position {
        std::array::from_fn(|i| (0..3).map(|j| self.matrix[i][j] * p[j]).sum())
    }
    fn angle(&self, radians: f64) -> f64 {
        let (x, z) = (-radians.sin(), radians.cos());
        let nx = self.matrix[0][0] as f64 * x + self.matrix[0][2] as f64 * z;
        let nz = self.matrix[2][0] as f64 * x + self.matrix[2][2] as f64 * z;
        (-nx).atan2(nz)
    }
    /// Whether the transform rotates or reflects rather than only translating.
    pub fn changes_orientation(&self) -> bool {
        self.matrix != [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    }
    /// Whether the horizontal orientation reverses handedness.
    pub fn mirrored(&self) -> bool {
        self.matrix[0][0] * self.matrix[2][2] - self.matrix[0][2] * self.matrix[2][0] < 0
    }
    pub(crate) fn entities<'a>(
        &self,
        source: impl Iterator<Item = &'a Entity>,
        next: &mut u64,
        duplicate: bool,
    ) -> Result<Vec<Entity>> {
        source
            .map(|e| {
                let mut e = e.clone();
                e.position = self.point(e.position)?;
                crate::edit::check_position(e.position)?;
                self.entity_data(&mut e.data, e.position, duplicate, 0)?;
                if duplicate {
                    e.reference = *next;
                    *next = next.checked_add(1).ok_or("Entity reference overflow")?;
                }
                Ok(e)
            })
            .collect()
    }
    pub(crate) fn entity_data(
        &self,
        data: &mut Compound,
        position: [f64; 3],
        duplicate: bool,
        depth: usize,
    ) -> Result<()> {
        use crate::nbt::{doubles, get, number, xyz};
        if depth > 32 {
            return Err("Entity passenger depth exceeded".into());
        }
        let positions = [
            "sleeping_pos",
            "flower_pos",
            "hive_pos",
            "beam_target",
            "patrol_target",
            "wander_target",
            "bound_pos",
            "anchor_pos",
            "home_pos",
        ];
        if self.changes_orientation()
            && ([
                "Leash",
                "leash",
                "Passengers",
                "Brain",
                "TileX",
                "TileY",
                "TileZ",
                "block_pos",
                "SleepingX",
                "SleepingY",
                "SleepingZ",
            ]
            .iter()
            .any(|key| data.contains_key(*key))
                || positions.iter().any(|key| data.contains_key(*key)))
        {
            return Err(
                "Entity has spatial links whose orientation cannot be transformed safely yet"
                    .into(),
            );
        }
        let frame = matches!(data.get("id"), Some(Value::String(id)) if matches!(id.as_str(), "minecraft:item_frame" | "minecraft:glow_item_frame"));
        for keys in [
            ["TileX", "TileY", "TileZ"],
            ["SleepingX", "SleepingY", "SleepingZ"],
        ] {
            if keys.iter().any(|key| data.contains_key(*key)) {
                let anchor = if frame && keys[0] == "TileX" {
                    position.map(|coordinate| coordinate.floor() as i32)
                } else {
                    self.cell([
                        number(get(data, keys[0])?)?,
                        number(get(data, keys[1])?)?,
                        number(get(data, keys[2])?)?,
                    ])?
                };
                for (key, coordinate) in keys.into_iter().zip(anchor) {
                    data.insert(key.into(), Value::Int(coordinate));
                }
            }
        }
        if let Some(value) = data.get_mut("block_pos") {
            let anchor = if frame {
                position.map(|coordinate| coordinate.floor() as i32)
            } else {
                self.cell(xyz(value)?)?
            };
            *value = crate::nbt::int_array(anchor);
        }
        for key in positions {
            if let Some(value) = data.get_mut(key) {
                *value = crate::nbt::int_array(self.cell(xyz(value)?)?);
            }
        }
        if let Some(Value::Compound(leash)) = data.get_mut("Leash")
            && ["X", "Y", "Z"].iter().any(|key| leash.contains_key(*key))
        {
            let anchor = self.cell([
                number(get(leash, "X")?)?,
                number(get(leash, "Y")?)?,
                number(get(leash, "Z")?)?,
            ])?;
            for (key, coordinate) in ["X", "Y", "Z"].into_iter().zip(anchor) {
                leash.insert(key.into(), Value::Int(coordinate));
            }
        }
        if let Some(value @ Value::IntArray(_)) = data.get_mut("leash")
            && matches!(value, Value::IntArray(values) if values.len() == 3)
        {
            *value = crate::nbt::int_array(self.cell(xyz(value)?)?);
        }
        if let Some(value) = data.get_mut("Passengers") {
            let Value::List(passengers) = value else {
                return Err("Passengers must be a list".into());
            };
            for value in passengers {
                let Value::Compound(passenger) = value else {
                    return Err("Passenger must be a compound".into());
                };
                let passenger_position = if let Some(value) = passenger.get_mut("Pos") {
                    let destination = self.point(doubles(value)?)?;
                    *value = crate::nbt::double_list(destination);
                    destination
                } else {
                    position
                };
                self.entity_data(passenger, passenger_position, duplicate, depth + 1)?;
            }
        }
        if self.changes_orientation()
            && let Some(Value::List(rotation)) = data.get_mut("Rotation")
            && let Some(Value::Float(yaw)) = rotation.first_mut()
        {
            *yaw = self.angle((*yaw as f64).to_radians()).to_degrees() as f32;
        }
        if self.changes_orientation()
            && let Some(value) = data.get_mut("Motion")
        {
            let motion = doubles(value)?;
            *value = crate::nbt::double_list(std::array::from_fn(|i| {
                (0..3).map(|j| self.matrix[i][j] as f64 * motion[j]).sum()
            }));
        }
        if duplicate {
            for key in ["UUID", "UUIDMost", "UUIDLeast"] {
                data.remove(key);
            }
        }
        Ok(())
    }
    /// Transforms orientation properties after catalog resolution.
    ///
    /// Errors when a supported state mapping cannot be established.
    pub fn block(&self, b: &Block, catalog: &catalog::Registry) -> Result<Block> {
        if !self.changes_orientation() {
            return Ok(b.clone());
        }
        let b = catalog.resolve(b)?;
        let mut properties = std::collections::BTreeMap::new();
        for (key, value) in &b.properties {
            let mut key = key.as_str();
            let mapped = match key {
                "facing" => self.direction(value)?.into(),
                "north" | "south" | "east" | "west" => {
                    key = self.direction(key)?;
                    value.clone()
                }
                "axis" if value == "x" || value == "z" => {
                    let v = self.vector(if value == "x" { [1, 0, 0] } else { [0, 0, 1] });
                    if v[0] != 0 { "x" } else { "z" }.into()
                }
                "rotation" => {
                    let r = value
                        .parse::<f64>()
                        .map_err(|_| "Invalid standing rotation")?;
                    let r = (self.angle(r * std::f64::consts::TAU / 16.) * 16.
                        / std::f64::consts::TAU)
                        .round() as i32;
                    r.rem_euclid(16).to_string()
                }
                "shape" if b.id.ends_with("rail") => {
                    if let Some(side) = value.strip_prefix("ascending_") {
                        format!("ascending_{}", self.direction(side)?)
                    } else {
                        let mut sides = value
                            .split('_')
                            .map(|s| self.direction(s))
                            .collect::<Result<Vec<_>>>()?;
                        let order = ["north", "south", "east", "west"];
                        sides.sort_by_key(|s| order.iter().position(|d| d == s));
                        sides.join("_")
                    }
                }
                "shape" | "hinge" | "type" | "side_chain" if self.mirrored() => {
                    match value.as_str() {
                        "inner_left" => "inner_right",
                        "inner_right" => "inner_left",
                        "outer_left" => "outer_right",
                        "outer_right" => "outer_left",
                        "left" => "right",
                        "right" => "left",
                        _ => value,
                    }
                    .into()
                }
                "orientation" => value
                    .split('_')
                    .map(|side| self.direction(side))
                    .collect::<Result<Vec<_>>>()?
                    .join("_"),
                _ => value.clone(),
            };
            properties.insert(key.into(), mapped);
        }
        catalog.resolve(&Block {
            id: b.id.clone(),
            properties,
        })
    }
    fn direction(&self, side: &str) -> Result<&'static str> {
        Ok(direction_name(self.vector(direction(side)?)))
    }
}
/// Transforms selected content atomically and returns its destination membership.
///
/// Coordinates are local to the named region. duplicate preserves source content; replace
/// allows occupied targets. Selected air clears destinations, and self-overlap is safe
/// for moves. Retained spatial data or unsupported state and NBT mappings cause errors.
pub fn transform_selection(
    schematic: &mut Schematic,
    name: &str,
    sel: &Selection,
    t: Transform,
    duplicate: bool,
    replace: bool,
) -> Result<Selection> {
    let r = schematic.region(name)?;
    if !r.retained.is_empty() {
        return Err("Region has retained format data (biomes or ticks); transform requires explicit removal or a supported mapping".into());
    }
    let points = sel.positions();
    if points.is_empty() && sel.entities.is_empty() {
        return Ok(sel.clone());
    }
    let mut edits = Vec::with_capacity(points.len());
    for &p in &points {
        let q = t.cell(p)?;
        if !replace && (duplicate || !sel.contains(q)) && !r.get(q).is_air() {
            return Err(format!(
                "{name} {q:?}: destination occupied; use replace=True"
            ));
        }
        let b = if t.changes_orientation() {
            t.block(&r.get(p), schematic.registry()?)
                .map_err(|e| format!("{name} {p:?}: {e}"))?
        } else {
            r.get(p)
        };
        let mut data = r.block_entities.get(&p).cloned();
        if let Some(data) = data.as_mut() {
            if t.changes_orientation()
                && data.len() > 4
                && !["RecordItem", "Items", "front_text", "back_text", "Text1"]
                    .iter()
                    .any(|k| data.contains_key(*k))
            {
                return Err(format!(
                    "{name} {p:?}: cannot verify spatial block-entity NBT transform"
                ));
            }
            data.remove("x");
            data.remove("y");
            data.remove("z");
        }
        edits.push((q, b, data));
    }
    let mut next = schematic.next_entity;
    let entities = t.entities(
        r.entities
            .iter()
            .filter(|e| sel.entities.contains(&e.reference)),
        &mut next,
        duplicate,
    )?;
    let cells: BTreeSet<_> = edits.iter().map(|(p, _, _)| *p).collect();
    let new_bounds = Bounds::around(cells.iter().copied())?;
    let expanded = r.expanded(points.iter().copied().chain(cells.iter().copied()))?;
    r.check_bounds(expanded)?;
    let result = Selection {
        bounds: new_bounds,
        cells: if sel.cells.is_none() {
            None
        } else {
            Some(cells)
        },
        entities: entities.iter().map(|e| e.reference).collect(),
    };
    let r = schematic.region_mut(name)?;
    if !duplicate {
        r.clear(sel);
    }
    for (p, _, _) in &edits {
        r.block_entities.remove(p);
    }
    r.write(edits, Some(expanded))?;
    r.entities.extend(entities);
    schematic.next_entity = next;
    Ok(result)
}
