use super::{compound, dye, list, number, string};
use crate::{
    Result,
    model::{Block, Compound},
    render::{GeometryAssets, Mesh, Quad, Vertex, geometry::rotate},
};

fn text(value: &str) -> String {
    fn flatten(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a.iter().map(flatten).collect(),
            serde_json::Value::Object(o) => {
                o.get("text").map(flatten).unwrap_or_default()
                    + &o.get("extra").map(flatten).unwrap_or_default()
            }
            _ => String::new(),
        }
    }
    serde_json::from_str(value).map_or_else(|_| value.into(), |v| flatten(&v))
}
pub(super) fn bake(
    assets: &GeometryAssets,
    block: &Block,
    data: &Compound,
    mesh: &mut Mesh,
) -> Result<()> {
    let Some(&texture) = assets.texture_ids.get("minecraft:font/ascii") else {
        return Err("Sign font missing from visual bundle".into());
    };
    let surfaces: Vec<_> = mesh
        .quads
        .iter()
        .filter(|q| {
            q.normal[1].abs() < 0.01
                && (q.vertices[1].position[1] - q.vertices[0].position[1]).abs() > 0.2
                && (0..3)
                    .map(|i| (q.vertices[3].position[i] - q.vertices[0].position[i]).powi(2))
                    .sum::<f32>()
                    > 0.2
        })
        .cloned()
        .collect();
    let angle = block
        .properties
        .get("rotation")
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(0.)
        * 22.5;
    let front = match block.properties.get("facing").map(String::as_str) {
        Some("north") => [0., 0., -1.],
        Some("east") => [1., 0., 0.],
        Some("west") => [-1., 0., 0.],
        Some("south") => [0., 0., 1.],
        _ => rotate([0., 0., 1.], 1, -angle, [0.; 3], false),
    };
    for face in surfaces {
        let is_front = (0..3).map(|i| face.normal[i] * front[i]).sum::<f32>() > 0.;
        let side = compound(data.get(if is_front { "front_text" } else { "back_text" }));
        let lines: Vec<String> = if let Some(side) = side {
            list(side.get("messages"))
                .iter()
                .take(4)
                .map(|v| string(Some(v)).map(text).unwrap_or_default())
                .collect()
        } else if is_front {
            (1..=4)
                .map(|i| {
                    string(data.get(&format!("Text{i}")))
                        .map(text)
                        .unwrap_or_default()
                })
                .collect()
        } else {
            Vec::new()
        };
        let color = dye(side
            .and_then(|s| string(s.get("color")))
            .or_else(|| string(data.get("Color")))
            .unwrap_or("black"));
        let glowing = side.is_some_and(|s| number(s.get("has_glowing_text")) != 0.);
        for (row, line) in lines.iter().enumerate() {
            let chars: Vec<_> = line.chars().take(24).collect();
            let width = (0.78 / chars.len().max(1) as f32).min(0.055);
            for (column, ch) in chars.iter().enumerate() {
                if ch.is_whitespace() {
                    continue;
                }
                let glyph = if ch.is_ascii() {
                    *ch as u32
                } else {
                    u32::from(b'?')
                };
                let u0 = 0.5 - chars.len() as f32 * width * 0.5 + column as f32 * width;
                let v0 = 0.08 + row as f32 * 0.21;
                let uv0 = [(glyph % 16) as f32 / 16., (glyph / 16) as f32 / 16.];
                let positions = [
                    [u0, v0],
                    [u0, v0 + 0.2],
                    [u0 + width, v0 + 0.2],
                    [u0 + width, v0],
                ]
                .map(|[u, v]| {
                    std::array::from_fn(|i| {
                        face.vertices[0].position[i]
                            + u * (face.vertices[3].position[i] - face.vertices[0].position[i])
                            + v * (face.vertices[1].position[i] - face.vertices[0].position[i])
                            + face.normal[i] * 0.0005
                    })
                });
                mesh.quads.push(Quad {
                    vertices: std::array::from_fn(|i| Vertex {
                        position: positions[i],
                        uv: [
                            uv0[0] + [0., 0., 1., 1.][i] / 16.,
                            uv0[1] + [0., 1., 1., 0.][i] / 16.,
                        ],
                    }),
                    normal: face.normal,
                    texture,
                    color,
                    shade: !glowing,
                    tint_index: None,
                    texture_flags: serde_json::json!({"force_cutout":true}),
                    cull_face: None,
                });
            }
        }
    }
    Ok(())
}
