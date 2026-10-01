mod visual_atlas;
mod visual_models;
mod visuals;

use crate::Result;
use fs2::FileExt;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};

// https://github.com/PrismarineJS/minecraft-data/tree/8ffb321c74cffe779acf5c447d08c473c4c291d7
pub const REVISION: &str = "8ffb321c74cffe779acf5c447d08c473c4c291d7";
const CATALOG_BASE_URL: &str = "https://raw.githubusercontent.com/PrismarineJS/minecraft-data";
const JSON_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Cache {
    pub root: PathBuf,
    pub offline: bool,
    agent: ureq::Agent,
}

pub(crate) fn safe_path(value: &str) -> Result<&Path> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(format!("Invalid data path {value:?}"));
    }
    Ok(path)
}

fn default_root() -> Result<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|p| Path::new(p).is_absolute())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")))
    };
    base.map(|p| p.join("schemora"))
        .ok_or_else(|| "Cannot determine a cache directory; provide cache_dir".into())
}

impl Cache {
    pub fn new(root: Option<PathBuf>, offline: bool) -> Result<Self> {
        Ok(Self {
            root: root.map(Ok).unwrap_or_else(default_root)?,
            offline,
            agent: ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .redirects(5)
                .build(),
        })
    }

    pub fn catalog_path(&self, relative: &str) -> Result<PathBuf> {
        Ok(self
            .root
            .join("minecraft-data")
            .join(REVISION)
            .join(safe_path(relative)?))
    }

    pub fn catalog(&self, relative: &str) -> Result<PathBuf> {
        let path = self.catalog_path(relative)?;
        let url = format!("{CATALOG_BASE_URL}/{REVISION}/data/{relative}");
        self.file(&path, &url, None, None, JSON_LIMIT, true)?;
        Ok(path)
    }

    pub fn json(&self, path: &Path) -> Result<Value> {
        let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn lock(&self, path: &Path) -> Result<File> {
        fs::create_dir_all(path.parent().ok_or("Cache path has no parent")?)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        file.lock_exclusive()
            .map_err(|e| format!("Cache lock: {e}"))?;
        Ok(file)
    }

    pub fn file(
        &self,
        path: &Path,
        url: &str,
        hash: Option<&str>,
        size: Option<u64>,
        limit: u64,
        json: bool,
    ) -> Result<()> {
        let valid = |bytes: &[u8]| -> bool {
            bytes.len() as u64 <= limit
                && size.is_none_or(|n| n == bytes.len() as u64)
                && hash.is_none_or(|h| format!("{:x}", Sha1::digest(bytes)) == h)
                && (!json || serde_json::from_slice::<Value>(bytes).is_ok())
        };
        let existing = || -> bool {
            fs::metadata(path).is_ok_and(|m| m.len() <= limit)
                && fs::read(path).is_ok_and(|b| valid(&b))
        };
        if existing() {
            return Ok(());
        }
        if self.offline {
            return Err(format!(
                "Offline cache miss or invalid cached file: {}. Fetch it with offline=False first.",
                path.display()
            ));
        }
        let _lock = self.lock(&path.with_extension("lock"))?;
        if existing() {
            return Ok(());
        }
        self.download(path, url, limit, valid)
    }

    fn download(
        &self,
        path: &Path,
        url: &str,
        limit: u64,
        valid: impl Fn(&[u8]) -> bool,
    ) -> Result<()> {
        let mut failure = String::new();
        for attempt in 0..3 {
            let result = (|| -> Result<()> {
                let response = self
                    .agent
                    .get(url)
                    .set("User-Agent", "schemora/0.1")
                    .call()
                    .map_err(|e| format!("Download {url}: {e}"))?;
                let mut bytes = Vec::new();
                response
                    .into_reader()
                    .take(limit + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| format!("Download {url}: {e}"))?;
                if bytes.len() as u64 > limit || !valid(&bytes) {
                    return Err(format!("Invalid or oversized download: {url}"));
                }
                let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())
                    .map_err(|e| e.to_string())?;
                temp.write_all(&bytes).map_err(|e| e.to_string())?;
                temp.as_file().sync_all().map_err(|e| e.to_string())?;
                temp.persist(path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(())
            })();
            match result {
                Ok(()) => return Ok(()),
                Err(e) => failure = e,
            }
            if attempt < 2 {
                std::thread::sleep(Duration::from_millis(250 * (attempt + 1)));
            }
        }
        Err(failure)
    }
}
