use crate::render::View;

#[derive(Clone, Copy)]
pub(super) struct Grid {
    pub view: View,
    pub scale: f64,
    pub offset: [f64; 2],
    pub anchor: [f64; 3],
}

impl Grid {
    pub fn triangle(self, points: [[f64; 3]; 3], area: f64) -> Lines {
        let world = points.map(|p| {
            let x = (p[0] - self.offset[0]) / self.scale;
            let y = (p[1] - self.offset[1]) / self.scale;
            let d = p[2];
            let p = self.view.unproject([x, y, d]);
            std::array::from_fn::<_, 3, _>(|i| p[i] + self.anchor[i].rem_euclid(1.))
        });
        Lines(std::array::from_fn(|axis| {
            let a = world[1][axis] - world[0][axis];
            let b = world[2][axis] - world[0][axis];
            let dx = (a * (points[2][1] - points[0][1]) - b * (points[1][1] - points[0][1])) / area;
            let dy = ((points[1][0] - points[0][0]) * b - (points[2][0] - points[0][0]) * a) / area;
            let step = dx.hypot(dy);
            [
                world[0][axis] - dx * points[0][0] - dy * points[0][1],
                dx,
                dy,
                step,
            ]
        }))
    }
}

pub(super) struct Lines([[f64; 4]; 3]);

impl Lines {
    pub fn color(&self, x: f64, y: f64) -> (f32, f32) {
        let mut dark = 0f64;
        let mut outline = 0f64;
        for &[base, dx, dy, step] in &self.0 {
            if step < 1e-8 || step >= 0.25 {
                continue;
            }
            let coordinate = base + dx * x + dy * y;
            let distance = (coordinate - coordinate.round()).abs() / step;
            let fade = (1. / step - 4.).clamp(0., 1.);
            dark = dark.max((1.5 - distance).clamp(0., 1.) * fade);
            outline = outline.max((2.5 - distance).clamp(0., 1.) * fade);
        }
        (
            ((1. - outline) * (1. - dark)) as f32,
            (outline * (1. - dark)) as f32,
        )
    }
}
