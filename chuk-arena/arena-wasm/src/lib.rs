//! arena-wasm — browser embedding of the REAL plant + cell + bench crates
//! (compiled to wasm32-unknown-unknown) so the bench console tweaks and
//! visualises the same code that produces native bench reports. This is a
//! fast-mode analog per SPEC §7: same source, different build — flagged
//! provisional until the executor differential job stands.
//!
//! ABI kept to plain f64/u32 so no bindgen tooling is needed: results land
//! in a shared f64 buffer read out via buf_ptr()/buf_len().

use std::sync::Mutex;

use arena_bench::{naive_certified, run_brake_traced, BrakeKernel};
use arena_cells::ActiveBrakeCell;
use arena_plant::dynamic::RigidBotSpec;

static BUF: Mutex<Vec<f64>> = Mutex::new(Vec::new());
static SPEC: Mutex<Option<RigidBotSpec>> = Mutex::new(None);

fn with_spec<T>(f: impl FnOnce(&RigidBotSpec) -> T) -> T {
    let mut guard = SPEC.lock().unwrap();
    if guard.is_none() {
        *guard = Some(RigidBotSpec::default_m1());
    }
    f(guard.as_ref().unwrap())
}

fn kernel_of(k: u32) -> BrakeKernel {
    if k == 0 {
        BrakeKernel::NaiveCoast
    } else {
        BrakeKernel::ActiveAligned
    }
}

/// Override the design vector's swept scalars (wheel layout stays fixed).
#[no_mangle]
pub extern "C" fn set_design(
    mass_kg: f64,
    stall_force: f64,
    no_load_speed: f64,
    stall_current: f64,
    v_nominal: f64,
    r_internal: f64,
    mu_kinetic_ratio: f64,
    c_rr: f64,
) {
    let mut s = RigidBotSpec::default_m1();
    s.mass_kg = mass_kg;
    s.yaw_inertia = mass_kg
        * (s.footprint_half_w * s.footprint_half_w
            + s.footprint_half_l * s.footprint_half_l)
        / 3.0;
    s.motor.stall_force = stall_force;
    s.motor.no_load_speed = no_load_speed;
    s.motor.stall_current = stall_current;
    s.battery.v_nominal = v_nominal;
    s.battery.r_internal = r_internal;
    s.mu_kinetic_ratio = mu_kinetic_ratio;
    s.c_rr = c_rr;
    *SPEC.lock().unwrap() = Some(s);
}

/// Certified stopping distance the given kernel promises for (v0, omega0).
#[no_mangle]
pub extern "C" fn cert_distance(kernel: u32, v0: f64, omega0: f64) -> f64 {
    with_spec(|s| match kernel_of(kernel) {
        BrakeKernel::NaiveCoast => naive_certified(s, v0),
        BrakeKernel::ActiveAligned => {
            ActiveBrakeCell::new(s).certified_stop_distance(v0, omega0)
        }
    })
}

/// Run a braking scenario; returns achieved max excursion (m). With
/// trace != 0, fills the buffer with [t, x, y, heading, speed] rows at 100 Hz.
#[no_mangle]
pub extern "C" fn run_brake(
    kernel: u32,
    v0: f64,
    slip_rad: f64,
    mu: f64,
    omega0: f64,
    trace: u32,
) -> f64 {
    with_spec(|s| {
        let cell = ActiveBrakeCell::new(s);
        let mut sink = Vec::new();
        let stride = if trace == 0 { 0 } else { 80 }; // 8 kHz / 80 = 100 Hz
        let achieved = run_brake_traced(
            s,
            kernel_of(kernel),
            &cell,
            v0,
            slip_rad,
            mu,
            omega0,
            stride,
            &mut sink,
        );
        let mut buf = BUF.lock().unwrap();
        buf.clear();
        for row in sink {
            buf.extend_from_slice(&row);
        }
        achieved
    })
}

#[no_mangle]
pub extern "C" fn buf_ptr() -> *const f64 {
    BUF.lock().unwrap().as_ptr()
}

#[no_mangle]
pub extern "C" fn buf_len() -> u32 {
    BUF.lock().unwrap().len() as u32
}
