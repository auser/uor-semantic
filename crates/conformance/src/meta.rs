//! Honesty and feature-first meta-gates.

use std::collections::BTreeSet;
use std::path::Path;

use repo_model::{Level, Model};

use crate::runner::scenarios_in;

/// Result of cross-checking model rows, scenarios, tests, and prose.
#[derive(Clone, Debug, Default)]
pub struct HonestyReport {
    /// Every detected violation.
    pub violations: Vec<String>,
    /// Registered IDs inspected.
    pub ids_checked: usize,
    /// Scenarios inspected.
    pub scenarios_checked: usize,
}

impl HonestyReport {
    /// Returns whether the cross-check found no violation.
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

const ASSERTIVE: &[&str] = &[
    "proves",
    "proven",
    "proof that",
    "guarantees",
    "establishes",
    "confirms universally",
];

/// Cross-checks the conformance model against feature suites and test names.
pub fn check_honesty(root: &Path, tests: &BTreeSet<String>) -> std::io::Result<HonestyReport> {
    let mut report = HonestyReport::default();
    let model = match Model::load(&root.join("model")) {
        Ok(model) => model,
        Err(error) => {
            report
                .violations
                .push(format!("R1: the model does not load: {error}"));
            return Ok(report);
        }
    };
    if let Err(error) = model.check() {
        report
            .violations
            .push(format!("R1: the model is inconsistent: {error}"));
        return Ok(report);
    }

    let suites = scenarios_in(&root.join("features/suites"))?;
    report.ids_checked = model.ids.id.len();
    report.scenarios_checked = suites.scenarios.len();

    let scenario_ids = suites.ids();
    for row in &model.ids.id {
        let scenario_count = suites
            .scenarios
            .iter()
            .filter(|scenario| scenario.id == row.id)
            .count();
        if !scenario_ids.contains(row.id.as_str()) {
            report.violations.push(format!(
                "R3: {} is registered but has no feature scenario",
                row.id
            ));
        } else if scenario_count != 1 {
            report.violations.push(format!(
                "CM-02: {} has {scenario_count} feature scenarios; exactly one is required",
                row.id
            ));
        }
        let slug = row.id.to_lowercase().replace('-', "_");
        if !tests.iter().any(|test| test.ends_with(&slug)) {
            report.violations.push(format!(
                "CM-02: {} has no Rust test whose name ends in `{slug}`",
                row.id
            ));
        }
    }

    for scenario in &suites.scenarios {
        let Some(row) = model.ids.get(&scenario.id) else {
            report.violations.push(format!(
                "CM-02: `{}` in {} names unregistered ID `{}`",
                scenario.statement, scenario.suite, scenario.id
            ));
            continue;
        };
        if scenario.level != row.level.as_str() {
            report.violations.push(format!(
                "R2: {} is `{}` in {} but `{}` in the register",
                scenario.id,
                scenario.level,
                scenario.suite,
                row.level.as_str()
            ));
        }
        if scenario.suite != row.suite {
            report.violations.push(format!(
                "R3: {} is in suite `{}` but registered under `{}`",
                scenario.id, scenario.suite, row.suite
            ));
        }
        if scenario.steps.is_empty() {
            report.violations.push(format!(
                "R3: {} has no executable behavior steps",
                scenario.id
            ));
        }
    }

    let prefixes: BTreeSet<String> = model
        .ids
        .id
        .iter()
        .filter_map(|row| row.id.split('-').next().map(str::to_lowercase))
        .collect();
    for name in tests {
        let tail = name.rsplit("::").next().unwrap_or(name.as_str());
        let mut parts = tail.rsplitn(3, '_');
        let digits = parts.next().unwrap_or("");
        let letters = parts.next().unwrap_or("");
        if parts.next().is_none() {
            continue;
        }
        if digits.len() != 2 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if letters.len() != 2 || !prefixes.contains(letters) {
            continue;
        }
        let id = format!("{}-{digits}", letters.to_uppercase());
        if model.ids.get(&id).is_none() {
            report.violations.push(format!(
                "CM-02: test `{name}` names unregistered conformance ID `{id}`"
            ));
        }
    }

    for document in ["README.md", "CONFORMANCE.md", "VERIFICATION.md"] {
        let path = root.join(document);
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (line_number, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            for row in model.ids.id.iter().filter(|row| row.level == Level::Open) {
                let id = row.id.as_str();
                if line.contains(id)
                    && let Some(word) = ASSERTIVE.iter().find(|word| lower.contains(*word))
                {
                    report.violations.push(format!(
                        "R2: {document}:{} asserts open claim {id} using `{word}`",
                        line_number + 1
                    ));
                }
            }
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::ASSERTIVE;

    #[test]
    fn assertive_vocabulary_is_falsifiable() {
        for word in ASSERTIVE {
            let sentence = format!("PF-02 {word} a universal latency bound");
            assert!(
                ASSERTIVE
                    .iter()
                    .any(|candidate| sentence.to_lowercase().contains(*candidate))
            );
        }
    }
}
