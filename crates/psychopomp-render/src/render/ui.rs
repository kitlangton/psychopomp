pub(crate) mod card;

/// Signed distance from `local` to a `size` rectangle centred on the origin,
/// its corners rounded by `radius` clamped to the shorter half-side.
pub(crate) fn rounded_rect_distance(local: [f32; 2], size: [f32; 2], radius: f32) -> f32 {
    let radius = radius.min(size[0].min(size[1]) * 0.5);
    let dx = local[0].abs() - (size[0] * 0.5 - radius);
    let dy = local[1].abs() - (size[1] * 0.5 - radius);
    let (outside_x, outside_y) = (dx.max(0.0), dy.max(0.0));
    let outside = if outside_x == 0.0 {
        outside_y.abs()
    } else if outside_y == 0.0 {
        outside_x.abs()
    } else {
        outside_x.hypot(outside_y)
    };
    outside + dx.max(dy).min(0.0) - radius
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub origin: [f32; 2],
    pub size: [f32; 2],
}

impl Bounds {
    pub fn from_center(center: [f32; 2], size: [f32; 2]) -> Self {
        Self {
            origin: [center[0] - size[0] * 0.5, center[1] - size[1] * 0.5],
            size,
        }
    }

    pub fn center(self) -> [f32; 2] {
        [
            self.origin[0] + self.size[0] * 0.5,
            self.origin[1] + self.size[1] * 0.5,
        ]
    }

    pub fn right(self) -> f32 {
        self.origin[0] + self.size[0]
    }

    pub fn bottom(self) -> f32 {
        self.origin[1] + self.size[1]
    }

    pub fn inset(self, edges: Edges) -> Self {
        Self {
            origin: [self.origin[0] + edges.left, self.origin[1] + edges.top],
            size: [
                (self.size[0] - edges.left - edges.right).max(0.0),
                (self.size[1] - edges.top - edges.bottom).max(0.0),
            ],
        }
    }

    pub fn expand(self, amount: f32) -> Self {
        self.inset(Edges::all(-amount))
    }

    pub fn translate(self, offset: [f32; 2]) -> Self {
        Self {
            origin: [self.origin[0] + offset[0], self.origin[1] + offset[1]],
            size: self.size,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub fn all(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::rounded_rect_distance;

    #[test]
    fn rounded_rect_distance_matches_the_hypot_formula_bit_for_bit() {
        let reference = |local: [f32; 2], size: [f32; 2], radius: f32| {
            let radius = radius.min(size[0].min(size[1]) * 0.5);
            let dx = local[0].abs() - (size[0] * 0.5 - radius);
            let dy = local[1].abs() - (size[1] * 0.5 - radius);
            dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius
        };
        for (size, radius) in [
            ([200.0, 120.0], 26.0),
            ([31.5, 9.0], 40.0),
            ([64.0, 64.0], 0.0),
        ] {
            for y in -90..=90 {
                for x in -130..=130 {
                    let local = [x as f32 * 0.83 + 0.5, y as f32 * 0.71 - 0.25];
                    assert_eq!(
                        rounded_rect_distance(local, size, radius).to_bits(),
                        reference(local, size, radius).to_bits(),
                        "{local:?} {size:?} {radius}"
                    );
                }
            }
            for local in [
                [0.0, 0.0],
                [-0.0, 0.0],
                [size[0] * 0.5, size[1] * 0.5],
                [f32::INFINITY, 1.0],
            ] {
                assert_eq!(
                    rounded_rect_distance(local, size, radius).to_bits(),
                    reference(local, size, radius).to_bits()
                );
            }
        }
    }
}
