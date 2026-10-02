use crate::{Result, catalog, model::*};
use fastnbt::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub struct Transform {
    pub matrix: [[i32; 3]; 3],
    pub pivot: [f64; 3],
    pub offset: Pos,
}
impl Transform {
    pub fn move_by(offset: Pos) -> Self {
        Self {
            matrix: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
            pivot: [0.; 3],
            offset,
        }
    }
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
    pub fn cell(&self, p: Pos) -> Result<Pos> {
        let q = self.point(p.map(|n| n as f64 + 0.5))?;
        let q = q.map(|v| v - 0.5);
        if q.iter().any(|v| (*v - v.round()).abs() > 1e-8) {
            return Err("Pivot puts blocks off the integer grid; use a grid-aligned pivot".into());
        }
        Ok(q.map(|v| v.round() as i32))
    }
    fn vector(&self, p: Pos) -> Pos {
        std::array::from_fn(|i| (0..3).map(|j| self.matrix[i][j] * p[j]).sum())
    }
    fn angle(&self, radians: f64) -> f64 {
        let (x, z) = (-radians.sin(), radians.cos());
        let nx = self.matrix[0][0] as f64 * x + self.matrix[0][2] as f64 * z;
        let nz = self.matrix[2][0] as f64 * x + self.matrix[2][2] as f64 * z;
        (-nx).atan2(nz)
    }
    pub fn changes_orientation(&self) -> bool {
        self.matrix != [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    }
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
                if [
                    "Leash",
                    "Passengers",
                    "Brain",
                    "TileX",
                    "TileY",
                    "TileZ",
                    "SleepingX",
                    "SleepingY",
                    "SleepingZ",
                ]
                .iter()
                .any(|k| e.data.contains_key(*k))
                {
                    return Err(
                        "Entity has spatial links that cannot be transformed safely yet".into(),
                    );
                }
                e.position = self.point(e.position)?;
                crate::edit::check_position(e.position)?;
                if self.changes_orientation()
                    && let Some(Value::List(rot)) = e.data.get_mut("Rotation")
                    && let Some(Value::Float(yaw)) = rot.first_mut()
                {
                    *yaw = self.angle((*yaw as f64).to_radians()).to_degrees() as f32;
                }
                if self.changes_orientation()
                    && let Some(Value::List(motion)) = e.data.get_mut("Motion")
                    && motion.len() == 3
                {
                    let v = motion
                        .iter()
                        .map(|v| {
                            if let Value::Double(x) = v {
                                Ok(*x)
                            } else {
                                Err("Motion must be doubles".into())
                            }
                        })
                        .collect::<Result<Vec<_>>>()?;
                    for (i, component) in motion.iter_mut().enumerate() {
                        *component =
                            Value::Double((0..3).map(|j| self.matrix[i][j] as f64 * v[j]).sum());
                    }
                }
                if duplicate {
                    e.reference = *next;
                    *next = next.checked_add(1).ok_or("Entity reference overflow")?;
                    e.data.remove("UUID");
                    e.data.remove("UUIDMost");
                    e.data.remove("UUIDLeast");
                }
                Ok(e)
            })
            .collect()
    }
    pub fn block(&self, b: &Block, catalog: &catalog::Registry) -> Result<Block> {
        if !self.changes_orientation() {
            return Ok(b.clone());
        }
        catalog.resolve(b)?;
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
                "shape" if b.name.ends_with("rail") => {
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
            name: b.name.clone(),
            properties,
        })
    }
    fn direction(&self, side: &str) -> Result<&'static str> {
        Ok(direction_name(self.vector(direction(side)?)))
    }
}
pub fn transform_selection(
    doc: &mut Document,
    name: &str,
    sel: &Selection,
    t: Transform,
    duplicate: bool,
    replace: bool,
) -> Result<Selection> {
    let r = doc.region(name)?;
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
            t.block(&r.get(p), doc.registry()?)
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
    let mut next = doc.next_entity;
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
    let r = doc.region_mut(name)?;
    if !duplicate {
        r.clear(sel);
    }
    for (p, _, _) in &edits {
        r.block_entities.remove(p);
    }
    r.write(edits, Some(expanded))?;
    r.entities.extend(entities);
    doc.next_entity = next;
    Ok(result)
}
