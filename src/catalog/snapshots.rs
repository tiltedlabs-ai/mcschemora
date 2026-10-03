use crate::Result;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct Snapshot {
    version: String,
    data_version: i32,
    datasets: BTreeMap<String, Value>,
}

const SOURCES: &[(&str, &str)] = &[
    ("1.14.2", include_str!("data/1.14.2.json")),
    ("1.19.1", include_str!("data/1.19.1.json")),
    ("1.21.2", include_str!("data/1.21.2.json")),
    ("1.21.7", include_str!("data/1.21.7.json")),
    ("26.1.1", include_str!("data/26.1.1.json")),
    ("26.1.2", include_str!("data/26.1.2.json")),
    ("26.2", include_str!("data/26.2.json")),
    ("26.3", include_str!("data/26.3.json")),
];

pub(super) fn versions() -> impl Iterator<Item = &'static str> {
    SOURCES.iter().map(|(version, _)| *version)
}

pub(super) fn contains(version: &str) -> bool {
    versions().any(|entry| entry == version)
}

pub(super) fn load(version: &str) -> Result<Option<(i32, BTreeMap<String, Value>)>> {
    let Some((_, source)) = SOURCES.iter().find(|(entry, _)| *entry == version) else {
        return Ok(None);
    };
    let snapshot: Snapshot = serde_json::from_str(source)
        .map_err(|error| format!("Official Java{version} registry snapshot: {error}"))?;
    if snapshot.version != version {
        return Err(format!(
            "Official registry snapshot version mismatch for Java{version}"
        ));
    }
    Ok(Some((snapshot.data_version, snapshot.datasets)))
}
