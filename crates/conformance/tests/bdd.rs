//! Register, Gherkin, test-name, and honesty meta-gate.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use repo_conformance::{check_honesty, scenarios_in};
use repo_model::{Level, Model};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/conformance is two levels below the repository root")
        .to_path_buf()
}

fn workspace_test_names(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut stack = vec![root.join("crates"), root.join("xtask")];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let Ok(text) = std::fs::read_to_string(path) else {
                    continue;
                };
                let mut armed = false;
                for line in text.lines() {
                    let line = line.trim();
                    if line == "#[test]" {
                        armed = true;
                    } else if armed {
                        if let Some(rest) = line.strip_prefix("fn ") {
                            let name: String = rest
                                .chars()
                                .take_while(|character| {
                                    character.is_alphanumeric() || *character == '_'
                                })
                                .collect();
                            if !name.is_empty() {
                                names.insert(name);
                            }
                            armed = false;
                        } else if !line.starts_with('#') && !line.is_empty() {
                            armed = false;
                        }
                    }
                }
            }
        }
    }
    names
}

#[test]
fn every_id_has_a_scenario_and_a_test_cm_02() {
    let root = root();
    let tests = workspace_test_names(&root);
    assert!(!tests.is_empty(), "the test-name inventory must be armed");

    let report = check_honesty(&root, &tests).expect("the meta-gate runs");
    assert!(
        report.is_clean(),
        "the honesty meta-gate failed:\n\n{}",
        report.violations.join("\n\n")
    );
    assert!(
        report.ids_checked >= 10,
        "the populated register is inspected"
    );
    assert_eq!(report.ids_checked, report.scenarios_checked);
}

#[test]
fn no_scenario_is_pending_cm_02() {
    let model = Model::load(&root().join("model")).expect("model loads");
    let suites = scenarios_in(&root().join("features/suites")).expect("suites read");
    assert!(!model.ids.id.is_empty());
    assert!(suites.files >= 1);
    for scenario in &suites.scenarios {
        assert!(!scenario.steps.is_empty(), "{} has no steps", scenario.id);
        for step in &scenario.steps {
            let lower = step.to_lowercase();
            assert!(!lower.contains("pending"));
            assert!(!lower.contains("todo"));
        }
    }
}

#[test]
fn every_some_true_claim_cites_an_authority_cm_03() {
    let model = Model::load(&root().join("model")).expect("model loads");
    model.check().expect("the model is consistent");

    for claim in &model.ledger.claim {
        if claim.level != Level::SomeTrue {
            continue;
        }
        let authority_id = claim
            .authority
            .as_ref()
            .expect("a some-true claim names an authority");
        let authority = model
            .authorities
            .authority
            .iter()
            .find(|authority| &authority.id == authority_id)
            .expect("cited authority exists");
        assert!(!authority.citation.trim().is_empty());
        assert!(authority.checksum != "none" || !authority.checksum_reason.trim().is_empty());
    }
}

#[test]
fn the_meta_gate_is_falsifiable_cm_02() {
    let root = root();
    let empty = BTreeSet::new();
    let failed = check_honesty(&root, &empty).expect("the planted gate runs");
    assert!(!failed.is_clean());
    assert!(
        failed
            .violations
            .iter()
            .any(|violation| violation.contains("no Rust test"))
    );

    let full = workspace_test_names(&root);
    let clean = check_honesty(&root, &full).expect("the control gate runs");
    assert!(clean.is_clean());
}
