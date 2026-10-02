use crate::{Result, model::Position};
use std::sync::LazyLock;

pub(super) const DIRECTIONS: [&str; 6] = ["down", "up", "north", "south", "west", "east"];
pub(super) const UV_CORNERS: [[f32; 2]; 4] = [[0., 0.], [0., 1.], [1., 1.], [1., 0.]];

pub(super) fn offset(p: Position, d: Position) -> Option<Position> {
    Some([
        p[0].checked_add(d[0])?,
        p[1].checked_add(d[1])?,
        p[2].checked_add(d[2])?,
    ])
}

pub(super) fn linear_colors() -> &'static [f32; 256] {
    static COLORS: LazyLock<[f32; 256]> = LazyLock::new(|| {
        std::array::from_fn(|i| {
            let value = i as f32 / 255.;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        })
    });
    &COLORS
}

pub(super) fn linear_color(color: [u8; 4]) -> [f32; 4] {
    let colors = linear_colors();
    [
        colors[color[0] as usize],
        colors[color[1] as usize],
        colors[color[2] as usize],
        f32::from(color[3]) / 255.,
    ]
}

pub(super) fn corners(direction: &str, f: [f32; 3], t: [f32; 3]) -> Result<[[f32; 3]; 4]> {
    let [x0, y0, z0] = f;
    let [x1, y1, z1] = t;
    Ok(match direction {
        "down" => [[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]],
        "up" => [[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]],
        "north" => [[x1, y1, z0], [x1, y0, z0], [x0, y0, z0], [x0, y1, z0]],
        "south" => [[x0, y1, z1], [x0, y0, z1], [x1, y0, z1], [x1, y1, z1]],
        "west" => [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]],
        "east" => [[x1, y1, z1], [x1, y0, z1], [x1, y0, z0], [x1, y1, z0]],
        _ => return Err(format!("Invalid model face {direction}")),
    })
}

pub(super) fn normal(positions: [[f32; 3]; 4]) -> Option<[f32; 3]> {
    let a: [f32; 3] = std::array::from_fn(|i| positions[1][i] - positions[0][i]);
    let b: [f32; 3] = std::array::from_fn(|i| positions[2][i] - positions[0][i]);
    let n = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let length = n.iter().map(|v| v * v).sum::<f32>().sqrt();
    (length > 1e-7).then(|| n.map(|v| v / length))
}

pub(super) fn rotate(
    mut p: [f32; 3],
    axis: usize,
    angle: f32,
    origin: [f32; 3],
    rescale: bool,
) -> [f32; 3] {
    let (sin, cos) = angle.to_radians().sin_cos();
    let a = (axis + 1) % 3;
    let b = (axis + 2) % 3;
    let x = p[a] - origin[a];
    let y = p[b] - origin[b];
    let scale = if rescale { 1. / cos.abs() } else { 1. };
    p[a] = origin[a] + (x * cos - y * sin) * scale;
    p[b] = origin[b] + (x * sin + y * cos) * scale;
    p
}
