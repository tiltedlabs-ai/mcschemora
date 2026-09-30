use super::{Cache, JSON_LIMIT, safe_path};
use crate::Result;
use serde_json::Value;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};

const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const ASSET_LIMIT: u64 = 256 * 1024 * 1024;

fn url(value: &Value) -> Result<&str> {
    let url = value.as_str().ok_or("Missing Mojang download URL")?;
    if [
        "piston-meta.mojang.com",
        "piston-data.mojang.com",
        "launchermeta.mojang.com",
        "launcher.mojang.com",
    ]
    .iter()
    .any(|host| url.starts_with(&format!("https://{host}/")))
    {
        Ok(url)
    } else {
        Err(format!("Invalid Mojang download URL {url:?}"))
    }
}

fn hash(value: &Value) -> Result<&str> {
    let hash = value.as_str().ok_or("Missing Mojang SHA-1")?;
    if hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(hash)
    } else {
        Err("Invalid Mojang SHA-1".into())
    }
}

impl Cache {
    pub fn visuals(&self, version: &str) -> Result<PathBuf> {
        safe_path(version)?;
        let base = self.root.join("mojang");
        let manifest_path = base.join("version_manifest_v2.json");
        self.file(&manifest_path, MANIFEST, None, None, JSON_LIMIT, true)?;
        let mut manifest = self.json(&manifest_path)?;
        let has_version = |value: &Value| {
            value["versions"]
                .as_array()
                .is_some_and(|versions| versions.iter().any(|v| v["id"] == version))
        };
        if !has_version(&manifest) && !self.offline {
            self.refresh_json(&manifest_path, MANIFEST)?;
            manifest = self.json(&manifest_path)?;
        }
        let entry = manifest["versions"]
            .as_array()
            .ok_or("Invalid Mojang version manifest")?
            .iter()
            .find(|v| v["id"] == version)
            .ok_or_else(|| format!("No official visual assets for Java {version}"))?;
        let version_path = base.join(version).join("version.json");
        self.file(
            &version_path,
            url(&entry["url"])?,
            Some(hash(&entry["sha1"])?),
            None,
            JSON_LIMIT,
            true,
        )?;
        let metadata = self.json(&version_path)?;
        let client = &metadata["downloads"]["client"];
        let client_hash = hash(&client["sha1"])?;
        let root = base.join(version).join(client_hash);
        let assets = root.join("assets");
        let complete = root.join("complete");
        if fs::read_to_string(&complete).is_ok_and(|v| v == client_hash) && assets.is_dir() {
            return Ok(assets);
        }
        let jar = root.with_extension("jar");
        let size = client["size"]
            .as_u64()
            .ok_or("Missing Mojang client size")?;
        self.file(
            &jar,
            url(&client["url"])?,
            Some(client_hash),
            Some(size),
            ASSET_LIMIT,
            false,
        )?;
        let _lock = self.lock(&root.with_extension("extract.lock"))?;
        if fs::read_to_string(&complete).is_ok_and(|v| v == client_hash) && assets.is_dir() {
            return Ok(assets);
        }
        let temp = tempfile::tempdir_in(root.parent().unwrap()).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(fs::File::open(&jar).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Invalid client archive: {e}"))?;
        let mut expanded = 0u64;
        let mut count = 0;
        for i in 0..archive.len() {
            let file = archive.by_index(i).map_err(|e| e.to_string())?;
            if !file.name().starts_with("assets/") {
                continue;
            }
            let relative = file.enclosed_name().ok_or("Unsafe asset archive path")?;
            if file.is_dir() {
                continue;
            }
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return Err("Asset archive contains a symbolic link".into());
            }
            expanded = expanded
                .checked_add(file.size())
                .ok_or("Asset archive exceeds limit")?;
            if expanded > ASSET_LIMIT {
                return Err("Asset archive exceeds 256 MiB".into());
            }
            let output = temp.path().join(relative);
            fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
            let size = file.size();
            let written = io::copy(
                &mut file.take(size + 1),
                &mut fs::File::create(output).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            if written != size {
                return Err("Truncated asset archive entry".into());
            }
            count += 1;
        }
        if count == 0 {
            return Err("Client archive contains no visual assets".into());
        }
        fs::write(temp.path().join("complete"), client_hash).map_err(|e| e.to_string())?;
        if root.exists() {
            fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        }
        fs::rename(temp.path(), &root).map_err(|e| e.to_string())?;
        Ok(assets)
    }
}
