use super::{Cache, JSON_LIMIT, VISUAL_FORMAT, safe_path, visual_atlas, visual_models};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

// https://github.com/PrismarineJS/minecraft-assets/tree/67c9b138b00a6b67c29ba68dae74c41faef4889d
const REVISION: &str = "67c9b138b00a6b67c29ba68dae74c41faef4889d";
const API_BASE_URL: &str = "https://api.github.com/repos/PrismarineJS/minecraft-assets";
const RAW_BASE_URL: &str = "https://raw.githubusercontent.com/PrismarineJS/minecraft-assets";
const FILE_LIMIT: u64 = 8 * 1024 * 1024;
const BUNDLE_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub path: String,
    pub hash: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize)]
struct Source {
    bundle: String,
    files: Vec<Input>,
}

type Index = BTreeMap<String, Source>;

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing output parent")?)
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut temp, value).map_err(|e| e.to_string())?;
    temp.write_all(b"\n").map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn release(version: &str) -> Option<Vec<u32>> {
    let parts: Option<Vec<u32>> = version.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| p.len() >= 2 && (p[0] != 1 || p[1] >= 13))
}

fn selected(path: &str) -> bool {
    path == "blocks_models.json"
        || path == "blocks_states.json"
        || ((path.starts_with("blocks/")
            || path.starts_with("entity/")
            || path.starts_with("colormap/"))
            && (path.ends_with(".png") || path.ends_with(".png.mcmeta")))
        || path.strip_prefix("items/").is_some_and(|p| {
            let name = p.trim_end_matches(".mcmeta").trim_end_matches(".png");
            (p.ends_with(".png") || p.ends_with(".png.mcmeta"))
                && (name == "barrier"
                    || name == "structure_void"
                    || name
                        .strip_prefix("light_")
                        .is_some_and(|n| n.len() == 2 && n.parse::<u8>().is_ok_and(|n| n <= 15)))
        })
}

fn blob_hash(bytes: &[u8]) -> String {
    let mut hash = Sha1::new();
    hash.update(format!("blob {}\0", bytes.len()));
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

fn complete(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path.join("manifest.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    manifest["preparation_format"] == VISUAL_FORMAT
        && ["models.json", "blockstates.json", "textures.json"]
            .iter()
            .all(|file| path.join(file).is_file())
        && manifest["atlases"].as_array().is_some_and(|atlases| {
            !atlases.is_empty()
                && atlases.iter().all(|atlas| {
                    atlas["file"]
                        .as_str()
                        .is_some_and(|file| safe_path(file).is_ok() && path.join(file).is_file())
                })
        })
}

impl Cache {
    fn prepared_visual_path(&self, bundle: &str) -> Result<PathBuf> {
        if bundle.len() != 40 || !bundle.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid visual bundle identity".into());
        }
        Ok(self
            .root
            .join("minecraft-assets")
            .join("prepared")
            .join(format!("v{VISUAL_FORMAT}"))
            .join(bundle))
    }
    fn visual_index(&self, base: &Path) -> Result<Index> {
        let path = base.join(format!("index-v{VISUAL_FORMAT}.json"));
        if path.is_file() {
            return serde_json::from_value(self.json(&path)?).map_err(|e| e.to_string());
        }
        let _lock = self.lock(&base.join("index.lock"))?;
        if path.is_file() {
            return serde_json::from_value(self.json(&path)?).map_err(|e| e.to_string());
        }
        let tree_path = base.join("tree.json");
        self.file(
            &tree_path,
            &format!("{API_BASE_URL}/git/trees/{REVISION}?recursive=1"),
            None,
            None,
            JSON_LIMIT,
            true,
        )?;
        let tree = self.json(&tree_path)?;
        if tree["truncated"].as_bool() != Some(false) {
            return Err("Pinned visual asset Git tree is truncated or invalid; refusing an incomplete bundle index".into());
        }
        let mut files: BTreeMap<String, Vec<Input>> = BTreeMap::new();
        for entry in tree["tree"]
            .as_array()
            .ok_or("Invalid visual asset Git tree")?
        {
            if entry["type"] != "blob" {
                continue;
            }
            let path = entry["path"].as_str().ok_or("Missing Git tree path")?;
            let Some((version, relative)) =
                path.strip_prefix("data/").and_then(|p| p.split_once('/'))
            else {
                continue;
            };
            if release(version).is_none() || !selected(relative) {
                continue;
            }
            safe_path(relative)?;
            let hash = entry["sha"].as_str().ok_or("Missing Git blob hash")?;
            if hash.len() != 40 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Invalid Git blob hash".into());
            }
            let size = entry["size"].as_u64().ok_or("Missing Git blob size")?;
            if size > FILE_LIMIT {
                return Err(format!("Visual input exceeds size limit: {path}"));
            }
            files.entry(version.into()).or_default().push(Input {
                path: relative.into(),
                hash: hash.into(),
                size,
            });
        }
        let mut index = Index::new();
        for (version, mut files) in files {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            if !["blocks_models.json", "blocks_states.json"]
                .iter()
                .all(|p| files.iter().any(|f| f.path == *p))
            {
                continue;
            }
            if files.iter().map(|f| f.size).sum::<u64>() > BUNDLE_LIMIT {
                return Err(format!("Visual bundle exceeds size limit: {version}"));
            }
            let mut hash = Sha1::new();
            for input in &files {
                hash.update(&input.path);
                hash.update([0]);
                hash.update(&input.hash);
                hash.update([0]);
            }
            index.insert(
                version,
                Source {
                    bundle: format!("{:x}", hash.finalize()),
                    files,
                },
            );
        }
        if index.is_empty() {
            return Err("Pinned asset source has no supported visual bundles".into());
        }
        write_json(&path, &index)?;
        Ok(index)
    }

    pub(super) fn visual_blob(&self, hash: &str) -> PathBuf {
        self.root.join("minecraft-assets").join("blobs").join(hash)
    }

    fn fetch_visual_input(&self, version: &str, input: &Input) -> Result<()> {
        let path = self.visual_blob(&input.hash);
        let valid =
            |bytes: &[u8]| bytes.len() as u64 == input.size && blob_hash(bytes) == input.hash;
        let exists = || {
            fs::metadata(&path).is_ok_and(|m| m.len() == input.size)
                && fs::read(&path).is_ok_and(|b| valid(&b))
        };
        if exists() {
            return Ok(());
        }
        if self.offline {
            return Err(format!("Offline visual cache miss: {}", input.path));
        }
        let _lock = self.lock(&path.with_extension("lock"))?;
        if exists() {
            return Ok(());
        }
        self.download(
            &path,
            &format!("{RAW_BASE_URL}/{REVISION}/data/{version}/{}", input.path),
            FILE_LIMIT,
            valid,
        )
    }

    pub fn visuals(&self, version: &str, family: Option<&str>) -> Result<PathBuf> {
        safe_path(version)?;
        let base = self.root.join("minecraft-assets").join(REVISION);
        let record_path = base.join("resolutions").join(format!("{version}.json"));
        if let Ok(record) = self.json(&record_path)
            && record["preparation_format"] == VISUAL_FORMAT
            && let Some(bundle) = record["bundle"].as_str()
        {
            let prepared = self.prepared_visual_path(bundle)?;
            if complete(&prepared) {
                return Ok(prepared);
            }
        }
        let index = self.visual_index(&base)?;
        let resolved = if index.contains_key(version) {
            version
        } else {
            let mut candidates: Vec<&str> = index.keys().map(String::as_str).collect();
            candidates.sort_by_key(|v| release(v).unwrap());
            let requested = release(version).or_else(|| family.and_then(release));
            let has_requested = requested.is_some();
            let family: Vec<&str> = requested
                .as_ref()
                .map(|r| {
                    candidates
                        .iter()
                        .copied()
                        .filter(|v| {
                            let p = release(v).unwrap();
                            p[0] == r[0] && p[1] == r[1]
                        })
                        .collect()
                })
                .unwrap_or_default();
            let choices = if family.is_empty() {
                &candidates
            } else {
                &family
            };
            requested
                .and_then(|r| {
                    choices
                        .iter()
                        .rev()
                        .find(|v| release(v).unwrap() <= r)
                        .copied()
                })
                .unwrap_or_else(|| {
                    if has_requested {
                        choices[0]
                    } else {
                        *choices.last().unwrap()
                    }
                })
        };
        let source = &index[resolved];
        let versions: Vec<&str> = index
            .iter()
            .filter(|(_, s)| s.bundle == source.bundle)
            .map(|(v, _)| v.as_str())
            .collect();
        let prepared = self.prepared_visual_path(&source.bundle)?;
        let _lock = self.lock(&prepared.with_extension("lock"))?;
        if !complete(&prepared) {
            let next = AtomicUsize::new(0);
            std::thread::scope(|scope| -> Result<()> {
                let workers: Vec<_> = (0..8)
                    .map(|_| {
                        scope.spawn(|| -> Result<()> {
                            loop {
                                let n = next.fetch_add(1, Ordering::Relaxed);
                                let Some(input) = source.files.get(n) else {
                                    break;
                                };
                                self.fetch_visual_input(resolved, input)?;
                            }
                            Ok(())
                        })
                    })
                    .collect();
                for worker in workers {
                    worker
                        .join()
                        .map_err(|_| "Visual download worker panicked")??;
                }
                Ok(())
            })?;
            let temp =
                tempfile::tempdir_in(prepared.parent().unwrap()).map_err(|e| e.to_string())?;
            let model_path = self.visual_blob(
                &source
                    .files
                    .iter()
                    .find(|f| f.path == "blocks_models.json")
                    .unwrap()
                    .hash,
            );
            let state_path = self.visual_blob(
                &source
                    .files
                    .iter()
                    .find(|f| f.path == "blocks_states.json")
                    .unwrap()
                    .hash,
            );
            let (models, states, unresolved) =
                visual_models::prepare(self.json(&model_path)?, self.json(&state_path)?)?;
            write_json(&temp.path().join("models.json"), &models)?;
            write_json(&temp.path().join("blockstates.json"), &states)?;
            let atlas = visual_atlas::prepare(self, &source.files, temp.path())?;
            visual_models::validate_textures(
                &models,
                &self.json(&temp.path().join("textures.json"))?,
            )?;
            let manifest = json!({
                "preparation_format": VISUAL_FORMAT, "bundle": source.bundle,
                "source": {"repository": "https://github.com/PrismarineJS/minecraft-assets", "revision": REVISION, "versions": versions},
                "files": source.files, "atlases": atlas, "unresolved_texture_variables": unresolved,
                "limitations": ["Entity textures are included; entity geometry is not supplied by this bundle.", "Fluids and entity-rendered blocks require specialized geometry.", "Tint indices and colormaps are retained; biome tint colors are not evaluated.", "Animations use their first declared frame; interpolation is not rendered.", "Unbound variables in abstract model templates remain symbolic."]
            });
            write_json(&temp.path().join("manifest.json"), &manifest)?;
            if prepared.exists() {
                fs::remove_dir_all(&prepared).map_err(|e| e.to_string())?;
            }
            fs::rename(temp.path(), &prepared).map_err(|e| e.to_string())?;
        }
        fs::create_dir_all(record_path.parent().unwrap()).map_err(|e| e.to_string())?;
        write_json(
            &record_path,
            &json!({"requested_version": version, "resolved_version": resolved, "match": if version == resolved { if versions.len() > 1 { "known_identical" } else { "exact" } } else { "approximate" }, "equivalent_versions": versions, "bundle": source.bundle, "preparation_format": VISUAL_FORMAT, "source_revision": REVISION}),
        )?;
        Ok(prepared)
    }
}
