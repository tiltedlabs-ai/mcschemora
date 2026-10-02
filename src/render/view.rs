#[derive(Clone, Copy, Debug, Default)]
pub enum View {
    #[default]
    Isometric,
    Top,
    Bottom,
    North,
    South,
    East,
    West,
}

impl std::str::FromStr for View {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "isometric" => Ok(Self::Isometric),
            "top" => Ok(Self::Top),
            "bottom" => Ok(Self::Bottom),
            "north" => Ok(Self::North),
            "south" => Ok(Self::South),
            "east" => Ok(Self::East),
            "west" => Ok(Self::West),
            _ => Err("view must be isometric, top, bottom, north, south, east, or west".into()),
        }
    }
}

impl View {
    fn basis(self) -> [[f64; 3]; 3] {
        match self {
            Self::Isometric => {
                let a = 1. / 2f64.sqrt();
                let b = 1. / 6f64.sqrt();
                let c = 1. / 3f64.sqrt();
                [[a, 0., -a], [b, -2. * b, b], [c; 3]]
            }
            Self::Top => [[1., 0., 0.], [0., 0., 1.], [0., 1., 0.]],
            Self::Bottom => [[1., 0., 0.], [0., 0., -1.], [0., -1., 0.]],
            Self::North => [[-1., 0., 0.], [0., -1., 0.], [0., 0., -1.]],
            Self::South => [[1., 0., 0.], [0., -1., 0.], [0., 0., 1.]],
            Self::East => [[0., 0., -1.], [0., -1., 0.], [1., 0., 0.]],
            Self::West => [[0., 0., 1.], [0., -1., 0.], [-1., 0., 0.]],
        }
    }

    pub(crate) fn project(self, position: [f64; 3]) -> [f64; 3] {
        self.basis()
            .map(|row| row.into_iter().zip(position).map(|(a, b)| a * b).sum())
    }

    pub(crate) fn unproject(self, position: [f64; 3]) -> [f64; 3] {
        let basis = self.basis();
        std::array::from_fn(|axis| (0..3).map(|i| basis[i][axis] * position[i]).sum())
    }
}
