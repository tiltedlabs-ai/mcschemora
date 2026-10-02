//! PyO3 adapter. Core data and editing rules stay in Rust.
use pyo3::{exceptions::PyValueError, prelude::*, types::PyBytes};
use schemora::{
    formats, helpers,
    model::*,
    nbt, registry,
    transform::{Transform, transform_selection},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};
type Shared = Arc<Mutex<Document>>;
type State = (String, BTreeMap<String, String>);
type Cell = (Pos, String, BTreeMap<String, String>);
fn error(e: impl ToString) -> PyErr {
    PyValueError::new_err(e.to_string())
}
fn lock(d: &Shared) -> PyResult<MutexGuard<'_, Document>> {
    d.lock().map_err(|_| error("Document lock poisoned"))
}
fn state(b: Block) -> State {
    (b.name, b.properties)
}
fn render(
    document: &Document,
    options: schemora::render::SceneOptions,
    encode: impl FnOnce(&schemora::render::PreparedScene) -> schemora::Result<Vec<u8>>,
) -> PyResult<(Vec<u8>, Vec<String>)> {
    let assets = document
        .data
        .geometry_assets(&document.version)
        .map_err(error)?;
    let scene = assets.prepare(document, &options).map_err(error)?;
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
    data: Arc<registry::MinecraftData>,
}
#[pymethods]
impl PyMinecraftData {
    #[new]
    #[pyo3(signature = (cache_dir=None, offline=false))]
    fn new(cache_dir: Option<std::path::PathBuf>, offline: bool) -> PyResult<Self> {
        Ok(Self {
            data: Arc::new(registry::MinecraftData::new(cache_dir, offline).map_err(error)?),
        })
    }
    fn cache_dir(&self) -> std::path::PathBuf {
        self.data.cache_dir().to_path_buf()
    }
    fn versions(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        py.detach(|| self.data.versions()).map_err(error)
    }
    fn fetch(&self, py: Python<'_>, version: &str, visuals: bool) -> PyResult<String> {
        py.detach(|| self.data.fetch(version, visuals))
            .map_err(error)
    }
    fn dataset_path(
        &self,
        py: Python<'_>,
        version: &str,
        kind: &str,
    ) -> PyResult<std::path::PathBuf> {
        py.detach(|| self.data.dataset_path(version, kind))
            .map_err(error)
    }
    fn visuals(&self, py: Python<'_>, version: &str) -> PyResult<std::path::PathBuf> {
        py.detach(|| self.data.visuals(version)).map_err(error)
    }
}

#[pyclass(name = "Document")]
struct PyDocument {
    data: Shared,
}
#[pymethods]
impl PyDocument {
    #[new]
    fn new(
        py: Python<'_>,
        edition: &str,
        version: &str,
        source: &PyMinecraftData,
    ) -> PyResult<Self> {
        Ok(Self {
            data: Arc::new(Mutex::new(
                py.detach(|| Document::new(edition, version, source.data.clone()))
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
        origin: Option<Pos>,
        palette: std::collections::BTreeMap<String, String>,
    ) -> PyResult<Self> {
        let options = formats::blueprint::import::Options {
            version,
            origin,
            palette,
        };
        Ok(Self {
            data: Arc::new(Mutex::new(
                py.detach(|| formats::decode(data, format, source.data.clone(), &options))
                    .map_err(error)?,
            )),
        })
    }
    fn to_bytes<'py>(
        &self,
        py: Python<'py>,
        format: &str,
        allow_loss: bool,
        flatten: bool,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let data =
            formats::encode(&*lock(&self.data)?, format, allow_loss, flatten).map_err(error)?;
        Ok(PyBytes::new(py, &data))
    }
    fn check_export(&self, format: &str, flatten: bool) -> PyResult<(Vec<String>, Vec<String>)> {
        let report = formats::check_export(&*lock(&self.data)?, format, flatten).map_err(error)?;
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
                schemora::render::SceneOptions { region, x, y, z },
                schemora::render::glb::encode,
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
        use schemora::render::View;
        let view = match view {
            "isometric" => View::Isometric,
            "top" => View::Top,
            "bottom" => View::Bottom,
            "north" => View::North,
            "south" => View::South,
            "east" => View::East,
            "west" => View::West,
            _ => {
                return Err(error(
                    "view must be isometric, top, bottom, north, south, east, or west",
                ));
            }
        };
        let [x, y, z] = ranges;
        let (bytes, diagnostics) = py.detach(|| {
            render(
                &*lock(&self.data)?,
                schemora::render::SceneOptions { region, x, y, z },
                |scene| {
                    schemora::render::png::encode(
                        scene,
                        &schemora::render::png::Options { size, view, grid },
                    )
                },
            )
        })?;
        Ok((PyBytes::new(py, &bytes), diagnostics))
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
    fn region(&self, name: &str) -> PyResult<PyRegion> {
        lock(&self.data)?.region(name).map_err(error)?;
        Ok(PyRegion {
            data: self.data.clone(),
            name: name.into(),
        })
    }
    fn add_region(&self, name: &str, origin: Pos) -> PyResult<PyRegion> {
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
    fn with<T>(&self, f: impl FnOnce(&mut Region) -> schemora::Result<T>) -> PyResult<T> {
        let mut d = lock(&self.data)?;
        f(d.region_mut(&self.name).map_err(error)?).map_err(error)
    }
}
#[pymethods]
impl PyRegion {
    fn bounds(&self) -> PyResult<(Pos, Pos)> {
        self.with(|r| Ok((r.bounds.start, r.bounds.size)))
    }
    fn origin(&self) -> PyResult<Pos> {
        self.with(|r| Ok(r.origin))
    }
    fn get(&self, at: Pos) -> PyResult<State> {
        self.with(|r| Ok(state(r.get(at))))
    }
    fn set_many(&self, cells: Vec<Cell>) -> PyResult<()> {
        let blocks = cells
            .into_iter()
            .map(|(p, id, props)| Block::new(&id, props).map(|b| (p, b)))
            .collect::<schemora::Result<Vec<_>>>()
            .map_err(error)?;
        lock(&self.data)?
            .set_blocks(&self.name, blocks)
            .map_err(error)
    }

    fn select(&self, start: Pos, size: Pos) -> PyResult<PySelection> {
        let d = lock(&self.data)?;
        let r = d.region(&self.name).map_err(error)?;
        Ok(PySelection {
            data: self.data.clone(),
            name: self.name.clone(),
            selection: Selection::new(r, Bounds::new(start, size).map_err(error)?),
        })
    }
    fn set_fragment(&self, at: Pos, fragment: &PyFragment) -> PyResult<()> {
        lock(&self.data)?
            .paste(&self.name, at, &fragment.fragment)
            .map_err(error)
    }
    fn place(&self, placement: &PyPlacement, at: Pos, replace: bool) -> PyResult<()> {
        lock(&self.data)?
            .place(&self.name, &placement.recipe, at, replace)
            .map_err(error)
    }

    fn block_entity_get(&self, at: Pos) -> PyResult<Option<String>> {
        self.with(|r| r.block_entities.get(&at).map(nbt::to_snbt).transpose())
    }
    fn block_entity_set(&self, at: Pos, snbt: &str) -> PyResult<()> {
        self.with(|r| r.set_block_entity(at, nbt::from_snbt(snbt)?))
    }
    fn block_entity_remove(&self, at: Pos) -> PyResult<()> {
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
    fn size(&self) -> Pos {
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
    fn bounds(&self) -> (Pos, Pos) {
        (self.selection.bounds.start, self.selection.bounds.size)
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

    fn move_by(&mut self, offset: Pos, replace: bool) -> PyResult<()> {
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
    fn duplicate(&self, offset: Pos, replace: bool) -> PyResult<Self> {
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
    fn layer(&self, y: i32) -> PyResult<Vec<(Pos, String)>> {
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
#[pyclass(name = "Placement", frozen)]
struct PyPlacement {
    recipe: helpers::Recipe,
}

/// Place both bed halves; at is the foot block.
#[pyfunction]
#[pyo3(signature = (*, color="red", head_toward="north"))]
fn bed(color: &str, head_toward: &str) -> PyPlacement {
    PyPlacement {
        recipe: helpers::Recipe::Bed {
            color: color.into(),
            head_toward: head_toward.into(),
        },
    }
}
/// Place both door halves; at is the lower block.
#[pyfunction]
#[pyo3(signature = (*, material="oak", facing="north", hinge="left", open=false, powered=false))]
fn door(material: &str, facing: &str, hinge: &str, open: bool, powered: bool) -> PyPlacement {
    PyPlacement {
        recipe: helpers::Recipe::Door {
            material: material.into(),
            facing: facing.into(),
            hinge: hinge.into(),
            open,
            powered,
        },
    }
}
#[pyfunction]
#[pyo3(signature = (*, facing="north", items=None))]
fn chest(facing: &str, items: Option<BTreeMap<i8, helpers::Item>>) -> PyPlacement {
    PyPlacement {
        recipe: helpers::Recipe::Chest {
            facing: facing.into(),
            items: items.unwrap_or_default(),
        },
    }
}
#[pyfunction]
#[pyo3(signature = (lines, *, material="oak", rotation=0, color="black"))]
fn sign(lines: Vec<String>, material: &str, rotation: i32, color: &str) -> PyPlacement {
    PyPlacement {
        recipe: helpers::Recipe::Sign {
            lines,
            material: material.into(),
            rotation,
            color: color.into(),
        },
    }
}
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDocument>()?;
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
