use crate::Result;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn identifier(value: &str, model: bool) -> String {
    let value = if model && !value.contains('/') && !value.contains(':') {
        format!("block/{value}")
    } else {
        value.into()
    };
    if value.contains(':') {
        value
    } else {
        format!("minecraft:{value}")
    }
}

fn inherit(
    name: &str,
    source: &BTreeMap<String, Value>,
    merged: &mut BTreeMap<String, Value>,
    visiting: &mut BTreeSet<String>,
) -> Result<Value> {
    if let Some(value) = merged.get(name) {
        return Ok(value.clone());
    }
    if !visiting.insert(name.into()) {
        return Err(format!("Cyclic model inheritance at {name}"));
    }
    let child = source
        .get(name)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("Missing or invalid visual model {name}"))?;
    let mut result = if let Some(parent) = child.get("parent").and_then(Value::as_str) {
        inherit(&identifier(parent, true), source, merged, visiting)?
            .as_object()
            .cloned()
            .ok_or("Invalid inherited model")?
    } else {
        Map::new()
    };
    for (key, value) in child {
        if key == "parent" {
            continue;
        }
        if key == "textures" || key == "display" {
            let mut fields = result
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            fields.extend(value.as_object().ok_or("Invalid model mapping")?.clone());
            result.insert(key.clone(), Value::Object(fields));
        } else {
            result.insert(key.clone(), value.clone());
        }
    }
    visiting.remove(name);
    let result = Value::Object(result);
    merged.insert(name.into(), result.clone());
    Ok(result)
}

fn texture(
    value: &Value,
    textures: &Map<String, Value>,
    visiting: &mut BTreeSet<String>,
) -> Result<Value> {
    let sprite = value
        .as_str()
        .or_else(|| value.get("sprite").and_then(Value::as_str))
        .ok_or("Unsupported visual texture binding")?;
    if let Some(variable) = sprite.strip_prefix('#').or_else(|| {
        (!sprite.contains('/') && !sprite.contains(':') && textures.contains_key(sprite))
            .then_some(sprite)
    }) {
        if !visiting.insert(variable.into()) {
            return Ok(value.clone());
        }
        let resolved = if let Some(binding) = textures.get(variable) {
            texture(binding, textures, visiting)?
        } else {
            value.clone()
        };
        visiting.remove(variable);
        if let Some(flags) = value.as_object() {
            let mut merged = resolved
                .as_object()
                .cloned()
                .unwrap_or_else(|| Map::from_iter([("sprite".into(), resolved)]));
            merged.extend(
                flags
                    .iter()
                    .filter(|(k, _)| k.as_str() != "sprite")
                    .map(|(k, v)| (k.clone(), v.clone())),
            );
            return Ok(Value::Object(merged));
        }
        return Ok(resolved);
    }
    let sprite = Value::String(identifier(sprite, false));
    if let Some(fields) = value.as_object() {
        let mut fields = fields.clone();
        fields.insert("sprite".into(), sprite);
        Ok(Value::Object(fields))
    } else {
        Ok(sprite)
    }
}

pub(super) fn validate_textures(models: &BTreeMap<String, Value>, textures: &Value) -> Result<()> {
    let sprites = textures
        .as_object()
        .ok_or("Invalid prepared texture mapping")?;
    for (name, model) in models {
        if let Some(elements) = model["elements"].as_array() {
            for element in elements {
                if let Some(faces) = element["faces"].as_object() {
                    for face in faces.values() {
                        let sprite = face["texture"]
                            .as_str()
                            .ok_or("Invalid prepared face texture")?;
                        if !sprite.starts_with('#') && !sprites.contains_key(sprite) {
                            return Err(format!(
                                "Model {name} references unavailable sprite {sprite}"
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn default_uv(direction: &str, from: &[Value], to: &[Value]) -> Result<Value> {
    let f: Vec<f64> = from
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| "Invalid model coordinate".to_owned())
        })
        .collect::<Result<_>>()?;
    let t: Vec<f64> = to
        .iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| "Invalid model coordinate".to_owned())
        })
        .collect::<Result<_>>()?;
    if f.len() != 3 || t.len() != 3 {
        return Err("Invalid model element bounds".into());
    }
    Ok(match direction {
        "down" => json!([f[0], 16.0 - t[2], t[0], 16.0 - f[2]]),
        "up" => json!([f[0], f[2], t[0], t[2]]),
        "north" => json!([16.0 - t[0], 16.0 - t[1], 16.0 - f[0], 16.0 - f[1]]),
        "south" => json!([f[0], 16.0 - t[1], t[0], 16.0 - f[1]]),
        "west" => json!([f[2], 16.0 - t[1], t[2], 16.0 - f[1]]),
        "east" => json!([16.0 - t[2], 16.0 - t[1], 16.0 - f[2], 16.0 - f[1]]),
        _ => return Err(format!("Unsupported model face direction {direction}")),
    })
}

fn normalize_states(
    value: &mut Value,
    models: &BTreeMap<String, Value>,
    used: &mut BTreeSet<String>,
) -> Result<()> {
    match value {
        Value::Object(fields) => {
            if let Some(model) = fields.get_mut("model") {
                let name = identifier(model.as_str().ok_or("Invalid blockstate model")?, true);
                if !models.contains_key(&name) {
                    return Err(format!("Blockstate references missing model {name}"));
                }
                used.insert(name.clone());
                *model = Value::String(name);
            }
            for field in fields.values_mut() {
                normalize_states(field, models, used)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_states(item, models, used)?;
            }
        }
        _ => (),
    }
    Ok(())
}

type Prepared = (
    BTreeMap<String, Value>,
    BTreeMap<String, Value>,
    BTreeMap<String, Vec<String>>,
);

pub(super) fn prepare(models: Value, states: Value) -> Result<Prepared> {
    let source: BTreeMap<String, Value> = models
        .as_object()
        .ok_or("Unsupported visual model aggregate")?
        .iter()
        .map(|(k, v)| (identifier(k, true), v.clone()))
        .collect();
    let mut merged = BTreeMap::new();
    for name in source.keys() {
        inherit(name, &source, &mut merged, &mut BTreeSet::new())?;
    }
    let mut unresolved = BTreeMap::new();
    for (name, model) in &mut merged {
        let bindings = model
            .get("textures")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let resolved: Map<String, Value> = bindings
            .iter()
            .map(|(k, v)| texture(v, &bindings, &mut BTreeSet::new()).map(|v| (k.clone(), v)))
            .collect::<Result<_>>()?;
        let mut variables: BTreeSet<String> = resolved
            .values()
            .filter_map(|v| v.as_str().or_else(|| v["sprite"].as_str()))
            .filter(|v| v.starts_with('#'))
            .map(str::to_owned)
            .collect();
        model["textures"] = Value::Object(resolved);
        if let Some(elements) = model.get_mut("elements").and_then(Value::as_array_mut) {
            for element in elements {
                let from = element["from"]
                    .as_array()
                    .cloned()
                    .ok_or("Missing model element from")?;
                let to = element["to"]
                    .as_array()
                    .cloned()
                    .ok_or("Missing model element to")?;
                if let Some(faces) = element.get_mut("faces").and_then(Value::as_object_mut) {
                    for (direction, face) in faces {
                        let fields = face.as_object_mut().ok_or("Invalid visual model face")?;
                        if let Some(binding) = fields.get("texture") {
                            let bound = texture(binding, &bindings, &mut BTreeSet::new())?;
                            if let Some(flags) = bound.as_object() {
                                fields.insert("texture".into(), flags["sprite"].clone());
                                let flags: Map<_, _> = flags
                                    .iter()
                                    .filter(|(k, _)| k.as_str() != "sprite")
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect();
                                if !flags.is_empty() {
                                    fields.insert("texture_flags".into(), Value::Object(flags));
                                }
                            } else {
                                fields.insert("texture".into(), bound);
                            }
                        }
                        if let Some(variable) = fields
                            .get("texture")
                            .and_then(Value::as_str)
                            .filter(|v| v.starts_with('#'))
                        {
                            variables.insert(variable.into());
                        }
                        if !fields.contains_key("uv") {
                            fields.insert("uv".into(), default_uv(direction, &from, &to)?);
                        }
                    }
                }
            }
        }
        if !variables.is_empty() {
            unresolved.insert(name.clone(), variables.into_iter().collect());
        }
    }
    let mut states: BTreeMap<String, Value> = states
        .as_object()
        .ok_or("Unsupported visual blockstate aggregate")?
        .iter()
        .map(|(k, v)| (identifier(k, false), v.clone()))
        .collect();
    let mut used = BTreeSet::new();
    for state in states.values_mut() {
        normalize_states(state, &merged, &mut used)?;
    }
    for name in used {
        if let Some(elements) = merged[&name]["elements"].as_array() {
            for element in elements {
                if let Some(faces) = element["faces"].as_object() {
                    for face in faces.values() {
                        if face["texture"].as_str().is_some_and(|v| v.starts_with('#')) {
                            return Err(format!(
                                "Concrete blockstate model {name} has an unbound face texture"
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok((merged, states, unresolved))
}
