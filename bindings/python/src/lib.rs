//! PyO3 adapter. Core data and editing rules stay in Rust.
use mcschemora::{
    catalog, formats, helpers,
    model::*,
    nbt,
    transform::{Transform, transform_selection},
};
use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyBytes, PyDict},
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, MutexGuard},
};
type Shared = Arc<Mutex<Schematic>>;
type State = (String, BTreeMap<String, String>);
type RepairChange = (String, Position, State, State);
fn error(e: impl ToString) -> PyErr {
    PyValueError::new_err(e.to_string())
}
fn lock(d: &Shared) -> PyResult<MutexGuard<'_, Schematic>> {
    d.lock().map_err(|_| error("Schematic lock poisoned"))
}
fn state(b: Block) -> State {
    (b.name, b.properties)
}
fn block_snapshot<'a, 'py>(
    py: Python<'py>,
    blocks: impl Iterator<Item = (&'a Position, &'a Block)>,
    factory: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    let mut palette = HashMap::new();
    for (position, block) in blocks.filter(|(_, block)| !block.is_air()) {
        let key = block as *const Block;
        let value = match palette.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(factory.call1((state(block.clone()),))?)
            }
        };
        result.set_item((position[0], position[1], position[2]), &*value)?;
    }
    Ok(result)
}
fn render(
    schematic: &Schematic,
    options: mcschemora::render::SceneOptions,
    encode: impl FnOnce(&mcschemora::render::PreparedScene) -> mcschemora::Result<Vec<u8>>,
) -> PyResult<(Vec<u8>, Vec<String>)> {
    let assets = schematic
        .data
        .geometry_assets(&schematic.version)
        .map_err(error)?;
    let scene = assets.prepare(schematic, &options).map_err(error)?;
    let bytes = encode(&scene).map_err(error)?;
    let diagnostics = scene
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{} at {:?} in region {:?}: {}",
                d.block, d.position, d.region, d.message
            )
        })
        .collect();
    Ok((bytes, diagnostics))
}

#[pyclass(name = "MinecraftData")]
struct PyMinecraftData {
    data: Arc<catalog::MinecraftData>,
}
#[pymethods]
impl PyMinecraftData {
    #[new]
    #[pyo3(signature = (cache_dir=None, offline=false))]
    fn new(cache_dir: Option<std::path::PathBuf>, offline: bool) -> PyResult<Self> {
        Ok(Self {
            data: Arc::new(catalog::MinecraftData::new(cache_dir, offline).map_err(error)?),
        })
    }
    fn cache_dir(&self) -> std::path::PathBuf {
        self.data.cache_dir().to_path_buf()
    }
    fn versions(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        py.detach(|| {
            pollster::block_on(self.data.initialize())?;
            self.data.versions()
        })
        .map_err(error)
    }
    fn load(&self, py: Python<'_>, version: &str) -> PyResult<String> {
        py.detach(|| pollster::block_on(self.data.load(version)).map(|r| r.version.clone()))
            .map_err(error)
    }
    fn dataset(&self, py: Python<'_>, version: &str, kind: &str) -> PyResult<String> {
        py.detach(|| {
            pollster::block_on(self.data.load(version))?;
            self.data.dataset(version, kind).map(|v| v.to_string())
        })
        .map_err(error)
    }
    fn load_visuals(&self, py: Python<'_>, version: &str) -> PyResult<std::path::PathBuf> {
        py.detach(|| {
            pollster::block_on(self.data.initialize())?;
            self.data.load_visuals(version)
        })
        .map_err(error)
    }
}

#[pyclass(name = "Schematic")]
struct PySchematic {
    data: Shared,
}
#[pymethods]
impl PySchematic {
    #[new]
    fn new(
        py: Python<'_>,
        edition: &str,
        version: &str,
        source: &PyMinecraftData,
    ) -> PyResult<Self> {
        Ok(Self {
            data: Arc::new(Mutex::new(
                py.detach(|| {
                    pollster::block_on(source.data.load(version))?;
                    Schematic::new(edition, version, source.data.clone())
                })
                .map_err(error)?,
            )),
        })
    }
    #[staticmethod]
    fn from_bytes(
        py: Python<'_>,
        data: &[u8],
        format: &str,
        source: &PyMinecraftData,
        version: Option<String>,
        origin: Option<Position>,
        palette: std::collections::BTreeMap<String, String>,
    ) -> PyResult<Self> {
        let options = formats::ImportOptions {
            version,
            origin,
            palette,
        };
        Ok(Self {
            data: Arc::new(Mutex::new(
                py.detach(|| {
                    pollster::block_on(formats::decode(data, format, source.data.clone(), &options))
                })
                .map_err(error)?,
            )),
        })
    }
    #[pyo3(signature = (format, allow_loss, flatten, version=None))]
    fn to_bytes<'py>(
        &self,
        py: Python<'py>,
        format: &str,
        allow_loss: bool,
        flatten: bool,
        version: Option<&str>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let data = py.detach(|| {
            pollster::block_on(formats::encode(
                &*lock(&self.data)?,
                format,
                version,
                allow_loss,
                flatten,
            ))
            .map_err(error)
        })?;
        Ok(PyBytes::new(py, &data))
    }
    #[pyo3(signature = (format, flatten, version=None))]
    fn check_export(
        &self,
        py: Python<'_>,
        format: &str,
        flatten: bool,
        version: Option<&str>,
    ) -> PyResult<(Vec<String>, Vec<String>)> {
        let report = py.detach(|| {
            pollster::block_on(formats::check_export(
                &*lock(&self.data)?,
                format,
                version,
                flatten,
            ))
            .map_err(error)
        })?;
        Ok((report.errors, report.losses))
    }
    fn glb<'py>(
        &self,
        py: Python<'py>,
        region: Option<String>,
        ranges: [Option<[i32; 2]>; 3],
    ) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
        let [x, y, z] = ranges;
        let (bytes, diagnostics) = py.detach(|| {
            render(
                &*lock(&self.data)?,
                mcschemora::render::SceneOptions { region, x, y, z },
                mcschemora::render::glb::encode,
            )
        })?;
        Ok((PyBytes::new(py, &bytes), diagnostics))
    }
    fn png<'py>(
        &self,
        py: Python<'py>,
        region: Option<String>,
        ranges: [Option<[i32; 2]>; 3],
        size: [u32; 2],
        view: &str,
        grid: bool,
    ) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
        let view = view.parse().map_err(error)?;
        let [x, y, z] = ranges;
        let (bytes, diagnostics) = py.detach(|| {
            render(
                &*lock(&self.data)?,
                mcschemora::render::SceneOptions { region, x, y, z },
                |scene| {
                    mcschemora::render::png::encode(
                        scene,
                        &mcschemora::render::png::Options { size, view, grid },
                    )
                },
            )
        })?;
        Ok((PyBytes::new(py, &bytes), diagnostics))
    }
    fn sprites<'py>(
        &self,
        py: Python<'py>,
        region: Option<String>,
        ranges: [Option<[i32; 2]>; 3],
        view: &str,
        style: (u32, bool, bool),
        sprites: BTreeMap<String, String>,
    ) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
        let view = view.parse().map_err(error)?;
        let (cell_size, grid, entities) = style;
        let [x, y, z] = ranges;
        let output = py.detach(|| {
            mcschemora::render::sprites::encode(
                &*lock(&self.data)?,
                &mcschemora::render::sprites::Options {
                    selection: mcschemora::render::SceneOptions { region, x, y, z },
                    view,
                    cell_size,
                    grid,
                    entities,
                    sprites,
                },
            )
            .map_err(error)
        })?;
        Ok((PyBytes::new(py, &output.png), output.diagnostics))
    }
    fn blueprint(
        &self,
        py: Python<'_>,
        name: String,
        region: Option<String>,
        y: Option<[i32; 2]>,
        rotation: i32,
        sprites: std::collections::BTreeMap<String, String>,
    ) -> PyResult<(String, Vec<String>)> {
        let output = py.detach(|| {
            formats::blueprint::encode(
                &*lock(&self.data)?,
                &formats::blueprint::Options {
                    name,
                    region,
                    y,
                    rotation,
                    sprites,
                },
            )
            .map_err(error)
        })?;
        Ok((output.text, output.diagnostics))
    }
    fn validate(&self, py: Python<'_>) -> PyResult<(Vec<String>, Vec<String>, Vec<String>)> {
        let report = py.detach(|| -> PyResult<_> { Ok(lock(&self.data)?.validate()) })?;
        Ok((report.errors, report.warnings, report.unknown))
    }
    #[pyo3(signature = (rules=None))]
    fn repair(
        &self,
        py: Python<'_>,
        rules: Option<Vec<String>>,
    ) -> PyResult<(Vec<RepairChange>, Vec<String>)> {
        let report = py.detach(|| lock(&self.data)?.repair(rules.as_deref()).map_err(error))?;
        let changes = report
            .changes
            .into_iter()
            .map(|change| {
                (
                    change.region,
                    change.position,
                    (change.before.name, change.before.properties),
                    (change.after.name, change.after.properties),
                )
            })
            .collect();
        Ok((changes, report.skipped))
    }
    fn region(&self, name: &str) -> PyResult<PyRegion> {
        lock(&self.data)?.region(name).map_err(error)?;
        Ok(PyRegion {
            data: self.data.clone(),
            name: name.into(),
        })
    }
    fn add_region(&self, name: &str, origin: Position) -> PyResult<PyRegion> {
        lock(&self.data)?.add_region(name, origin).map_err(error)?;
        Ok(PyRegion {
            data: self.data.clone(),
            name: name.into(),
        })
    }
    fn regions(&self) -> PyResult<Vec<String>> {
        Ok(lock(&self.data)?.regions.keys().cloned().collect())
    }
    fn import_diagnostics(&self) -> PyResult<Vec<String>> {
        Ok(lock(&self.data)?.import_diagnostics.clone())
    }
    fn info(&self) -> PyResult<(String, String, i32)> {
        let d = lock(&self.data)?;
        Ok((d.edition.clone(), d.version.clone(), d.data_version))
    }
    fn describe(&self, id: &str) -> PyResult<String> {
        Ok(lock(&self.data)?
            .registry()
            .map_err(error)?
            .describe(id)
            .map_err(error)?
            .to_string())
    }
    fn metadata(&self) -> PyResult<String> {
        nbt::to_snbt(&lock(&self.data)?.metadata).map_err(error)
    }
    fn set_metadata(&self, snbt: &str) -> PyResult<()> {
        let data = nbt::from_snbt(snbt).map_err(error)?;
        lock(&self.data)?.metadata = data;
        Ok(())
    }
}
#[pyclass(name = "Region")]
struct PyRegion {
    data: Shared,
    name: String,
}
impl PyRegion {
    fn with<T>(&self, f: impl FnOnce(&mut Region) -> mcschemora::Result<T>) -> PyResult<T> {
        let mut d = lock(&self.data)?;
        f(d.region_mut(&self.name).map_err(error)?).map_err(error)
    }
}
#[pymethods]
impl PyRegion {
    fn bounds(&self) -> PyResult<(Position, Position)> {
        self.with(|r| Ok((r.bounds.start, r.bounds.size)))
    }
    fn origin(&self) -> PyResult<Position> {
        self.with(|r| Ok(r.origin))
    }
    fn to_global(&self, local: Position) -> PyResult<Position> {
        self.with(|r| r.to_global(local))
    }
    fn to_local(&self, global_position: Position) -> PyResult<Position> {
        self.with(|r| r.to_local(global_position))
    }
    fn get(&self, at: Position) -> PyResult<State> {
        self.with(|r| Ok(state(r.get(at))))
    }
    fn get_all<'py>(
        &self,
        py: Python<'py>,
        factory: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        block_snapshot(py, r.blocks.iter(), factory)
    }
    fn set_many(&self, palette: Vec<State>, cells: Vec<(Position, usize)>) -> PyResult<()> {
        let palette = palette
            .into_iter()
            .map(|(id, props)| Block::new(&id, props))
            .collect::<mcschemora::Result<Vec<_>>>()
            .map_err(error)?;
        lock(&self.data)?
            .set_indexed_blocks(&self.name, &palette, cells)
            .map_err(error)
    }

    fn select(&self, start: Position, size: Position) -> PyResult<PySelection> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        Ok(PySelection {
            data: self.data.clone(),
            name: self.name.clone(),
            selection: Selection::new(r, Bounds::new(start, size).map_err(error)?),
        })
    }
    fn set_fragment(&self, at: Position, fragment: &PyFragment) -> PyResult<()> {
        lock(&self.data)?
            .paste(&self.name, at, &fragment.fragment)
            .map_err(error)
    }
    fn place(&self, placement: &PyPlacement, at: Position, replace: bool) -> PyResult<()> {
        lock(&self.data)?
            .place(&self.name, &placement.placement, at, replace)
            .map_err(error)
    }

    fn block_entity_get(&self, at: Position) -> PyResult<Option<String>> {
        self.with(|r| r.block_entities.get(&at).map(nbt::to_snbt).transpose())
    }
    fn block_entity_set(&self, at: Position, snbt: &str) -> PyResult<()> {
        self.with(|r| r.set_block_entity(at, nbt::from_snbt(snbt)?))
    }
    fn block_entity_remove(&self, at: Position) -> PyResult<()> {
        self.with(|r| {
            r.block_entities.remove(&at);
            Ok(())
        })
    }
    fn entity_add(
        &self,
        id: &str,
        persistent: bool,
        snbt: Option<&str>,
        at: [f64; 3],
    ) -> PyResult<u64> {
        let mut d = lock(&self.data)?;
        let data =
            helpers::mob(d.registry().map_err(error)?, id, persistent, snbt).map_err(error)?;
        d.add_entity(&self.name, at, data).map_err(error)
    }

    fn entity_list(&self) -> PyResult<Vec<u64>> {
        self.with(|r| Ok(r.entities.iter().map(|e| e.reference).collect()))
    }
    fn entity_get(&self, reference: u64) -> PyResult<([f64; 3], String)> {
        self.with(|r| {
            let e = r.entity(reference)?;
            Ok((e.position, nbt::to_snbt(&e.data)?))
        })
    }
    fn entity_update(
        &self,
        reference: u64,
        position: Option<[f64; 3]>,
        snbt: Option<&str>,
    ) -> PyResult<()> {
        self.with(|r| r.update_entity(reference, position, snbt.map(nbt::from_snbt).transpose()?))
    }
    fn entity_remove(&self, reference: u64) -> PyResult<()> {
        self.with(|r| r.remove_entity(reference))
    }
}
#[pyclass(name = "Fragment")]
struct PyFragment {
    fragment: Fragment,
}
#[pymethods]
impl PyFragment {
    fn size(&self) -> Position {
        self.fragment.size
    }
}
#[pyclass(name = "Selection")]
struct PySelection {
    data: Shared,
    name: String,
    selection: Selection,
}
impl PySelection {
    fn apply(&self, t: Transform, duplicate: bool, replace: bool) -> PyResult<Selection> {
        transform_selection(
            &mut *lock(&self.data)?,
            &self.name,
            &self.selection,
            t,
            duplicate,
            replace,
        )
        .map_err(error)
    }
}
#[pymethods]
impl PySelection {
    fn bounds(&self) -> (Position, Position) {
        (self.selection.bounds.start, self.selection.bounds.size)
    }
    fn get_all<'py>(
        &self,
        py: Python<'py>,
        factory: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        if self.selection.cells.is_some()
            || self.selection.bounds.volume().map_err(error)? < r.blocks.len()
        {
            let positions = self.selection.positions();
            block_snapshot(
                py,
                positions
                    .iter()
                    .filter_map(|position| r.blocks.get(position).map(|block| (position, block))),
                factory,
            )
        } else {
            block_snapshot(
                py,
                r.blocks
                    .iter()
                    .filter(|(position, _)| self.selection.contains(**position)),
                factory,
            )
        }
    }
    fn select(&self, id: Option<&str>, properties: BTreeMap<String, String>) -> PyResult<Self> {
        let d = lock(&self.data)?;
        Ok(Self {
            data: self.data.clone(),
            name: self.name.clone(),
            selection: self
                .selection
                .filter(d.region(&self.name).map_err(error)?, id, &properties)
                .map_err(error)?,
        })
    }
    fn fill(&self, id: &str, properties: BTreeMap<String, String>) -> PyResult<()> {
        let b = Block::new(id, properties).map_err(error)?;
        lock(&self.data)?
            .fill(&self.name, &self.selection, &b)
            .map_err(error)
    }
    fn patch(&self, properties: BTreeMap<String, String>) -> PyResult<()> {
        lock(&self.data)?
            .patch(&self.name, self.selection.positions(), &properties)
            .map_err(error)
    }
    fn delete(&self) -> PyResult<()> {
        lock(&self.data)?
            .delete(&self.name, &self.selection)
            .map_err(error)
    }

    fn move_by(&mut self, offset: Position, replace: bool) -> PyResult<()> {
        self.selection = self.apply(Transform::move_by(offset), false, replace)?;
        Ok(())
    }
    fn rotate(
        &mut self,
        axis: &str,
        steps: i32,
        pivot: Option<[f64; 3]>,
        replace: bool,
    ) -> PyResult<()> {
        let t = Transform::rotate(axis, steps, pivot.unwrap_or(self.selection.bounds.center()))
            .map_err(error)?;
        self.selection = self.apply(t, false, replace)?;
        Ok(())
    }
    fn flip(&mut self, axis: &str, center: Option<f64>, replace: bool) -> PyResult<()> {
        let a = match axis {
            "x" => 0,
            "z" => 2,
            _ => return Err(error("Flip axis must be x or z")),
        };
        let t = Transform::flip(axis, center.unwrap_or(self.selection.bounds.center()[a]))
            .map_err(error)?;
        self.selection = self.apply(t, false, replace)?;
        Ok(())
    }
    fn duplicate(&self, offset: Position, replace: bool) -> PyResult<Self> {
        Ok(Self {
            data: self.data.clone(),
            name: self.name.clone(),
            selection: self.apply(Transform::move_by(offset), true, replace)?,
        })
    }
    fn copy(&self) -> PyResult<PyFragment> {
        let d = lock(&self.data)?;
        Ok(PyFragment {
            fragment: self
                .selection
                .snapshot(d.region(&self.name).map_err(error)?, &d.edition, &d.version)
                .map_err(error)?,
        })
    }
    fn counts(&self) -> PyResult<BTreeMap<String, usize>> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        let mut counts = BTreeMap::new();
        for p in self.selection.positions() {
            let b = r.get(p);
            if !b.is_air() {
                *counts.entry(b.text()).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }
    fn layer(&self, y: i32) -> PyResult<Vec<(Position, String)>> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        if self.selection.bounds.size[0] as i64 * self.selection.bounds.size[2] as i64 > 65536 {
            return Err(error(
                "Layer inspection is limited to 65,536 cells; select a smaller area",
            ));
        }
        let bounds = self.selection.bounds;
        if y < bounds.start[1] || y >= bounds.start[1] + bounds.size[1] {
            return Ok(vec![]);
        }
        let layer = Bounds::new(
            [bounds.start[0], y, bounds.start[2]],
            [bounds.size[0], 1, bounds.size[2]],
        )
        .map_err(error)?;
        Ok(layer
            .positions()
            .filter(|p| self.selection.contains(*p))
            .map(|p| (p, r.get(p).text()))
            .collect())
    }
}
/// A placement committed with Region.place; validated against the target version.
#[pyclass(name = "Placement", frozen)]
struct PyPlacement {
    placement: helpers::Placement,
}

/// Creates a bed placement anchored at the foot block when placed.
///
/// Args:
///     color: Minecraft bed color.
///     head_toward: Direction from the foot to the head: north, south, east, or west.
///
/// Returns:
///     A two-block placement for Region.place. Validated when placed.
#[pyfunction]
#[pyo3(signature = (*, color="red", head_toward="north"))]
fn bed(color: &str, head_toward: &str) -> PyPlacement {
    PyPlacement {
        placement: helpers::Placement::Bed {
            color: color.into(),
            head_toward: head_toward.into(),
        },
    }
}
/// Creates a door placement anchored at the lower block when placed.
///
/// Args:
///     material: Minecraft door material, such as oak or iron.
///     facing: Horizontal facing direction.
///     hinge: Hinge side, left or right.
///     open: Whether the door is open.
///     powered: Whether the door is powered.
///
/// Returns:
///     A two-block placement for Region.place. Validated when placed.
#[pyfunction]
#[pyo3(signature = (*, material="oak", facing="north", hinge="left", open=false, powered=false))]
fn door(material: &str, facing: &str, hinge: &str, open: bool, powered: bool) -> PyPlacement {
    PyPlacement {
        placement: helpers::Placement::Door {
            material: material.into(),
            facing: facing.into(),
            hinge: hinge.into(),
            open,
            powered,
        },
    }
}
/// Creates a single-chest placement with optional inventory contents.
///
/// Args:
///     facing: Horizontal facing direction.
///     items: Slots 0 through 26 mapped to item() values; None leaves it empty.
///
/// Returns:
///     A placement for Region.place. Items and states are validated when placed.
#[pyfunction]
#[pyo3(signature = (*, facing="north", items=None))]
fn chest(facing: &str, items: Option<BTreeMap<i8, helpers::Item>>) -> PyPlacement {
    PyPlacement {
        placement: helpers::Placement::Chest {
            facing: facing.into(),
            items: items.unwrap_or_default(),
        },
    }
}
/// Creates a standing-sign placement with plain text on its front face.
///
/// Args:
///     lines: Up to four text lines; omitted lines are blank.
///     material: Minecraft sign material, such as oak.
///     rotation: Minecraft rotation value, 0 through 15.
///     color: Minecraft text color.
///
/// Returns:
///     A placement for Region.place, with text encoded for the target version.
#[pyfunction]
#[pyo3(signature = (lines, *, material="oak", rotation=0, color="black"))]
fn sign(lines: Vec<String>, material: &str, rotation: i32, color: &str) -> PyPlacement {
    PyPlacement {
        placement: helpers::Placement::Sign {
            lines,
            material: material.into(),
            rotation,
            color: color.into(),
        },
    }
}
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PySchematic>()?;
    m.add_class::<PyRegion>()?;
    m.add_class::<PySelection>()?;
    m.add_class::<PyFragment>()?;
    m.add_class::<PyMinecraftData>()?;
    m.add_class::<PyPlacement>()?;
    m.add_function(wrap_pyfunction!(bed, m)?)?;
    m.add_function(wrap_pyfunction!(door, m)?)?;
    m.add_function(wrap_pyfunction!(chest, m)?)?;
    m.add_function(wrap_pyfunction!(sign, m)?)?;
    Ok(())
}
