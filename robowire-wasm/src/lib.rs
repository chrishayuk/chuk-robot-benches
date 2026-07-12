//! Browser embedding of the REAL robowire E-check engine (design-servers
//! discipline: one check codebase — CLI, editor, and future MCP server all
//! link these same functions). Plain buffer ABI, no bindgen: JS allocates,
//! writes UTF-8 JSON, calls, reads the report back.

use robosim::{run_state, RunInputs};
use robowire::catalogue::{ElecCatalogue, ElecPart};
use robowire::{run_checks, Netlist};
use std::sync::Mutex;

static OUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

#[no_mangle]
pub extern "C" fn wasm_alloc(n: usize) -> *mut u8 {
    let mut v: Vec<u8> = Vec::with_capacity(n);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p` must come from `wasm_alloc(n)` and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn wasm_free(p: *mut u8, n: usize) {
    drop(Vec::from_raw_parts(p, 0, n));
}

fn set_out(s: String) {
    *OUT.lock().unwrap() = s.into_bytes();
}

/// Run the full E-check set. Inputs: netlist JSON, parts catalogue as a JSON
/// array of part objects. Output (via out_ptr/out_len): {"checks":[...]} or
/// {"error": "..."}. Returns 0 on success, 1 on error.
///
/// # Safety
/// Pointers must reference `len` bytes of valid UTF-8 written by the caller.
#[no_mangle]
pub unsafe extern "C" fn run_checks_json(
    nl_ptr: *const u8,
    nl_len: usize,
    parts_ptr: *const u8,
    parts_len: usize,
) -> i32 {
    let nl_bytes = std::slice::from_raw_parts(nl_ptr, nl_len);
    let parts_bytes = std::slice::from_raw_parts(parts_ptr, parts_len);
    let netlist: Netlist = match serde_json::from_slice(nl_bytes) {
        Ok(n) => n,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("netlist: {e}")).unwrap()));
            return 1;
        }
    };
    let parts: Vec<ElecPart> = match serde_json::from_slice(parts_bytes) {
        Ok(p) => p,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("parts: {e}")).unwrap()));
            return 1;
        }
    };
    let cat = ElecCatalogue::from_values(parts);
    match run_checks(&netlist, &cat) {
        Ok(checks) => {
            set_out(serde_json::json!({ "checks": checks }).to_string());
            0
        }
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&e).unwrap()));
            1
        }
    }
}

/// Compute interactive run-mode state (specs/robowire.md §3a): which nets are
/// energized and how each instance should render, given the current
/// switch/button/throttle/sensor inputs. Inputs: netlist JSON, parts
/// catalogue as a JSON array, RunInputs JSON (an empty/unparseable buffer
/// defaults to all-off, so the caller can pass `{}` on first entry into run
/// mode). Output (via out_ptr/out_len): `{"nets":{...},"instances":{...}}`
/// or `{"error": "..."}`. Returns 0 on success, 1 on error.
///
/// # Safety
/// Pointers must reference `len` bytes of valid UTF-8 written by the caller.
#[no_mangle]
pub unsafe extern "C" fn run_state_json(
    nl_ptr: *const u8,
    nl_len: usize,
    parts_ptr: *const u8,
    parts_len: usize,
    inputs_ptr: *const u8,
    inputs_len: usize,
) -> i32 {
    let nl_bytes = std::slice::from_raw_parts(nl_ptr, nl_len);
    let parts_bytes = std::slice::from_raw_parts(parts_ptr, parts_len);
    let inputs_bytes = std::slice::from_raw_parts(inputs_ptr, inputs_len);
    let netlist: Netlist = match serde_json::from_slice(nl_bytes) {
        Ok(n) => n,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("netlist: {e}")).unwrap()));
            return 1;
        }
    };
    let parts: Vec<ElecPart> = match serde_json::from_slice(parts_bytes) {
        Ok(p) => p,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("parts: {e}")).unwrap()));
            return 1;
        }
    };
    let inputs: RunInputs = serde_json::from_slice(inputs_bytes).unwrap_or_default();
    let cat = ElecCatalogue::from_values(parts);
    match run_state(&netlist, &cat, &inputs) {
        Ok(state) => {
            set_out(serde_json::to_string(&state).unwrap());
            0
        }
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&e).unwrap()));
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn out_ptr() -> *const u8 {
    OUT.lock().unwrap().as_ptr()
}

#[no_mangle]
pub extern "C" fn out_len() -> usize {
    OUT.lock().unwrap().len()
}

/// Plain-English descriptions for the current netlist (pins + nets), from
/// the SAME prose generator the native tools use. Output via out_ptr/len.
///
/// # Safety
/// Pointers must reference `len` bytes of valid UTF-8 written by the caller.
#[no_mangle]
pub unsafe extern "C" fn describe_json(
    nl_ptr: *const u8,
    nl_len: usize,
    parts_ptr: *const u8,
    parts_len: usize,
) -> i32 {
    let nl_bytes = std::slice::from_raw_parts(nl_ptr, nl_len);
    let parts_bytes = std::slice::from_raw_parts(parts_ptr, parts_len);
    let netlist: Netlist = match serde_json::from_slice(nl_bytes) {
        Ok(n) => n,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("netlist: {e}")).unwrap()));
            return 1;
        }
    };
    let parts: Vec<ElecPart> = match serde_json::from_slice(parts_bytes) {
        Ok(p) => p,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("parts: {e}")).unwrap()));
            return 1;
        }
    };
    let cat = ElecCatalogue::from_values(parts);
    set_out(robowire::prose::describe(&netlist, &cat).to_string());
    0
}

/// Plain-English what/why/fix teaching content for a check code
/// (specs/codes.md) — independent of any netlist, the same content
/// `robowire explain-error <CODE>` prints natively (`robowire::teach`).
/// Output via out_ptr/len: `{"code","what","why","fix"}` or `{"error"}`.
/// Returns 0 if the code is known, 1 otherwise.
///
/// # Safety
/// `code_ptr` must reference `code_len` bytes of valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn explain_error_json(code_ptr: *const u8, code_len: usize) -> i32 {
    let code_bytes = std::slice::from_raw_parts(code_ptr, code_len);
    let code = match std::str::from_utf8(code_bytes) {
        Ok(s) => s,
        Err(e) => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("code: {e}")).unwrap()));
            return 1;
        }
    };
    match robowire::teach::explain_error(code) {
        Some(e) => {
            set_out(serde_json::json!({ "code": e.code, "what": e.what, "why": e.why, "fix": e.fix }).to_string());
            0
        }
        None => {
            set_out(format!("{{\"error\":{}}}", serde_json::to_string(&format!("no explanation for '{code}'")).unwrap()));
            1
        }
    }
}
