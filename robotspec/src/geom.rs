//! Planar geometry primitives, dependency-free and reusable.

/// Polygon area + centroid (shoelace).
pub fn polygon_area_centroid(pts: &[[f64; 2]]) -> (f64, [f64; 2]) {
    let n = pts.len();
    let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let [x0, y0] = pts[i];
        let [x1, y1] = pts[(i + 1) % n];
        let cross = x0 * y1 - x1 * y0;
        a2 += cross;
        cx += (x0 + x1) * cross;
        cy += (y0 + y1) * cross;
    }
    let area = a2 / 2.0;
    (area.abs(), [cx / (3.0 * a2), cy / (3.0 * a2)])
}

pub fn dist_point_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let (apx, apy) = (p[0] - a[0], p[1] - a[1]);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        ((apx * abx + apy * aby) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (apx - t * abx, apy - t * aby);
    (dx * dx + dy * dy).sqrt()
}
