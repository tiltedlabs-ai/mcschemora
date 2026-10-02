mod registry;
pub(crate) mod source;
mod storage;

use crate::Result;
pub use registry::{Registry, namespace};
use serde_json::Value;
pub(crate) use source::MIN_DATA_VERSION;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

#[cfg(not(target_arch = "wasm32"))]
pub const VISUAL_VERSION: &str = "1.21.1";

#[derive(Debug)]
struct Loaded {
    registry: Arc<Registry>,
    datasets: BTreeMap<String, Value>,
}

#[derive(Debug)]
pub struct MinecraftData {
    cache: storage::Cache,
    metadata: OnceLock<source::Metadata>,
    legacy: OnceLock<BTreeMap<String, String>>,
    catalogs: Mutex<BTreeMap<String, Loaded>>,
    #[cfg(not(target_arch = "wasm32"))]
    geometry: Mutex<Option<Arc<crate::render::GeometryAssets>>>,
}

impl MinecraftData {
    pub fn new(cache_dir: Option<PathBuf>, offline: bool) -> Result<Self> {
        Ok(Self {
            cache: storage::Cache::new(cache_dir, offline)?,
            metadata: OnceLock::new(),
            legacy: OnceLock::new(),
            catalogs: Mutex::new(BTreeMap::new()),
            #[cfg(not(target_arch = "wasm32"))]
            geometry: Mutex::new(None),
        })
    }

    async fn read(&self, relative: &str) -> Result<Value> {
        source::json(relative, &self.cache.read(relative).await?)
    }

    pub async fn initialize(&self) -> Result<()> {
        if self.metadata.get().is_none() {
            let paths = self.read("dataPaths.json").await?;
            let versions = self.read("pc/common/protocolVersions.json").await?;
            let _ = self.metadata.set(source::Metadata::parse(paths, versions)?);
        }
        Ok(())
    }

    fn metadata(&self) -> Result<&source::Metadata> {
        self.metadata.get().ok_or_else(|| {
            "Catalog metadata is not loaded; call initialize() or load() first".into()
        })
    }

    pub fn versions(&self) -> Result<Vec<String>> {
        Ok(self.metadata()?.versions())
    }

    pub fn version_for_data_version(&self, id: i32) -> Result<Option<String>> {
        Ok(self.metadata()?.version_for_data_version(id))
    }

    pub async fn load(&self, requested: &str) -> Result<Arc<Registry>> {
        self.initialize().await?;
        let metadata = self.metadata()?;
        let version = metadata.resolve(requested)?;
        let data_version = metadata.data_version(&version)?;
        if let Some(loaded) = self
            .catalogs
            .lock()
            .map_err(|_| "Catalog lock poisoned")?
            .get(&version)
        {
            return Ok(loaded.registry.clone());
        }
        if self.legacy.get().is_none() {
            let value = self.read("pc/common/legacy.json").await?;
            let legacy = serde_json::from_value(value["blocks"].clone())
                .map_err(|e| format!("Invalid legacy mappings: {e}"))?;
            let _ = self.legacy.set(legacy);
        }
        let mut datasets = BTreeMap::new();
        for kind in source::KINDS {
            datasets.insert(
                kind.into(),
                self.read(&metadata.dataset(&version, kind)?).await?,
            );
        }
        let registry = Arc::new(Registry::parse(
            version.clone(),
            data_version,
            datasets["blocks"].clone(),
            datasets["items"].clone(),
            datasets["entities"].clone(),
            self.legacy()?,
        )?);
        self.catalogs
            .lock()
            .map_err(|_| "Catalog lock poisoned")?
            .insert(
                version,
                Loaded {
                    registry: registry.clone(),
                    datasets,
                },
            );
        Ok(registry)
    }

    pub fn registry(&self, requested: &str) -> Result<Arc<Registry>> {
        let version = self.metadata()?.resolve(requested)?;
        self.catalogs
            .lock()
            .map_err(|_| "Catalog lock poisoned")?
            .get(&version)
            .map(|v| v.registry.clone())
            .ok_or_else(|| format!("Java {version} catalog is not loaded; call load() first"))
    }

    pub fn dataset(&self, requested: &str, kind: &str) -> Result<Value> {
        let version = self.metadata()?.resolve(requested)?;
        self.catalogs
            .lock()
            .map_err(|_| "Catalog lock poisoned")?
            .get(&version)
            .and_then(|v| v.datasets.get(kind))
            .cloned()
            .ok_or_else(|| format!("Dataset {kind} for Java {version} is not loaded"))
    }

    pub(crate) fn collision_shapes(&self, version: &str) -> Result<Value> {
        self.dataset(version, "blockCollisionShapes")
    }

    pub(crate) fn legacy(&self) -> Result<&BTreeMap<String, String>> {
        self.legacy
            .get()
            .ok_or_else(|| "Legacy mappings are not loaded".into())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn cache_dir(&self) -> &std::path::Path {
        &self.cache.root
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_visuals(&self, requested: &str) -> Result<PathBuf> {
        let version = self.metadata()?.resolve(requested)?;
        if version != VISUAL_VERSION {
            eprintln!(
                "warning: visuals for Java {version} are not supported; falling back to Java {VISUAL_VERSION} textures and models"
            );
        }
        self.cache.load_visuals(VISUAL_VERSION, Some("1.21"))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn geometry_assets(&self, requested: &str) -> Result<Arc<crate::render::GeometryAssets>> {
        let path = self.load_visuals(requested)?;
        let mut assets = self
            .geometry
            .lock()
            .map_err(|_| "Geometry cache lock poisoned")?;
        if assets.is_none() {
            *assets = Some(Arc::new(crate::render::GeometryAssets::load(&path)?));
        }
        Ok(assets.as_ref().unwrap().clone())
    }
}
