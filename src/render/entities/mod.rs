mod catalog;
mod geometry;

use super::{Draw, GeometryAssets, Instance, Mesh};
use crate::{Result, model::Entity};
use catalog::EntityCatalog;
use fastnbt::Value;

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Byte(v) => Some(f64::from(*v)),
        Value::Short(v) => Some(f64::from(*v)),
        Value::Int(v) => Some(f64::from(*v)),
        Value::Long(v) => Some(*v as f64),
        Value::Float(v) => Some(f64::from(*v)),
        Value::Double(v) => Some(*v),
        _ => None,
    }
}

pub(super) fn id(entity: &Entity) -> &str {
    match entity.data.get("id") {
        Some(Value::String(id)) => id,
        _ => "entity",
    }
}

pub(super) fn facing(entity: &Entity) -> Result<[f32; 4]> {
    for field in ["Age", "Saddle", "Invisible"] {
        if let Some(value) = entity.data.get(field) {
            let value = number(value).ok_or_else(|| format!("Invalid entity {field}"))?;
            if !value.is_finite()
                || (if field == "Age" {
                    value < 0.
                } else {
                    value != 0.
                })
            {
                return Err("Only visible adult, unsaddled entities are supported".into());
            }
        }
    }
    if entity.data.contains_key("variant") || entity.data.contains_key("Passengers") {
        return Err("Entity variants and passengers are not supported".into());
    }
    let yaw = match entity.data.get("Rotation") {
        None => 0.,
        Some(Value::List(values)) if values.len() == 2 => {
            let yaw = number(&values[0]).ok_or("Invalid entity yaw")?;
            let pitch = number(&values[1]).ok_or("Invalid entity pitch")?;
            if !yaw.is_finite() || !pitch.is_finite() {
                return Err("Non-finite entity rotation".into());
            }
            if pitch != 0. {
                return Err("Entity head pitch is not supported yet".into());
            }
            yaw
        }
        _ => return Err("Invalid entity Rotation".into()),
    };
    let angle = ((180. - yaw.rem_euclid(360.)) as f32).to_radians() / 2.;
    Ok([0., angle.sin(), 0., angle.cos()])
}

pub(super) fn bake(assets: &GeometryAssets, id: &str) -> Result<Mesh> {
    let catalog = EntityCatalog::bundled()?;
    let definition = catalog
        .entities
        .get(id)
        .ok_or_else(|| format!("No model definition for {id}"))?;
    let model = catalog
        .models
        .get(&definition.model)
        .ok_or("Missing entity geometry")?;
    let texture = &definition.texture;
    let sprite = assets
        .texture_ids
        .get(texture)
        .map(|&id| &assets.textures[id])
        .ok_or("Entity texture is unavailable in this visual bundle")?;
    let dimensions = [
        model["texturewidth"].as_f64().unwrap_or(64.),
        model["textureheight"].as_f64().unwrap_or(64.),
    ];
    if dimensions.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("Invalid entity texture dimensions".into());
    }
    let scale = std::array::from_fn(|i| sprite.size[i] as f32 / dimensions[i] as f32);
    fn scale_uvs(part: &mut super::parts::Part, scale: [f32; 2]) {
        for cube in &mut part.cuboids {
            for face in cube.faces.values_mut() {
                for (i, uv) in face.uv.iter_mut().enumerate() {
                    *uv *= scale[i % 2];
                }
            }
        }
        for child in part.children.values_mut() {
            scale_uvs(child, scale);
        }
    }
    let mut parts = geometry::parts(model, texture)?;
    scale_uvs(&mut parts, scale);
    parts.scale = [model["scale"].as_f64().unwrap_or(1.) as f32; 3];
    let mut mesh = assets.bake_parts(&parts)?;
    for quad in &mut mesh.quads {
        quad.texture_flags = serde_json::json!({"force_cutout": true});
        for vertex in &mut quad.vertices {
            vertex.position[0] = -vertex.position[0];
        }
        quad.vertices.reverse();
        quad.normal[0] = -quad.normal[0];
    }
    Ok(mesh)
}

pub(super) fn instance(
    name: &str,
    position: [f64; 3],
    rotation: [f32; 4],
    mesh: usize,
    quads: usize,
) -> Instance {
    Instance {
        is_entity: true,
        position,
        rotation,
        name: name.into(),
        draws: vec![Draw {
            mesh,
            quads: (0..quads).collect(),
        }],
    }
}
