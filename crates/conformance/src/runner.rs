//! Reading `features/suites/*.feature` for feature-first conformance.

use std::collections::BTreeSet;
use std::path::Path;

/// One Gherkin scenario and the conformance ID it discharges.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Scenario {
    /// Conformance ID from the first scenario tag.
    pub id: String,
    /// Honesty level from the second scenario tag.
    pub level: String,
    /// One-line scenario statement.
    pub statement: String,
    /// Feature-suite file stem.
    pub suite: String,
    /// Ordered Given/When/Then steps.
    pub steps: Vec<String>,
}

/// Parsed contents of one feature-suite directory.
#[derive(Clone, Debug, Default)]
pub struct SuiteReport {
    /// Every scenario found.
    pub scenarios: Vec<Scenario>,
    /// Number of feature files read.
    pub files: usize,
}

/// Parses every `.feature` file in `dir`.
///
/// The repository uses a deliberately small Gherkin subset. Scenarios are
/// readable contracts; ordinary Rust tests execute them, and the meta-gate ties
/// register rows, scenarios, and test names together.
pub fn scenarios_in(dir: &Path) -> std::io::Result<SuiteReport> {
    let mut report = SuiteReport::default();
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let path = entry.path();
        if !path
            .extension()
            .is_some_and(|extension| extension == "feature")
        {
            continue;
        }
        report.files += 1;
        let suite = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path)?;

        let mut pending_tags: Vec<String> = Vec::new();
        let mut current: Option<Scenario> = None;
        for raw in text.lines() {
            let line = raw.trim();
            if line.starts_with('@') {
                pending_tags = line
                    .split_whitespace()
                    .map(|tag| tag.trim_start_matches('@').to_string())
                    .collect();
            } else if let Some(rest) = line.strip_prefix("Scenario:") {
                if let Some(done) = current.take() {
                    report.scenarios.push(done);
                }
                current = Some(Scenario {
                    id: pending_tags.first().cloned().unwrap_or_default(),
                    level: pending_tags.get(1).cloned().unwrap_or_default(),
                    statement: rest.trim().to_string(),
                    suite: suite.clone(),
                    steps: Vec::new(),
                });
                pending_tags.clear();
            } else if let Some(scenario) = current.as_mut() {
                for keyword in ["Given ", "When ", "Then ", "And ", "But "] {
                    if let Some(step) = line.strip_prefix(keyword) {
                        scenario.steps.push(format!("{keyword}{step}"));
                        break;
                    }
                }
            }
        }
        if let Some(done) = current.take() {
            report.scenarios.push(done);
        }
    }

    Ok(report)
}

impl SuiteReport {
    /// Returns the unique conformance IDs represented by the suites.
    pub fn ids(&self) -> BTreeSet<&str> {
        self.scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect()
    }
}
