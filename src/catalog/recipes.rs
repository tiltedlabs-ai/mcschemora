use crate::Result;
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, io::Read, sync::OnceLock};

pub(super) const KINDS: [&str; 3] = ["recipes", "recipeDefinitions", "itemTags"];

#[derive(Deserialize)]
struct Variant {
    versions: Vec<String>,
    value: Value,
}

#[derive(Deserialize)]
struct Tracker {
    provenance: BTreeMap<String, Value>,
    datasets: BTreeMap<String, BTreeMap<String, Vec<Variant>>>,
}

fn item_id(name: &str, items: &BTreeMap<String, u64>) -> Result<u64> {
    items
        .get(name)
        .copied()
        .ok_or_else(|| format!("Unknown recipe item {name}"))
}

fn remap(value: &mut Value, items: &BTreeMap<String, u64>) -> Result<()> {
    match value {
        Value::String(name) => *value = item_id(name, items)?.into(),
        Value::Array(values) => {
            for value in values {
                remap(value, items)?;
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                remap(value, items)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn load(version: &str, items: &Value) -> Result<BTreeMap<String, Value>> {
    static TRACKER: OnceLock<Result<Tracker>> = OnceLock::new();
    let tracker = TRACKER
        .get_or_init(|| {
            serde_json::from_reader(
                GzDecoder::new(include_bytes!("recipes/tracker.json.gz").as_slice())
                    .take(super::source::JSON_LIMIT + 1),
            )
            .map_err(|error| format!("Invalid official recipe tracker: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    if !tracker.provenance.contains_key(version) {
        return Err(format!(
            "No official recipes for Java {version}; regenerate the recipe tracker"
        ));
    }
    let items: BTreeMap<_, _> = items
        .as_array()
        .ok_or("Item catalog must be an array")?
        .iter()
        .map(|item| {
            Ok((
                super::namespace(item["name"].as_str().ok_or("Item has no name")?),
                item["id"].as_u64().ok_or("Item has no numeric ID")?,
            ))
        })
        .collect::<Result<_>>()?;
    let mut datasets = BTreeMap::new();
    for kind in KINDS {
        let mut dataset = serde_json::Map::new();
        for (name, variants) in tracker
            .datasets
            .get(kind)
            .ok_or_else(|| format!("Recipe tracker has no {kind} dataset"))?
        {
            let mut matching = variants
                .iter()
                .filter(|variant| variant.versions.iter().any(|entry| entry == version));
            let Some(variant) = matching.next() else {
                continue;
            };
            if matching.next().is_some() {
                return Err(format!("Overlapping recipe variants for {kind}/{name}"));
            }
            let mut value = variant.value.clone();
            if kind != "recipeDefinitions" {
                remap(&mut value, &items)?;
            }
            let key = if kind == "recipes" {
                item_id(name, &items)?.to_string()
            } else {
                name.clone()
            };
            dataset.insert(key, value);
        }
        datasets.insert(kind.to_string(), Value::Object(dataset));
    }
    Ok(datasets)
}
