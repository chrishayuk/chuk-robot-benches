//! `explain_error` coverage: every code robowire's own `run_checks()`
//! actually emits must have a teaching explanation, or `robowire
//! explain-error <CODE>` silently fails on exactly the codes someone is
//! most likely to ask about (one they just triggered).

use robowire::catalogue::ElecCatalogue;
use robowire::{run_checks, Netlist};
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

#[test]
fn every_code_run_checks_can_emit_has_an_explanation() {
    let root = repo_root();
    let nl: Netlist =
        serde_json::from_slice(&std::fs::read(root.join("harness/mvp-wedge-harness.json")).unwrap()).unwrap();
    let cat = ElecCatalogue::load(&root.join("parts")).unwrap();
    let checks = run_checks(&nl, &cat).unwrap();
    assert!(!checks.is_empty());
    for c in &checks {
        assert!(
            robowire::teach::explain_error(&c.code).is_some(),
            "no explain-error content for {} — every emitted code needs one",
            c.code
        );
    }
}

#[test]
fn explain_error_is_case_insensitive_and_reports_unknown_codes_as_none() {
    assert!(robowire::teach::explain_error("e20").is_some());
    assert!(robowire::teach::explain_error("E20").is_some());
    assert!(robowire::teach::explain_error("E9999").is_none());
}

#[test]
fn every_explanation_has_non_empty_what_why_fix() {
    for code in [
        "E01", "E02", "E03", "E04", "E10", "E11", "E20", "E21", "E30", "E31", "E32", "E33", "E40", "E41", "E43",
        "E44", "E45",
    ] {
        let e = robowire::teach::explain_error(code).unwrap_or_else(|| panic!("missing {code}"));
        assert!(!e.what.is_empty() && !e.why.is_empty() && !e.fix.is_empty(), "{code} has an empty field");
    }
}
