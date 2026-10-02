use super::geometry::offset;
use crate::model::Position;
use std::collections::HashSet;

const NEIGHBORS: [Position; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];

pub(super) enum Occlusion {
    Dense {
        origin: Position,
        size: [usize; 3],
        bits: Vec<u64>,
    },
    Sparse(HashSet<Position>),
}

impl Occlusion {
    pub(super) fn new(positions: impl ExactSizeIterator<Item = Position>) -> Self {
        let count = positions.len();
        let mut min = [i32::MAX; 3];
        let mut max = [i32::MIN; 3];
        for position in positions {
            for axis in 0..3 {
                min[axis] = min[axis].min(position[axis]);
                max[axis] = max[axis].max(position[axis]);
            }
        }
        let size = std::array::from_fn(|axis| {
            usize::try_from(i64::from(max[axis]) - i64::from(min[axis]) + 1).unwrap_or(0)
        });
        let volume = size.iter().try_fold(1usize, |v, side| v.checked_mul(*side));
        if let Some(volume) =
            volume.filter(|&v| v > 0 && v <= 16_777_216 && v <= count.saturating_mul(16))
        {
            Self::Dense {
                origin: min,
                size,
                bits: vec![0; volume.div_ceil(64)],
            }
        } else {
            Self::Sparse(HashSet::with_capacity(count))
        }
    }

    fn index(position: &Position, origin: &Position, size: &[usize; 3]) -> Option<usize> {
        let mut local = [0; 3];
        for axis in 0..3 {
            local[axis] =
                usize::try_from(i64::from(position[axis]) - i64::from(origin[axis])).ok()?;
            if local[axis] >= size[axis] {
                return None;
            }
        }
        Some((local[0] * size[1] + local[1]) * size[2] + local[2])
    }

    fn occupied(bits: &[u64], index: usize) -> bool {
        bits[index / 64] & (1 << (index % 64)) != 0
    }

    pub(super) fn insert(&mut self, position: Position) {
        match self {
            Self::Dense { origin, size, bits } => {
                let index = Self::index(&position, origin, size).unwrap();
                bits[index / 64] |= 1 << (index % 64);
            }
            Self::Sparse(positions) => {
                positions.insert(position);
            }
        }
    }

    pub(super) fn contains(&self, position: &Position) -> bool {
        match self {
            Self::Dense { origin, size, bits } => {
                Self::index(position, origin, size).is_some_and(|index| Self::occupied(bits, index))
            }
            Self::Sparse(positions) => positions.contains(position),
        }
    }

    pub(super) fn encloses(&self, position: Position) -> bool {
        match self {
            Self::Dense { origin, size, bits } => {
                for axis in 0..3 {
                    let local = i64::from(position[axis]) - i64::from(origin[axis]);
                    if local <= 0 || local >= size[axis] as i64 - 1 {
                        return false;
                    }
                }
                let index = Self::index(&position, origin, size).unwrap();
                [size[1] * size[2], size[2], 1].into_iter().all(|stride| {
                    Self::occupied(bits, index - stride) && Self::occupied(bits, index + stride)
                })
            }
            Self::Sparse(positions) => NEIGHBORS.into_iter().all(|direction| {
                offset(position, direction).is_some_and(|neighbor| positions.contains(&neighbor))
            }),
        }
    }
}
