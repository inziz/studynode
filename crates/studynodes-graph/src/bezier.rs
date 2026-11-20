use crate::coords::WorldPos;

/// Represents a 2D cubic Bézier curve segment defined by 4 control points: P0, P1, P2, P3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub p0: WorldPos,
    pub p1: WorldPos,
    pub p2: WorldPos,
    pub p3: WorldPos,
}

impl CubicBezier {
    pub fn new(p0: WorldPos, p1: WorldPos, p2: WorldPos, p3: WorldPos) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// Construct a Blender-style horizontal S-curve wire from source socket to destination socket.
    pub fn wire(from: WorldPos, to: WorldPos) -> Self {
        let dx = (to.x - from.x).abs();
        // Dynamic horizontal handle curvature: at least 40pt, scaled by half delta x
        let handle_dist = (dx * 0.5).max(40.0);

        let p0 = from;
        let p1 = WorldPos::new(from.x + handle_dist, from.y);
        let p2 = WorldPos::new(to.x - handle_dist, to.y);
        let p3 = to;

        Self { p0, p1, p2, p3 }
    }

    /// Evaluate the cubic Bézier curve at parameter t in [0.0, 1.0].
    pub fn eval(&self, t: f32) -> WorldPos {
        let u = 1.0 - t;
        let tt = t * t;
        let uu = u * u;
        let uuu = uu * u;
        let ttt = tt * t;

        let x = uuu * self.p0.x
            + 3.0 * uu * t * self.p1.x
            + 3.0 * u * tt * self.p2.x
            + ttt * self.p3.x;

        let y = uuu * self.p0.y
            + 3.0 * uu * t * self.p1.y
            + 3.0 * u * tt * self.p2.y
            + ttt * self.p3.y;

        WorldPos::new(x, y)
    }

    /// Compute midpoint B(0.5) where the relation / annotation pill is anchored.
    pub fn midpoint(&self) -> WorldPos {
        self.eval(0.5)
    }

    /// Subdivide the curve into N line segments.
    pub fn sample_segments(&self, count: usize) -> Vec<WorldPos> {
        let n = count.max(4);
        (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                self.eval(t)
            })
            .collect()
    }

    /// Calculate the minimum distance from a point to this Bézier curve by sampling segments.
    pub fn distance_to_point(&self, pt: WorldPos, segments: usize) -> f32 {
        let pts = self.sample_segments(segments);
        let mut min_dist = f32::MAX;

        for pair in pts.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let d = point_to_segment_distance(pt, a, b);
            if d < min_dist {
                min_dist = d;
            }
        }

        min_dist
    }

    /// Hit-test: returns true if the point is within `threshold` units of the curve.
    pub fn hit_test(&self, pt: WorldPos, threshold: f32) -> bool {
        self.distance_to_point(pt, 16) <= threshold
    }
}

/// Calculate the Euclidean distance from point P to line segment AB.
pub fn point_to_segment_distance(p: WorldPos, a: WorldPos, b: WorldPos) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-6 {
        return p.distance_to(&a);
    }

    // Projection parameter t of point P onto segment AB clamped to [0, 1]
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    let proj = WorldPos::new(a.x + t * dx, a.y + t * dy);
    p.distance_to(&proj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bezier_endpoints() {
        let from = WorldPos::new(10.0, 20.0);
        let to = WorldPos::new(300.0, 200.0);
        let curve = CubicBezier::wire(from, to);

        let start = curve.eval(0.0);
        let end = curve.eval(1.0);

        assert!((start.x - from.x).abs() < 1e-4);
        assert!((start.y - from.y).abs() < 1e-4);
        assert!((end.x - to.x).abs() < 1e-4);
        assert!((end.y - to.y).abs() < 1e-4);
    }

    #[test]
    fn test_wire_hit_test() {
        let from = WorldPos::new(0.0, 0.0);
        let to = WorldPos::new(200.0, 100.0);
        let curve = CubicBezier::wire(from, to);

        let mid = curve.midpoint();
        // Point right on the midpoint should hit
        assert!(curve.hit_test(mid, 5.0));

        // Point near the midpoint (2 units away) should hit
        let near = WorldPos::new(mid.x, mid.y + 2.0);
        assert!(curve.hit_test(near, 5.0));

        // Point far away (50 units away) should not hit
        let far = WorldPos::new(mid.x, mid.y + 50.0);
        assert!(!curve.hit_test(far, 5.0));
    }
}
