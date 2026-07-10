//! Contact resolution (SPEC §2.2 primitive: bodies, hulls, impulse
//! resolution). Pulled forward from M2 so the Rapier differential can
//! exercise the contact scenarios C2/C3/C5 against a genuine independent
//! solver.
//!
//! v1 scope: oriented boxes, vertex-face contact generation, sequential
//! impulses with restitution + Coulomb contact friction, Baumgarte
//! positional bias. At the 8 kHz world rate penetrations are micrometres per
//! tick, so vertex-face manifolds dominate and few solver iterations are
//! needed. Deterministic: fixed body order, fixed iteration counts, no
//! allocation-order dependence.

use crate::Vec2;
use serde::{Deserialize, Serialize};

pub const CONTACT_VERSION: &str = "0.1.0-m1-boxes";

const SOLVER_ITERATIONS: u32 = 16;
const BAUMGARTE: f64 = 0.2;
const PENETRATION_SLOP: f64 = 1e-5;
/// Approach speeds below this restitute to zero (must match the value the
/// differential rig configures on the Rapier side).
pub const RESTITUTION_THRESHOLD: f64 = 0.05;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContactBody {
    pub inv_m: f64,
    pub inv_i: f64,
    pub pos: Vec2,
    pub heading: f64,
    pub vel: Vec2,
    pub omega: f64,
    /// Half-extent along the body x axis (heading direction).
    pub half_l: f64,
    /// Half-extent along the body y axis.
    pub half_w: f64,
    pub restitution: f64,
    pub friction: f64,
}

impl ContactBody {
    pub fn new_box(mass: f64, half_l: f64, half_w: f64, pos: Vec2, heading: f64) -> Self {
        let inertia = mass * (half_l * half_l + half_w * half_w) / 3.0;
        ContactBody {
            inv_m: 1.0 / mass,
            inv_i: 1.0 / inertia,
            pos,
            heading,
            vel: Vec2::ZERO,
            omega: 0.0,
            half_l,
            half_w,
            restitution: 0.0,
            friction: 0.0,
        }
    }

    pub fn fixed_box(half_l: f64, half_w: f64, pos: Vec2, heading: f64) -> Self {
        ContactBody {
            inv_m: 0.0,
            inv_i: 0.0,
            pos,
            heading,
            vel: Vec2::ZERO,
            omega: 0.0,
            half_l,
            half_w,
            restitution: 0.0,
            friction: 0.0,
        }
    }

    fn rot(&self) -> (f64, f64) {
        (self.heading.cos(), self.heading.sin())
    }

    fn velocity_at(&self, p: Vec2) -> Vec2 {
        let r = p - self.pos;
        Vec2::new(self.vel.x - self.omega * r.y, self.vel.y + self.omega * r.x)
    }
}

/// One contact point. `normal` points in the push-A direction (from B to A).
struct Contact {
    a: usize,
    b: usize,
    point: Vec2,
    normal: Vec2,
    // presolve
    mass_n: f64,
    mass_t: f64,
    bias: f64,
    mu: f64,
    acc_n: f64,
    acc_t: f64,
}

fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

fn perp(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

/// SAT + reference-face clipping (Box2D-lite style) for oriented boxes.
/// Returns up to two contact points as (point, normal, depth) where `normal`
/// pushes A away from B. Handles the degenerate aligned face-face case that
/// vertex-containment tests miss (two identical boxes meeting face-on have
/// every corner exactly ON the other's boundary).
fn collide_boxes(a: &ContactBody, b: &ContactBody) -> Vec<(Vec2, Vec2, f64)> {
    let axes_of = |body: &ContactBody| {
        let (c, s) = body.rot();
        [Vec2::new(c, s), Vec2::new(-s, c)]
    };
    let halves_of = |body: &ContactBody| [body.half_l, body.half_w];
    let (ax_a, ax_b) = (axes_of(a), axes_of(b));
    let (h_a, h_b) = (halves_of(a), halves_of(b));
    let d = b.pos - a.pos;

    // Candidate axes: A's two face normals then B's two.
    let mut best_sep = f64::NEG_INFINITY;
    let mut best: usize = 0;
    for (idx, &u) in ax_a.iter().chain(ax_b.iter()).enumerate() {
        let extent_a = h_a[0] * ax_a[0].dot(u).abs() + h_a[1] * ax_a[1].dot(u).abs();
        let extent_b = h_b[0] * ax_b[0].dot(u).abs() + h_b[1] * ax_b[1].dot(u).abs();
        let sep = d.dot(u).abs() - extent_a - extent_b;
        if sep > 0.0 {
            return Vec::new();
        }
        // Strict > keeps A-owned axes on exact ties: deterministic for the
        // aligned case where A and B share axes.
        if sep > best_sep {
            best_sep = sep;
            best = idx;
        }
    }

    let a_owns = best < 2;
    let u = if a_owns { ax_a[best] } else { ax_b[best - 2] };
    // Axis oriented from A toward B.
    let n_ab = if d.dot(u) >= 0.0 { u } else { u * -1.0 };

    // Reference box owns the axis; its outward face normal points at the
    // incident box.
    let (rf, inc, ref_normal) = if a_owns {
        (a, b, n_ab)
    } else {
        (b, a, n_ab * -1.0)
    };
    let (ax_ref, h_ref) = (axes_of(rf), halves_of(rf));
    let (ax_inc, h_inc) = (axes_of(inc), halves_of(inc));

    // Incident face: the face of `inc` whose outward normal is most opposed
    // to the reference normal.
    let mut inc_axis = 0;
    let mut inc_sign = 1.0;
    let mut most_opposed = f64::INFINITY;
    for k in 0..2 {
        for sign in [1.0, -1.0] {
            let dot = (ax_inc[k] * sign).dot(ref_normal);
            if dot < most_opposed {
                most_opposed = dot;
                inc_axis = k;
                inc_sign = sign;
            }
        }
    }
    let face_n = ax_inc[inc_axis] * inc_sign;
    let tangent_axis = 1 - inc_axis;
    let face_center = inc.pos + face_n * h_inc[inc_axis];
    let t = ax_inc[tangent_axis];
    let mut p0 = face_center + t * h_inc[tangent_axis];
    let mut p1 = face_center - t * h_inc[tangent_axis];

    // Clip the incident segment against the reference face's side planes.
    let ref_axis = if a_owns { best } else { best - 2 };
    let side_axis = 1 - ref_axis;
    let s = ax_ref[side_axis];
    let side_half = h_ref[side_axis];
    for (dir, offset) in [(s, side_half), (s * -1.0, side_half)] {
        // Keep points with (p - rf.pos)·dir <= offset.
        let d0 = (p0 - rf.pos).dot(dir) - offset;
        let d1 = (p1 - rf.pos).dot(dir) - offset;
        if d0 > 0.0 && d1 > 0.0 {
            return Vec::new();
        }
        if d0 > 0.0 {
            p0 = p0 + (p1 - p0) * (d0 / (d0 - d1));
        } else if d1 > 0.0 {
            p1 = p1 + (p0 - p1) * (d1 / (d1 - d0));
        }
    }

    // Keep clipped points at or below the reference face.
    let face_point = rf.pos + ref_normal * h_ref[ref_axis];
    let push_a = n_ab * -1.0;
    let mut out = Vec::new();
    for p in [p0, p1] {
        let sep = (p - face_point).dot(ref_normal);
        if sep <= 0.0 {
            out.push((p, push_a, -sep));
        }
    }
    out
}

pub struct ContactWorld {
    pub bodies: Vec<ContactBody>,
    /// Accumulated normal impulse per body pair from the LAST step —
    /// divide by dt for the contact force (C3's metric).
    pub last_normal_impulse: Vec<((usize, usize), f64)>,
}

impl ContactWorld {
    pub fn new(bodies: Vec<ContactBody>) -> Self {
        ContactWorld {
            bodies,
            last_normal_impulse: Vec::new(),
        }
    }

    /// One world tick: apply external forces, resolve contacts, integrate.
    pub fn step(&mut self, forces: &[Vec2], dt: f64) {
        for (i, body) in self.bodies.iter_mut().enumerate() {
            let f = forces.get(i).copied().unwrap_or(Vec2::ZERO);
            body.vel = body.vel + f * (body.inv_m * dt);
        }

        // Contact generation per pair: SAT manifold, up to two points.
        let mut raw: Vec<(usize, usize, Vec2, Vec2, f64)> = Vec::new();
        for i in 0..self.bodies.len() {
            for j in (i + 1)..self.bodies.len() {
                let (a, b) = (&self.bodies[i], &self.bodies[j]);
                if a.inv_m == 0.0 && b.inv_m == 0.0 {
                    continue;
                }
                for (point, normal, depth) in collide_boxes(a, b) {
                    raw.push((i, j, point, normal, depth));
                }
            }
        }

        // Presolve.
        let mut contacts: Vec<Contact> = raw
            .into_iter()
            .map(|(ia, ib, point, normal, depth)| {
                let (a, b) = (&self.bodies[ia], &self.bodies[ib]);
                let ra = point - a.pos;
                let rb = point - b.pos;
                let rn_a = cross(ra, normal);
                let rn_b = cross(rb, normal);
                let mass_n = 1.0
                    / (a.inv_m + b.inv_m + rn_a * rn_a * a.inv_i + rn_b * rn_b * b.inv_i);
                let t = perp(normal);
                let rt_a = cross(ra, t);
                let rt_b = cross(rb, t);
                let mass_t = 1.0
                    / (a.inv_m + b.inv_m + rt_a * rt_a * a.inv_i + rt_b * rt_b * b.inv_i);
                let vn0 = (a.velocity_at(point) - b.velocity_at(point)).dot(normal);
                let e = a.restitution.max(b.restitution);
                // Threshold gates restitution; it does not shave the target.
                let restitution_bias = if -vn0 > RESTITUTION_THRESHOLD {
                    e * -vn0
                } else {
                    0.0
                };
                let baumgarte_bias =
                    BAUMGARTE / dt * (depth - PENETRATION_SLOP).max(0.0);
                Contact {
                    a: ia,
                    b: ib,
                    point,
                    normal,
                    mass_n,
                    mass_t,
                    bias: restitution_bias.max(baumgarte_bias),
                    mu: (a.friction * b.friction).sqrt(),
                    acc_n: 0.0,
                    acc_t: 0.0,
                }
            })
            .collect();

        // Sequential impulses.
        for _ in 0..SOLVER_ITERATIONS {
            for c in contacts.iter_mut() {
                let (va, vb) = (
                    self.bodies[c.a].velocity_at(c.point),
                    self.bodies[c.b].velocity_at(c.point),
                );
                let vrel = va - vb;
                let vn = vrel.dot(c.normal);
                let dl = -(vn - c.bias) * c.mass_n;
                let new_acc = (c.acc_n + dl).max(0.0);
                let dl = new_acc - c.acc_n;
                c.acc_n = new_acc;
                self.apply(c.a, c.b, c.point, c.normal * dl);

                let t = perp(c.normal);
                let (va, vb) = (
                    self.bodies[c.a].velocity_at(c.point),
                    self.bodies[c.b].velocity_at(c.point),
                );
                let vt = (va - vb).dot(t);
                let dl_t = -vt * c.mass_t;
                let max_t = c.mu * c.acc_n;
                let new_acc_t = (c.acc_t + dl_t).clamp(-max_t, max_t);
                let dl_t = new_acc_t - c.acc_t;
                c.acc_t = new_acc_t;
                self.apply(c.a, c.b, c.point, t * dl_t);
            }
        }

        // Force bookkeeping for the shove/push metric.
        self.last_normal_impulse.clear();
        for c in &contacts {
            let key = (c.a.min(c.b), c.a.max(c.b));
            match self.last_normal_impulse.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => *v += c.acc_n,
                None => self.last_normal_impulse.push((key, c.acc_n)),
            }
        }
        for body in self.bodies.iter_mut() {
            body.pos = body.pos + body.vel * dt;
            body.heading += body.omega * dt;
        }
    }

    fn apply(&mut self, ia: usize, ib: usize, point: Vec2, impulse: Vec2) {
        {
            let a = &mut self.bodies[ia];
            let ra = point - a.pos;
            a.vel = a.vel + impulse * a.inv_m;
            a.omega += cross(ra, impulse) * a.inv_i;
        }
        {
            let b = &mut self.bodies[ib];
            let rb = point - b.pos;
            b.vel = b.vel - impulse * b.inv_m;
            b.omega -= cross(rb, impulse) * b.inv_i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WORLD_DT;

    #[test]
    fn head_on_wall_impact_restitutes() {
        let mut ball = ContactBody::new_box(0.15, 0.05, 0.05, Vec2::new(0.0, 0.0), 0.0);
        ball.vel = Vec2::new(1.0, 0.0);
        ball.restitution = 0.5;
        let mut wall = ContactBody::fixed_box(0.05, 1.0, Vec2::new(0.2, 0.0), 0.0);
        wall.restitution = 0.5;
        let mut w = ContactWorld::new(vec![ball, wall]);
        for _ in 0..(0.5 / WORLD_DT) as u64 {
            w.step(&[], WORLD_DT);
        }
        let v = w.bodies[0].vel.x;
        assert!(
            (v + 0.5).abs() < 0.02,
            "expected rebound at ~-0.5 m/s, got {v}"
        );
        assert!(w.bodies[0].omega.abs() < 1e-6, "head-on hit must not spin");
    }

    #[test]
    fn two_body_collision_conserves_momentum() {
        let mut a = ContactBody::new_box(0.15, 0.05, 0.05, Vec2::new(-0.2, 0.01), 0.0);
        a.vel = Vec2::new(1.5, 0.0);
        a.restitution = 0.3;
        a.friction = 0.3;
        let mut b = ContactBody::new_box(0.15, 0.05, 0.05, Vec2::new(0.0, -0.01), 0.0);
        b.restitution = 0.3;
        b.friction = 0.3;
        let p0 = a.vel * (1.0 / a.inv_m) + b.vel * (1.0 / b.inv_m);
        let ke0 = 0.5 / a.inv_m * a.vel.dot(a.vel);
        let mut w = ContactWorld::new(vec![a, b]);
        for _ in 0..(0.5 / WORLD_DT) as u64 {
            w.step(&[], WORLD_DT);
        }
        let p1 = w.bodies[0].vel * (1.0 / w.bodies[0].inv_m)
            + w.bodies[1].vel * (1.0 / w.bodies[1].inv_m);
        assert!((p1 - p0).norm() / p0.norm() < 1e-6, "momentum drifted");
        let ke1 = 0.5 / w.bodies[0].inv_m * w.bodies[0].vel.dot(w.bodies[0].vel)
            + 0.5 / w.bodies[1].inv_m * w.bodies[1].vel.dot(w.bodies[1].vel)
            + 0.5 / (w.bodies[0].inv_i) * w.bodies[0].omega * w.bodies[0].omega
            + 0.5 / (w.bodies[1].inv_i) * w.bodies[1].omega * w.bodies[1].omega;
        assert!(ke1 <= ke0 * 1.001, "solver injected energy: {ke0} -> {ke1}");
    }
}
