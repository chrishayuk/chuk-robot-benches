//! Browser embedding of the REAL robowire E-check engine (design-servers
//! discipline: one check codebase — CLI, editor, and future MCP server all
//! link these same functions). Plain buffer ABI, no bindgen: JS allocates,
//! writes UTF-8 JSON, calls, reads the report back.

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
